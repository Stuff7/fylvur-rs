extern crate ffmpeg_next as ffmpeg;

use std::fmt::Display;
use std::fmt::Debug;

use ffmpeg::Rescale;
use ffmpeg::rescale;
use ffmpeg::codec::context::Context as CodecCtx;
use ffmpeg::decoder;
use ffmpeg::format;
use ffmpeg::format::context::Input as AVFormatContext;
use ffmpeg::packet::side_data;
use ffmpeg::media::Type;
use ffmpeg::software::scaling::{context::Context as ScalingCtx, flag::Flags};
use ffmpeg::util::frame::video::Video as VideoFrame;
use webp::Encoder;
use webp::WebPMemory;

use crate::{f, math};

const FFMPEG_RETRY_ERR: ffmpeg::Error = ffmpeg::Error::Other { errno: ffmpeg::error::EAGAIN };
const MAX_ATLAS_TILE_WIDTH: usize = 10;
const MAX_ATLAS_TILE_HEIGHT: usize = 10;
const ATLAS_TILE_WIDTH: usize = 80;
const ATLAS_TILE_HEIGHT: usize = 45;
const MAX_ATLAS_TILES: u32 = MAX_ATLAS_TILE_WIDTH as u32 * MAX_ATLAS_TILE_HEIGHT as u32;

pub fn init() -> Result<(), ffmpeg::Error> {
  ffmpeg::init()
}

/// Returns 10x10 webp atlas with an 80x45 tile for every second of the video
/// 
/// # Arguments
/// * `video_path` - Path to the video where the atlas will be made from
/// * `progress_secs` - Atlas page will contain the frame at this second
pub fn get_video_atlas(
  video_path: &String,
  page_i: u32,
  frame_step: u32,
) -> Result<WebPMemory, VideoError> {
  let mut av_format_ctx = match format::input(video_path) {
    Ok(av_format_ctx) => av_format_ctx,
    Err(err) => return Err((f!("Could not open file \"{video_path}\""), err).into())
  };

  let tile_index_start = page_i * MAX_ATLAS_TILES;
  let tile_index_end = std::cmp::min(
    (page_i + 1) * MAX_ATLAS_TILES, {
      let max_frames = get_duration(&av_format_ctx) as u32 / 1000 / frame_step;
      let modulo = max_frames % frame_step;
      max_frames + (frame_step - modulo)
    },
  );
  let tile_count = std::cmp::max(
    0,
    tile_index_end as i32 - tile_index_start as i32
  ) as usize;

  if tile_count == 0 {
    return Ok(encode_webp_from_frame(&VideoFrame::new(
      ffmpeg::format::Pixel::RGBA,
      ATLAS_TILE_WIDTH as u32,
      ATLAS_TILE_HEIGHT as u32,
    )))
  }

  let mut out_frame = VideoFrame::new(
    format::Pixel::RGBA,
    ATLAS_TILE_WIDTH as u32 * std::cmp::min(
      tile_count as u32,
      MAX_ATLAS_TILE_WIDTH as u32,
    ),
    ATLAS_TILE_HEIGHT as u32 * std::cmp::min(
      (tile_count as u32 / MAX_ATLAS_TILE_WIDTH as u32) + 1,
      MAX_ATLAS_TILE_HEIGHT as u32,
    ),
  );

  let out_width = out_frame.width();
  let out_data = out_frame.data_mut(0);

  let mut thumb_pos = 0;

  let frames = get_frame(
    &mut av_format_ctx,
    ATLAS_TILE_WIDTH as u32,
    SeekTime::Seconds(tile_index_start),
    tile_count,
    frame_step,
    Some(ATLAS_TILE_HEIGHT as u32),
  )?;
  for frame in frames {
    let frame_width = frame.width() as usize;
    let frame_height = frame.height() as usize;
    // Center image in tile when width/height is too small
    let blank_width_offset = (ATLAS_TILE_WIDTH - frame_width) / 2;
    let blank_height_offset = (ATLAS_TILE_HEIGHT - frame_height) / 2;
    let frame_area = frame_width * frame_height;
    let tile_x = thumb_pos % MAX_ATLAS_TILE_WIDTH;
    let tile_y = thumb_pos / MAX_ATLAS_TILE_HEIGHT;
    let tile_x_offset = tile_x * ATLAS_TILE_WIDTH + blank_width_offset;
    let tile_y_offset = tile_y * ATLAS_TILE_HEIGHT + blank_height_offset;

    let frame_data = frame.data(0);

    // Loop over every pixel in the frame
    for i in 0..frame_area {
      let x = i % frame_width;
      let y = i / frame_width;

      // Calculate frame pixel position in the atlas
      let dx = tile_x_offset + x;
      let dy = tile_y_offset + y;
      let di = (dx + out_width as usize * dy) * 4;

      // Copy RGBA values
      for color_i in 0..4 {
        out_data[di + color_i] = frame_data[i * 4 + color_i];
      }
    }
    thumb_pos += 1;
  }
  Ok(encode_webp_from_frame(&out_frame))
}

/// Returns webp image for the `video_path` at `frame_time` second
/// with `frame_width`, keeping the aspect ratio of the video
/// # Arguments
/// * `video_path` - Path to the video where the frame will be taken from
/// * `frame_width` - Width of the returned frame, pass 0 to use the video's width
/// * `frame_time` - Video time where the frame will come from, in seconds
/// 
/// # Examples
/// Saving webp file to disk
/// ```ignore
/// let thumbnail = video::get_frame(
/// String::from("/path/to/video/file"),
/// 0, // Use the video's width
/// 60 // Take frame at the 60 seconds mark,
/// ).expect("Could not get thumbnail");
/// 
/// let output_path = PathBuf::from(format!("./thumbnail.webp"));
/// 
/// std::fs::write(&output_path, &*thumbnail).expect("Could not save thumbnail");
/// ```
pub fn get_video_thumbnail(
  video_path: &String,
  thumbnail_width: u32,
  time_position: SeekTime,
) -> Result<WebPMemory, VideoError> {
  let mut av_format_ctx = match format::input(video_path) {
    Ok(av_format_ctx) => av_format_ctx,
    Err(err) => return Err((f!("Could not open file \"{video_path}\""), err).into())
  };
  let frame = get_frame(
    &mut av_format_ctx,
    thumbnail_width,
    time_position,
    1,
    1,
    None,
  )?;
  Ok(encode_webp_from_frame(&frame[0]))
}

pub fn get_frame(
  mut av_format_ctx: &mut AVFormatContext,
  frame_width: u32,
  frame_time: SeekTime,
  frame_count: usize,
  fps: u32,
  max_height: Option<u32>,
) -> Result<Vec<VideoFrame>, VideoError> {
  seek(&mut av_format_ctx, &frame_time)?;

  let video_stream = av_format_ctx
  .streams()
  .best(Type::Video)
  .ok_or(ffmpeg::Error::StreamNotFound)?;
  let video_stream_index = video_stream.index();

  // Find decoder
  let context_decoder = CodecCtx::from_parameters(video_stream.parameters())?;
  // Used to decode the packets and be able to receive frames
  let mut decoder = context_decoder.decoder().video()?;

  let frame_width = if frame_width == 0 {
    decoder.width()
  } else {
    frame_width
  };

  let matrix = get_display_matrix_values(&video_stream).ok();
  let rotation = match matrix {
    Some(transform) => {
      math::av_display_rotation_get(&transform)
      .unwrap_or_default() as i32
    }
    None => 0
  };

  // Allows to perform image rescaling and pixel format conversion
  let mut scaler = get_scaler(
    &decoder,
    frame_width,
    rotation,
    max_height,
  )?;
  let mut frames = Vec::new();
  let mut seconds: u32 = frame_time.into();

  while frames.len() < frame_count {
    for (stream, packet) in av_format_ctx.packets() {
      // Only send packet for video streams
      if stream.index() == video_stream_index {
        // Decode into a video frame
        if let Err(err) = decoder.send_packet(&packet) {
          if err != FFMPEG_RETRY_ERR {
            return Err(("Error sending packet", err).into())
          }
        }
        // Receive the video frame and do format/scale/rotation transformations
        match decode_frame(&mut decoder, matrix, rotation, &mut scaler) {
          Ok(frame) => {
            frames.push(frame);
            break
          }
          Err(err) => {
            if err != FFMPEG_RETRY_ERR {
              return Err(("Error receiving frame", err).into())
            }
          }
        }
      }
    }
    seconds += fps;
    seek_seconds(&mut av_format_ctx, seconds)?;
  }

  // Signal end of stream on encoding success
  decoder.send_eof()?;
  // Receive all the frames for the eof signal
  while decoder.receive_frame(&mut VideoFrame::empty()).is_ok() {}
  Ok(frames)
}

fn get_display_matrix_values(stream: &ffmpeg::Stream) -> Result<[i32; 9], String> {
  // Find rotation in video metadata
  let side_data = stream.side_data().find(|tag| {
    tag.kind() == side_data::Type::DisplayMatrix
  }).ok_or("Could not find display matrix in side data")?;
  // Convert bytes to i32 3x3 matrix
  math::parse_display_matrix(side_data.data())
}

fn get_scaler(
  decoder: &decoder::Video,
  frame_width: u32,
  rotation: i32,
  max_height: Option<u32>
) -> Result<ScalingCtx, ffmpeg::Error> {
  let (scaler_dst_w, scaler_dst_h) = if frame_width != decoder.width() &&
  rotation.abs() == 90 {
    let mut width = frame_width * decoder.width() / decoder.height() + 1;
    let mut height = frame_width;
    if let Some(max_height) = max_height {
      if height > max_height {
        height = max_height * height / width;
        width = max_height;
      }
    }
    (width, height)
  } else {
    let mut width = frame_width;
    let mut height = frame_width * decoder.height() / decoder.width();
    if let Some(max_height) = max_height {
      if height > max_height {
        width = max_height * width / height;
        height = max_height;
      }
    }
    (width, height)
  };

  ScalingCtx::get(
    decoder.format(),
    decoder.width(),
    decoder.height(),
    format::Pixel::RGBA,
    scaler_dst_w,
    scaler_dst_h,
    Flags::SINC,
  )
}

fn decode_frame(
  decoder: &mut decoder::Video,
  matrix: Option<[i32; 9]>,
  rotation: i32,
  scaler: &mut ScalingCtx,
) -> Result<VideoFrame, ffmpeg::Error> {
  let mut decoded = VideoFrame::empty();
  decoder.receive_frame(&mut decoded)?;

  let mut src_frame = VideoFrame::empty();
  // Convert to RGBA pixel format and resize
  scaler.run(&decoded, &mut src_frame)?;
  // Running the scaler can break images depending on the output size
  fix_img_data(&mut src_frame);

  if let Some(mut transform) = matrix {
    let (dst_width, dst_height) = if rotation.abs() == 90 {
      (src_frame.height(), src_frame.width())
    } else {(src_frame.width(), src_frame.height())};

    // |a b c|
    // |d e f|
    // |g h i|
    // h and g in the transform matrix indicate the width and height respectively.
    // It must be updated after scaling as it is required to rotate the image correctly
    if transform[6] != 0 {
      transform[6] = dst_width as i32 - 1;
    }
    if transform[7] != 0 {
      transform[7] = dst_height as i32 - 1;
    }

    // Create rotated empty frame
    let mut dst_frame = VideoFrame::new(
      src_frame.format(),
      dst_width,
      dst_height,
    );

    math::rotate_frame(
      &src_frame,
      &mut dst_frame,
      &transform,
    );

    return Ok(dst_frame)
  }
  return Ok(src_frame)
}

fn encode_webp_from_frame(frame: &VideoFrame) -> WebPMemory {
  let encoder = Encoder::from_rgba(
    frame.data(0),
    frame.width(),
    frame.height(),
  );
  let webp = encoder.encode(50.);
  webp
}

fn fix_img_data(frame: &mut VideoFrame) {
  let stride = frame.stride(0);
  let width: usize = frame.width() as usize;
  let height: usize = frame.height() as usize;
  let data = frame.data_mut(0);
  let byte_width = width * 4;
  let mut buffer = Vec::with_capacity(data.len());

  for line in 0..height {
    let begin = line * stride;
    let end = begin + byte_width;
    buffer.extend_from_slice(&data[begin..end]);
  }

  if buffer.len() < data.len() {
    buffer.extend_from_slice(&data[buffer.len()..data.len()]);
  }
  data.clone_from_slice(buffer.as_slice());
}

fn seek(
  mut video_stream: &mut AVFormatContext,
  seek_time: &SeekTime,
) -> Result<(), ffmpeg::Error> {
  match seek_time {
    SeekTime::Seconds(seconds) => seek_seconds(&mut video_stream, *seconds),
    SeekTime::Percentage(percentage) => {
      let duration = video_stream.duration();
      let position = (percentage * duration as f32) as i64;
      video_stream.seek(position, ..position)
    }
  }
}

fn seek_seconds(video_stream: &mut AVFormatContext, seconds: u32) -> Result<(), ffmpeg::Error> {
  let position = seconds.rescale((1, 1), rescale::TIME_BASE);
  video_stream.seek(position, ..position)
}

pub fn get_duration_from_path(video_path: &String) -> Result<i64, VideoError> {
  let av_format_ctx = match format::input(video_path) {
    Ok(av_format_ctx) => av_format_ctx,
    Err(err) => return Err((f!("Could not open file \"{video_path}\""), err).into())
  };

  Ok(get_duration(&av_format_ctx))
}

pub fn get_duration(av_format_ctx: &AVFormatContext) -> i64 {
  let time_base = ffmpeg::rescale::TIME_BASE.0 as f32 / ffmpeg::rescale::TIME_BASE.1 as f32;
  (av_format_ctx.duration() as f32 * time_base * 1000.) as i64
}

#[derive(Debug)]
pub enum SeekTime {
  Seconds(u32),
  Percentage(f32),
}

impl Into<u32> for SeekTime {
  fn into(self) -> u32 {
    use SeekTime::*;
    match self {
      Seconds(s) => s,
      Percentage(s) => s as u32,
    }
  }
}

#[derive(Debug)]
pub struct VideoError {
  message: String,
}

impl Display for VideoError {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    write!(f, "{}", self.message)
  }
}

impl<T: Display, E: Debug> From<(T, E)> for VideoError {
  fn from((message, err): (T, E)) -> Self {
    Self { message: f!("Video Error: {message}\n\n{err:?}") }
  }
}

impl From<ffmpeg::Error> for VideoError {
  fn from(err: ffmpeg::Error) -> Self {
    Self { message: f!("Video Error: {err:?}") }
  }
}
