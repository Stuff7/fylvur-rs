use serde::Deserialize;

fn main() {
  println!("cargo:rerun-if-changed=build.rs");
  println!("cargo:rerun-if-changed=fylvur-cfg.toml");

  let out_dir = std::env::var_os("OUT_DIR").unwrap();
  let path = std::path::Path::new(&out_dir).join("config.rs");
  let cfg = load_config().expect("Failed to load config");
  std::fs::write(
    &path,
    format!("\
    const PUBLIC_FOLDER: &str = {public_folder:?};\
    const MEDIA_FOLDER: &str = {media_folder:?};\
    const HOST: &str = {host:?};\
    const PORT: u16 = {port:?};\
    ",
    public_folder = cfg.public_folder,
    media_folder = cfg.media_folder,
    host = cfg.host,
    port = cfg.port,
  ),
  ).unwrap();
}

#[derive(Debug, Deserialize)]
pub struct Config {
  pub public_folder: String,
  pub media_folder: String,
  pub host: String,
  pub port: u16,
}

pub fn load_config() -> std::io::Result<Config> {
  let content = std::fs::read_to_string("./fylvur-cfg.toml")?;
  Ok(toml::from_str(&content)?)
}
