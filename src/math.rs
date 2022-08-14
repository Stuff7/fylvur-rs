use ffmpeg::frame::Video as VideoFrame;

use crate::f;

/// Extract the rotation component of the transformation matrix and
/// returns the angle (in degrees) by which the transformation rotates
/// the frame counterclockwise. The angle will be in range `[-180.0, 180.0]`,
/// or `None` if the matrix is singular
/// 
/// # Arguments
/// * `matrix` - The transformation matrix
/// 
/// *Note: This is a translated implementation from [libavutil](https://ffmpeg.org/doxygen/trunk/display_8c_source.html#l00035)*
pub fn av_display_rotation_get(matrix: [i32; 9]) -> Option<f32> {
  let mut scale = [0_f32; 2];

  scale[0] = f32::hypot(matrix[0] as f32, matrix[3] as f32);
  scale[1] = f32::hypot(matrix[1] as f32, matrix[4] as f32);

  if scale[0] == 0.0 || scale[1] == 0.0 {
    return None;
  }

  let rotation = f32::atan2(
    (matrix[1] as f32) / scale[1],
    (matrix[0] as f32) / scale[0],
  ) * 180_f32 / std::f32::consts::PI;

  Some(-rotation)
}

/// The transformation maps a point (p, q) in the source (pre-transformation) frame
/// to the point (p', q') in the destination (post-transformation) frame as follows:
///             | a b u |
/// (p, q, 1) . | c d v | = z * (p', q', 1)
///             | x y w |
///
/// The transformation can also be more explicitly written in components as follows:
/// p' = (a * p + c * q + x) / z;
/// q' = (b * p + d * q + y) / z;
/// z  =  u * p + v * q + w
/// 
/// | a:0           b:65536    u:0          |
/// | c:-65536      d:0        v:0          |
/// | x:70778880    y:0        w:1073741824 |
pub fn rotate_frame(src_frame: &mut VideoFrame, dst_frame: &mut VideoFrame, transform: &[i32; 9]) {
  let src_width = src_frame.width() as usize;
  let src_data = src_frame.data(0);
  let dst_width = dst_frame.width() as usize;
  let dst_data = dst_frame.data_mut(0);
  let [
    a, b, u,
    c, d, v,
    x, y, w,
  ] = transform;

  // u, v and w must be 2 bytes max
  let u = std::cmp::min(u, &(u16::MAX as i32));
  let v = std::cmp::min(v, &(u16::MAX as i32));
  let w = std::cmp::min(w, &(u16::MAX as i32));

  let img_area = src_data.len() / 4;
  for i in 0..img_area {
    let (p, q) = (
      (i % src_width) as i32,
      (i / src_width) as i32,
    );

    let z = u * p + v * q + w;
    let dp = (a * p + c * q + x) / z;
    let dq = (b * p + d * q + y) / z;
    let di = (dp + dst_width as i32 * dq) as usize;

    let no_overflow = di == 0 || usize::MAX / di >= 4;
    if no_overflow && di * 4 < dst_data.len() {
      for color_idx in 0..4 {
        dst_data[di * 4 + color_idx] = src_data[i * 4 + color_idx];
      }
    }
  }
}

/// Converts display matrix bytes into 3x3 integer matrix `[u8; 36]` => `[i32; 9]`
/// # Arguments
/// * `bytes` - Display matrix side data
pub fn parse_display_matrix(bytes: &[u8]) -> Result<[i32; 9], String> {
  let mut matrix = [0; 9];
  // loop 3x3 matrix
  for i in 0..9 {
    let chunk_range = (i * 4)..(i * 4 + 4);
    // Split bytes slice &[u8] into array chunk [u8; 4]
    let conversion:
      Result<[u8; 4], std::array::TryFromSliceError> =
      bytes[chunk_range].try_into();

    match conversion {
      Ok(chunk) => {
        matrix[i] = i32::from_ne_bytes(chunk);
      }
      Err(e) => {
        return Err(f!("FAILED TO CONVERT {:?}\n\nErr:{e:?}", (i * 4)..(i * 4 + 4)))
      }
    }
  }
  Ok(matrix)
}
