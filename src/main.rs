extern crate ffmpeg_next as ffmpeg;

use format as f;

mod file;
mod math;
mod video;

use serde::Deserialize;
use actix_files as actix_fs;
use actix_web::{get, web, App, HttpResponse, HttpServer, Responder};

const HOST: &str = "127.0.0.1";
const PORT: u16 = 80;

#[derive(Debug, Deserialize)]
pub struct ThumbnailRequest {
  width: Option<u32>,
  seek: Option<f32>,
}

#[get("/{any:.*}")]
async fn index() -> impl Responder {
  actix_fs::NamedFile::open_async("./public/index.html").await
}

#[get("/api/folder/{video_path:.*}")]
async fn get_folder_info(
  path: web::Path<String>,
) -> impl Responder {
  if let Ok(paths) = file::get_folder_contents(&path.into_inner()) {
    return HttpResponse::Ok().json(paths)
  }

  HttpResponse::BadRequest().body("Invalid path")
}

#[get("/api/thumbnail/{video_path:.*}")]
async fn get_video_thumbnail(
  path: web::Path<String>,
  query: web::Query<ThumbnailRequest>,
) -> impl Responder {
  use video::SeekTime::*;

  let video_path = file::get_static_path(&path.into_inner());

  let seek = query.seek.unwrap_or(0.);

  match video::get_frame(
    &video_path.to_string(),
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

#[actix_web::main]
async fn main() -> std::io::Result<()> {
  video::init()
  .expect("Could not initialize video API");

  std::fs::create_dir_all("./frames")
  .expect("Could not create frames folder");

  let server = HttpServer::new(|| {
    App::new()
      .service(get_video_thumbnail)
      .service(get_folder_info)
      .service(actix_fs::Files::new("/file", file::STATIC_FOLDER))
      .service(actix_fs::Files::new("/static", "./public"))
      .service(index)
  })
  .bind((HOST, PORT))?
  .run();

  println!("Listening in http://{HOST}:{PORT}");
  server.await
}
