## Fylvur

File explorer to access and preview media files over a local network

## Building on Windows

- Install FFmpeg (complete with headers) through any means, e.g. downloading a pre-built "full_build-shared" version from https://ffmpeg.org/download.html. Set FFMPEG_DIR to the directory containing include and lib
- Add ffmpeg bin directory to PATH
- Create `fylvur-cfg.toml` and fill in the fields found in`fylvur-cfg.example.toml`
- `cargo build`
