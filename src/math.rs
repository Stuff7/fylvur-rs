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
