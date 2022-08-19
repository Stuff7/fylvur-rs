use std::path;

use serde::Serialize;
use actix_files as actix_fs;

use crate::{f, video, MEDIA_FOLDER};

pub fn get_media_path(path: &String) -> path::PathBuf {
  path::Path::new(&MEDIA_FOLDER).join(path)
}

pub fn get_folder_contents(path: &String) -> std::io::Result<Vec<FileInfo>> {
  let folder = get_media_path(&path);

  let dir = std::fs::read_dir(&folder)?;

  let mut paths: Vec<FileInfo> = dir.map(|p| {
    if let Ok(dir_entry) = p {
      FileInfo::from_path(&dir_entry.path()).unwrap()
    } else {FileInfo::default()}
  }).collect();

  paths.sort_unstable_by_key(|f| !f.is_folder);

  Ok(paths)
}

#[derive(Debug, Default, Serialize)]
pub struct FileMetadata {
  duration_ms: i64,
}

impl FileMetadata {
  pub fn from_path(path: &path::PathBuf) -> Self {
    let duration_ms = video::get_duration(
      &path.to_str().unwrap_or_default().to_string()
    ).unwrap_or_default();
    Self { duration_ms }
  }
}

#[derive(Debug, Serialize)]
pub struct FileInfo {
  api_href: String,
  file_type: String,
  href: String,
  is_folder: bool,
  name: String,
  mime: String,
}

impl FileInfo {
  pub fn from_path(file_path: &path::PathBuf) -> std::io::Result<Self> {
    let name = file_path
    .file_name().unwrap_or_default()
    .to_str().unwrap_or_default();
    let is_folder = file_path.is_dir();
    let url_path = match file_path.strip_prefix(MEDIA_FOLDER) {
      Ok(path) => path.to_str().unwrap_or_default(),
      Err(_) => "",
    }.replace("\\", "/");

    if is_folder {
      return Ok(Self {
        api_href: f!("/api/folder/{url_path}"),
        file_type: "folder".into(),
        href: f!("/{url_path}"),
        is_folder,
        name: name.to_string(),
        mime: "application/json".into(),
      })
    }

    let named_file = actix_fs::NamedFile::open(&file_path)?;
    let content_type = named_file.content_type();
    let mut file_type = content_type.type_().to_string();
    if file_type == "application" {
      file_type = content_type.subtype().to_string();
    }

    // let duration_ms = if file_type == "video" {
    //   video::get_duration(&raw_file_path).unwrap_or_default()
    // } else {0};

    let endpoint = if file_type == "video" {
      "api/thumbnail"
    } else {"file"}.to_string();

    Ok(Self {
      api_href: f!("/{endpoint}/{url_path}"),
      file_type,
      href: f!("/{url_path}"),
      is_folder,
      name: name.to_string(),
      mime: content_type.to_string(),
    })
  }
}

impl Default for FileInfo {
  fn default() -> Self {
    Self {
      api_href: "".into(),
      file_type: "unknown".into(),
      href: "".into(),
      is_folder: false,
      name: "unknown".into(),
      mime: "text/plain".into(),
    }
  }
}
