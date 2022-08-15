extern crate ffmpeg_next as ffmpeg;

use std::fmt::Display;
use std::fmt::Debug;

use ffmpeg::Rescale;
use ffmpeg::rescale;
use ffmpeg::codec::context::Context as CodecCtx;
use ffmpeg::decoder;
use ffmpeg::format::{context, input, Pixel};
use ffmpeg::packet::side_data;
use ffmpeg::media::Type;
use ffmpeg::software::scaling::{context::Context as ScalingCtx, flag::Flags};
use ffmpeg::util::frame::video::Video;
use webp::Encoder;
use webp::WebPMemory;

use crate::{f, math};

const FFMPEG_RETRY_ERR: ffmpeg::Error = ffmpeg::Error::Other { errno: ffmpeg::error::EAGAIN };

pub fn init() -> Result<(), ffmpeg::Error> {
  ffmpeg::init()
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
/// ```
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
pub fn get_frame(
  video_path: String,
  frame_width: u32,
  frame_time: u32,
) -> Result<WebPMemory, VideoError> {
  let mut av_format_ctx = match input(&video_path) {
    Ok(av_format_ctx) => av_format_ctx,
    Err(err) => return Err((f!("Could not open file \"{video_path}\""), err).into())
  };
  if let Err(err) = seek(&mut av_format_ctx, frame_time) {
    return Err((f!("Failed to seek to {frame_time} in \"{video_path}\""), err).into())
  }

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

  for (stream, packet) in av_format_ctx.packets() {
    // Only send packet for video streams
    if stream.index() == video_stream_index {
      // Decode into a video frame
      decoder.send_packet(&packet)?;
      // Receive the video frame and encode as webp
      match encode_webp_from_decoded_frame(&mut decoder, frame_width, &stream) {
        Ok(webp_data) => {
          // Signal end of stream on encoding success
          decoder.send_eof()?;
          // Receive all the frames for the eof signal
          while decoder.receive_frame(&mut Video::empty()).is_ok() {}
          return Ok(webp_data)
        }
        Err(err) => {
          if err != FFMPEG_RETRY_ERR {
            return Err(("Error receiving frame", err).into())
          }
        }
      }
    }
  }

  Err(VideoError::new(f!("Could not find a valid video stream in \"{video_path}\"")))
}

fn get_display_matrix_values(stream: &ffmpeg::Stream) -> Result<[i32; 9], String> {
  // Find rotation in video metadata
  let side_data = stream.side_data().find(|tag| {
    tag.kind() == side_data::Type::DisplayMatrix
  }).ok_or("Could not find display matrix in side data")?;
  // Convert bytes to i32 3x3 matrix
  math::parse_display_matrix(side_data.data())
}

fn get_scaler_with_rotation(
  decoder: &decoder::Video,
  frame_width: u32,
  rotation: i32,
) -> Result<ScalingCtx, ffmpeg::Error> {
  let (scaler_dst_w, scaler_dst_h) = if frame_width != decoder.width() &&
  rotation.abs() == 90 {(
    frame_width * decoder.width() / decoder.height(),
    frame_width
  )} else {(
    frame_width,
    frame_width * decoder.height() / decoder.width(),
  )};

  ScalingCtx::get(
    decoder.format(),
    decoder.width(),
    decoder.height(),
    Pixel::RGBA,
    scaler_dst_w,
    scaler_dst_h,
    Flags::BILINEAR,
  )
}

fn encode_webp_from_decoded_frame(
  decoder: &mut decoder::Video,
  frame_width: u32,
  stream: &ffmpeg::Stream,
) -> Result<WebPMemory, ffmpeg::Error> {
  let mut decoded = Video::empty();

  let matrix = get_display_matrix_values(&stream).ok();
  let rotation = match matrix {
    Some(transform) => {
      math::av_display_rotation_get(&transform)
      .unwrap_or_default() as i32
    }
    None => 0
  };

  // Allows to perform image rescaling and pixel format conversion
  let mut scaler = get_scaler_with_rotation(
    &decoder,
    frame_width,
    rotation,
  )?;

  loop {
    decoder.receive_frame(&mut decoded)?;

    let mut src_frame = Video::empty();
    // Convert to RGBA pixel format and resize
    scaler.run(&decoded, &mut src_frame)?;

    if let Some(transform) = matrix {
      println!("MATRIX => {transform:?}");

      let (dst_width, dst_height) = if rotation.abs() == 90 {
        (src_frame.height(), src_frame.width())
      } else {(src_frame.width(), src_frame.height())};

      // Create rotated empty frame
      let mut dst_frame = Video::new(
        src_frame.format(),
        dst_width,
        dst_height,
      );

      let now = std::time::Instant::now();
      math::rotate_frame(
        &mut src_frame,
        &mut dst_frame,
        &transform,
      );
      println!("Rotated in {:?}", now.elapsed());

      return Ok(webp_from_frame(&mut dst_frame))
    }
    return Ok(webp_from_frame(&mut src_frame))
  }
}

fn webp_from_frame(mut frame: &mut Video) -> WebPMemory {
  fix_img_data(&mut frame);
  let encoder = Encoder::from_rgba(
    frame.data(0),
    frame.width(),
    frame.height(),
  );
  let now = std::time::Instant::now();
  let webp = encoder.encode(50.);
  println!("Encoded in {:?}", now.elapsed());
  webp
}

fn fix_img_data(frame: &mut Video) {
  let stride = frame.stride(0);
  let width: usize = frame.width() as usize;
  let height: usize = frame.height() as usize;
  let data = frame.data_mut(0);
  let byte_width = width * stride / width;
  let mut buffer = Vec::with_capacity(data.len());

  println!("\n\n\
    STRIDE: {stride:?}\n\
    BYTE_WIDTH: {byte_width:?}\n\
    WIDTH: {width:?}\n\
    HEIGHT: {height:?}\n\
    STRIDE / WIDTH: {}\n\
    WIDTH / HEIGHT: {}\n\
    HEIGHT / WIDTH: {}\n",
    stride as f32 / width as f32,
    width as f32 / height as f32,
    height as f32 / width as f32,
  );

  let mut line = 0;
  loop {
    let begin = line * stride;
    let end = begin + byte_width;
    if line < height || end > data.len() {
      break
    }
    buffer.extend_from_slice(&data[begin..end]);
    line += byte_width;
  }

  if buffer.len() < data.len() {
    buffer.extend_from_slice(&data[buffer.len()..data.len()]);
  }
  data.clone_from_slice(buffer.as_slice());
}

fn seek(video_stream: &mut context::Input, seconds: u32) -> Result<(), ffmpeg::Error> {
  let position = seconds.rescale((1, 1), rescale::TIME_BASE);
  video_stream.seek(position, ..position)
}

#[derive(Debug)]
pub struct VideoError {
  message: String,
}

impl VideoError {
  fn new<T: ToString>(message: T) -> Self {
    Self { message: message.to_string() }
  }
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
