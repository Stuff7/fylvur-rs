extern crate ffmpeg_next as ffmpeg;

use format as f;

mod video;
mod math;

use serde::Deserialize;
use actix_files as fs;
use actix_web::{get, web, App, HttpResponse, HttpServer, Responder};

const HOST: &str = "127.0.0.1";
const PORT: u16 = 80;

const STATIC_FOLDER: &str = r#"D:\images\Screenshots"#;

#[derive(Debug, Deserialize)]
pub struct ThumbnailRequest {
  width: Option<u32>,
  seek: Option<f32>,
}

#[derive(Debug, Deserialize)]
pub struct ThumbnailPath {
  video_path: String,
}

#[get("/")]
async fn index() -> impl Responder {
  fs::NamedFile::open_async("./public/index.html").await
}

#[get("/thumbnail/{video_path:.*}")]
async fn get_video_thumbnail(
  path: web::Path<ThumbnailPath>,
  query: web::Query<ThumbnailRequest>,
) -> impl Responder {
  let static_path = std::path::Path::new(STATIC_FOLDER);
  if let Some(video_path) = static_path.join(&path.video_path).to_str() {
    use video::SeekTime::*;
    let seek = query.seek.unwrap_or(0.);
    return match video::get_frame(
      video_path.to_string(),
      query.width.unwrap_or_default(),
      if seek < 1. {Percentage(seek)} else {Seconds(seek as u32)},
    ) {
      Ok(thumbnail) => HttpResponse::Ok()
        .content_type("image/webp")
        .body(web::Bytes::copy_from_slice(&*thumbnail)),
      Err(err) => HttpResponse::BadRequest()
        .body(f!("Could not get thumbnail - {err:?}"))
    }
  }
  HttpResponse::BadRequest()
  .body("Invalid path")
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
  video::init()
  .expect("Could not initialize video API");

  std::fs::create_dir_all("./frames")
  .expect("Could not create frames folder");

  let server = HttpServer::new(|| {
    App::new()
      .service(index)
      .service(get_video_thumbnail)
      .service(fs::Files::new("/file", STATIC_FOLDER))
      .service(fs::Files::new("/static", "./public"))
  })
  .bind((HOST, PORT))?
  .run();

  println!("Listening in http://{HOST}:{PORT}");
  server.await
}
