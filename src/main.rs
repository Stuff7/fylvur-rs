extern crate ffmpeg_next as ffmpeg;

use format as f;

mod file;
mod math;
mod video;

use serde::Deserialize;
use actix_files as actix_fs;
use actix_web::{get, web, App, HttpResponse, HttpServer, Responder};

include!(concat!(env!("OUT_DIR"), "/config.rs"));

#[derive(Debug, Deserialize)]
pub struct ThumbnailRequest {
  width: Option<u32>,
  seek: Option<f32>,
}

#[get("/{any:.*}")]
async fn index() -> impl Responder {
  actix_fs::NamedFile::open_async("./public/index.html").await
}

#[get("/api/file/{video_path:.*}")]
async fn get_folder_info(
  path: web::Path<String>,
) -> impl Responder {
  let path = &path.into_inner();
  if let Ok(paths) = file::get_folder_contents(path) {
    return HttpResponse::Ok().json(paths)
  }
  if let Ok(file) = file::FileInfo::from_path(&file::get_media_path(path)) {
    return HttpResponse::Ok().json(file)
  }
  HttpResponse::NotFound().json(file::FileInfo::default())
}

#[get("/api/file-metadata/{path:.*}")]
async fn get_file_metadata(
  path: web::Path<String>,
) -> impl Responder {
  let path = &path.into_inner();
  HttpResponse::Ok().json(
    file::FileMetadata::from_path(&file::get_media_path(path))
  )
}

#[get("/api/thumbnail/{video_path:.*}")]
async fn get_video_thumbnail(
  path: web::Path<String>,
  query: web::Query<ThumbnailRequest>,
) -> impl Responder {
  use video::SeekTime::*;

  let video_path = file::get_media_path(&path.into_inner());
  let video_path = video_path.to_str().unwrap_or_default();

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
      .service(get_file_metadata)
      .service(actix_fs::Files::new("/file", MEDIA_FOLDER))
      .service(actix_fs::Files::new("/static", "./public"))
      .service(index)
  })
  .bind((HOST, PORT))?
  .run();

  println!("Listening in http://{}:{}", HOST, PORT);
  server.await
}
