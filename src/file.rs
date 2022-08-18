use serde::Serialize;
use actix_files as actix_fs;

use crate::{f, video, MEDIA_FOLDER};

pub fn get_static_path(path: &String) -> String {
  let static_path = std::path::Path::new(&MEDIA_FOLDER);
  match static_path.join(path).to_str() {
    Some(static_path) => static_path.to_string(),
    None => String::new(),
  }
}

pub fn get_folder_contents(path: &String) -> std::io::Result<Vec<FileInfo>> {
  let timer = std::time::Instant::now();
  let folder = get_static_path(&path);
  let paths = std::fs::read_dir(&folder)?;

  let paths: Vec<FileInfo> = paths.map(|p| {
    if let Ok(p) = p {
      let name = p.file_name();
      let name = name.to_str().unwrap_or_default();
      let is_folder = match p.file_type() {
        Ok(file_type) => file_type.is_dir(),
        Err(_) => false,
      };

      let file_path = p.path().display().to_string();
      let path = if path.is_empty() {name.to_string()} else {f!("{path}/{name}")};

      if is_folder {
        return FileInfo {
          api_href: f!("/api/folder/{path}"),
          details: Metadata::default(),
          file_type: "folder".into(),
          href: path,
          is_folder,
          name: name.to_string(),
          mime: "application/json".into(),
        }
      }

      return match actix_fs::NamedFile::open(&file_path) {
        Ok(named_file) => {
          let content_type = named_file.content_type();
          let mut file_type = content_type.type_().to_string();
          if file_type == "application" {
            file_type = content_type.subtype().to_string();
          }

          let duration_ms = if file_type == "video" {
            video::get_duration(&file_path).unwrap_or_default()
          } else {0};

          let endpoint = if file_type == "video" {
            "api/thumbnail"
          } else {"file"}.to_string();

          FileInfo {
            api_href: f!("/{endpoint}/{path}"),
            details: Metadata { duration_ms },
            file_type,
            href: path,
            is_folder,
            name: name.to_string(),
            mime: content_type.to_string(),
          }
        }
        Err(e) => {
          println!("Failed to open NamedFile: {e:?}");
          FileInfo { name: name.to_string(), ..FileInfo::default() }
        }
      };
    }
    FileInfo::default()
  }).collect();

  println!("Elapsed: {:?}", timer.elapsed());
  Ok(paths)
}

#[derive(Debug, Default, Serialize)]
pub struct Metadata {
  duration_ms: i64,
}

#[derive(Debug, Serialize)]
pub struct FileInfo {
  api_href: String,
  details: Metadata,
  file_type: String,
  href: String,
  is_folder: bool,
  name: String,
  mime: String,
}

impl Default for FileInfo {
  fn default() -> Self {
    Self {
      api_href: "".into(),
      details: Metadata::default(),
      file_type: "unknown".into(),
      href: "".into(),
      is_folder: false,
      name: "unknown".into(),
      mime: "text/plain".into(),
    }
  }
}
