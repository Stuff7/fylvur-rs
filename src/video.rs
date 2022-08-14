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
  // Allows to perform image rescaling and pixel format conversion
  let mut scaler = ScalingCtx::get(
    decoder.format(),
    decoder.width(),
    decoder.height(),
    Pixel::RGBA,
    frame_width,
    frame_width * decoder.height() / decoder.width(),
    Flags::BILINEAR,
  )?;

  for (stream, packet) in av_format_ctx.packets() {
    // Only send packet for video streams
    if stream.index() == video_stream_index {
      println!("Sending packet {video_stream_index}");
      // Decode into a video frame
      decoder.send_packet(&packet)?;
      // Receive the video frame and encode as webp
      match encode_webp_from_decoded_frame(&mut decoder, &mut scaler, &stream) {
        Ok(webp_data) => {
          // Signal end of stream on encoding success
          decoder.send_eof()?;
          // Receive all the frames for the eof signal
          while decoder.receive_frame(&mut Video::empty()).is_ok() {}
          return Ok(webp_data)
        }
        Err(err) => {
          if err == FFMPEG_RETRY_ERR {
            println!("Resource unavailable. Sending next packet...");
          } else {
            return Err(("Error receiving frame", err).into())
          }
        }
      }
    }
  }

  Err(VideoError::new(f!("Could not find a valid video stream in \"{video_path}\"")))
}

fn encode_webp_from_decoded_frame(
  decoder: &mut decoder::Video,
  scaler: &mut ScalingCtx,
  stream: &ffmpeg::Stream,
) -> Result<WebPMemory, ffmpeg::Error> {
  let mut decoded = Video::empty();
  loop {
    decoder.receive_frame(&mut decoded)?;
    println!("RECEIVED FRAME");
    let mut src_frame = Video::empty();
    // Convert to RGBA pixel format and resize
    scaler.run(&decoded, &mut src_frame)?;

    // Check for rotation metadata
    if let Some(side_data) = stream.side_data()
    .find(|tag| tag.kind() == side_data::Type::DisplayMatrix) {
      if let Ok(matrix) = math::parse_display_matrix(side_data.data()) {
        // Create rotated empty frame
        let mut dst_frame = Video::new(
          Pixel::RGBA,
          src_frame.height(),
          src_frame.width(),
        );
        math::rotate_frame(
          &mut src_frame,
          &mut dst_frame,
          &matrix,
        );
        return Ok(encode_webp_file(&dst_frame))
      }
    }
    return Ok(encode_webp_file(&src_frame))
  }
}

fn encode_webp_file(frame: &Video) -> WebPMemory {
  let data = fix_img_padding(frame);
  let encoder = Encoder::from_rgba(data.as_slice(), frame.width(), frame.height());
  let now = std::time::Instant::now();
  let webp = encoder.encode(50.);
  println!("Encoded in {:?}", now.elapsed());
  webp
}

fn fix_img_padding(frame: &Video) -> Vec<u8> {
  let data = frame.data(0);
  let mut buffer = Vec::with_capacity(data.len());
  let stride = frame.stride(0);
  let byte_width: usize = 4 * frame.width() as usize;
  let height: usize = frame.height() as usize;
  println!("{stride:?} {byte_width:?} {:?} {height:?}", frame.width());
  for line in 0..height {
    let begin = line * stride;
    let end = begin + byte_width;
    buffer.extend_from_slice(&data[begin..end]);
  }
  buffer
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
