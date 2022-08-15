use ffmpeg::frame::Video as VideoFrame;

use crate::f;

const PX_BYTES: usize = 4;

/// Extract the rotation component of the transformation matrix and
/// returns the angle (in degrees) by which the transformation rotates
/// the frame counterclockwise. The angle will be in range `[-180.0, 180.0]`,
/// or `None` if the matrix is singular
/// 
/// # Arguments
/// * `matrix` - The transformation matrix
/// 
/// *Note: This is a translated implementation from [libavutil](https://ffmpeg.org/doxygen/trunk/display_8c_source.html#l00035)*
pub fn av_display_rotation_get(matrix: &[i32; 9]) -> Option<f32> {
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

/// Rotates `src_frame` using `transform` matrix and stores it in `dst_frame`
/// 
/// The transformation maps a point `(p, q)` in the source (pre-transformation) frame
/// to the point `(p', q')` in the destination (post-transformation) frame as follows:
/// ```
/// //             | a b u |
/// // (p, q, 1) . | c d v | = z * (p', q', 1)
/// //             | x y w |
/// ```
/// The transformation can also be more explicitly written in components as follows:
/// ```
/// let dp = (a * p + c * q + x) / z;
/// let dq = (b * p + d * q + y) / z;
/// let z  =  u * p + v * q + w;
/// ```
/// 
/// *For more info on how this works check [libav docs](https://libav.org/documentation/doxygen/master/group__lavu__video__display.html)*
pub fn rotate_frame(src_frame: &VideoFrame, dst_frame: &mut VideoFrame, transform: &[i32; 9]) {
  let src_width = src_frame.width() as usize;
  let src_data = src_frame.data(0);
  let dst_width = dst_frame.width() as usize;
  let dst_data = dst_frame.data_mut(0);
  let [
    a, b, u,
    c, d, v,
    x, y, w,
  ] = transform;

  let px_area = src_data.len() / PX_BYTES;
  for i in 0..px_area {
    let (p, q) = (
      (i % src_width) as i32,
      (i / src_width) as i32,
    );

    let z = u * p + v * q + w;
    let dp = (a * p + c * q + x) / z;
    let dq = (b * p + d * q + y) / z;
    let di = (dp + dst_width as i32 * dq) as usize;

    let no_overflow = di == 0 || usize::MAX / di >= PX_BYTES;
    if no_overflow && di * PX_BYTES < dst_data.len() {
      for color_idx in 0..PX_BYTES {
        dst_data[di * PX_BYTES + color_idx] = src_data[i * PX_BYTES + color_idx];
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
        // | 0 1 2 |
        // | 3 4 5 |
        // | 6 7 8 |
        // All numbers are stored in native endianness, as 16.16 fixed-point values,
        // except for 2, 5 and 8, which are stored as 2.30 fixed-point values.
        let value = i32::from_ne_bytes(chunk);
        matrix[i] = if i == 2 || i == 5 || i == 8 {
          to_fixed_point(value, 30)
        } else {
          to_fixed_point(value, 16)
        }
      }
      Err(e) => {
        return Err(f!("FAILED TO CONVERT {:?}\n\nErr:{e:?}", (i * 4)..(i * 4 + 4)))
      }
    }
  }
  Ok(matrix)
}

fn to_fixed_point(x: i32, n: i32) -> i32 {
  ((x as f32) / (1 << n) as f32) as i32
}
