extern crate ffmpeg_next as ffmpeg;
use format as f;

mod video;
mod math;

use std::env;
use std::path::PathBuf;
use std::fmt::Debug;

fn main() {
  video::init()
  .expect("Could not initialize video API");

  std::fs::create_dir_all("./frames")
  .expect("Could not create frames folder");

  let args = CLIArgs::parse().expect("Failed to parse arguments");

  let thumbnail = video::get_frame(
    args.video_path,
    args.th_width,
    args.seek_pos,
  ).expect("Could not get thumbnail");

  let output_path = PathBuf::from(format!("./frames/thumbnail.webp"));
  std::fs::write(&output_path, &*thumbnail)
  .expect("Could not save thumbnail");
}

#[derive(Debug)]
struct CLIArgs {
  video_path: String,
  th_width: u32,
  seek_pos: u32,
}

impl CLIArgs {
  fn parse<'a>() -> Result<Self, &'a str> {
    let mut args = env::args();
    let video_path = if let Some(arg) = args.nth(1) {
      arg
    } else {
      return Err("You need to specifiy a path to the video file")
    };

    let mut options = [0; 2];
    for (i, arg) in args.take(2).enumerate() {
      options[i] = arg.parse::<u32>().unwrap_or_default();
    }
    let [th_width, seek_pos] = options;

    Ok(Self { video_path, th_width, seek_pos })
  }
}
