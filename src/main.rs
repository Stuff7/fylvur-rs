extern crate ffmpeg_next as ffmpeg;

use format as f;

mod file;
mod math;
mod video;

use serde::Deserialize;
use actix_files as actix_fs;
use actix_web::{get, web, App, HttpResponse, HttpServer, Responder};

use std::path::Path;

include!(concat!(env!("OUT_DIR"), "/config.rs"));

#[derive(Debug, Deserialize)]
pub struct ThumbnailRequest {
  width: Option<u32>,
  seek: Option<f32>,
}

#[derive(Debug, Deserialize)]
pub struct AtlasRequest {
  seek: Option<u32>,
}

#[get("/{any:.*}")]
async fn index() -> impl Responder {
  actix_fs::NamedFile::open_async(Path::new(PUBLIC_FOLDER).join("index.html")).await
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

  match video::get_video_thumbnail(
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

#[get("/api/atlas/{video_path:.*}")]
async fn get_video_atlas(
  path: web::Path<String>,
  query: web::Query<AtlasRequest>,
) -> impl Responder {
  let video_path = file::get_media_path(&path.into_inner());
  let video_path = video_path.to_str().unwrap_or_default();

  let seek = query.seek.unwrap_or(0);

  match video::get_video_atlas(
    &video_path.to_string(),
    seek,
  ) {
    Ok(atlas) => HttpResponse::Ok()
      .content_type("image/webp")
      .body(web::Bytes::copy_from_slice(&*atlas)),
    Err(err) => HttpResponse::BadRequest()
      .content_type("text/plain")
      .body(f!("Could not get video atlas - {err:?}"))
  }
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
  video::init()
  .expect("Could not initialize video API");

  let server = HttpServer::new(|| {
    App::new()
      .service(get_video_thumbnail)
      .service(get_folder_info)
      .service(get_file_metadata)
      .service(get_video_atlas)
      .service(actix_fs::Files::new("/file", MEDIA_FOLDER))
      .service(actix_fs::Files::new("/static", PUBLIC_FOLDER))
      .service(index)
  })
  .bind((HOST, PORT))?
  .run();

  println!("Listening in http://{}:{}", HOST, PORT);
  server.await
}
