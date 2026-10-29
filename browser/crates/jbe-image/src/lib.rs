//! Safe and bounded image decoding for Jinn Browser Engine.
//!
//! Implements Netpbm PPM (P6 binary and P3 ASCII) format with strict limits
//! to prevent memory exhaustion, excessive dimensions, or arithmetic overflow.

use jbe_core::{EngineError, EngineLimits};

/// A decoded in-memory image with ARGB32 pixels.
#[derive(Debug, Clone, PartialEq)]
pub struct DecodedImage {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<u32>,
}

impl DecodedImage {
    /// Gets a pixel color as ARGB32.
    pub fn get_pixel(&self, x: u32, y: u32) -> Option<u32> {
        if x < self.width && y < self.height {
            Some(self.pixels[(y * self.width + x) as usize])
        } else {
            None
        }
    }
}

/// Decodes an image from raw bytes with EngineLimits enforcement.
pub fn decode_image(data: &[u8], limits: &EngineLimits) -> Result<DecodedImage, EngineError> {
    if data.len() > limits.max_document_size {
        return Err(EngineError::ResourceTooLarge {
            size: data.len(),
            max_size: limits.max_document_size,
        });
    }

    if data.starts_with(b"P6") {
        decode_ppm_p6(data, limits)
    } else if data.starts_with(b"P3") {
        decode_ppm_p3(data, limits)
    } else {
        Err(EngineError::ImageDecodeFailed(
            "Unsupported image format (only PPM P3/P6 supported in v0.1.0)".into(),
        ))
    }
}

/// Decodes P6 binary PPM.
fn decode_ppm_p6(data: &[u8], limits: &EngineLimits) -> Result<DecodedImage, EngineError> {
    let mut pos = 2; // skip 'P6'
    let width = read_ppm_number(data, &mut pos)?;
    let height = read_ppm_number(data, &mut pos)?;
    let max_val = read_ppm_number(data, &mut pos)?;

    validate_dimensions(width, height, limits)?;

    if max_val == 0 || max_val > 255 {
        return Err(EngineError::ImageDecodeFailed(
            "PPM max color value must be 1..255".into(),
        ));
    }

    // Skip single whitespace after max_val
    if pos < data.len() && data[pos].is_ascii_whitespace() {
        pos += 1;
    }

    let total_pixels = (width as usize)
        .checked_mul(height as usize)
        .ok_or_else(|| EngineError::ImageDecodeFailed("Pixel count arithmetic overflow".into()))?;

    let expected_bytes = total_pixels
        .checked_mul(3)
        .ok_or_else(|| EngineError::ImageDecodeFailed("Byte count arithmetic overflow".into()))?;

    let pixel_data = &data[pos..];
    if pixel_data.len() < expected_bytes {
        return Err(EngineError::ImageDecodeFailed(format!(
            "Truncated PPM data: expected {} bytes, got {}",
            expected_bytes,
            pixel_data.len()
        )));
    }

    let mut pixels = Vec::with_capacity(total_pixels);
    let (chunks, _) = pixel_data[..expected_bytes].as_chunks::<3>();
    for chunk in chunks {
        let r = chunk[0];
        let g = chunk[1];
        let b = chunk[2];
        let argb = 0xFF000000 | ((r as u32) << 16) | ((g as u32) << 8) | (b as u32);
        pixels.push(argb);
    }

    Ok(DecodedImage {
        width,
        height,
        pixels,
    })
}

/// Decodes P3 ASCII PPM.
fn decode_ppm_p3(data: &[u8], limits: &EngineLimits) -> Result<DecodedImage, EngineError> {
    let mut pos = 2; // skip 'P3'
    let width = read_ppm_number(data, &mut pos)?;
    let height = read_ppm_number(data, &mut pos)?;
    let max_val = read_ppm_number(data, &mut pos)?;

    validate_dimensions(width, height, limits)?;

    if max_val == 0 || max_val > 255 {
        return Err(EngineError::ImageDecodeFailed(
            "PPM max color value must be 1..255".into(),
        ));
    }

    let total_pixels = (width as usize)
        .checked_mul(height as usize)
        .ok_or_else(|| EngineError::ImageDecodeFailed("Pixel count arithmetic overflow".into()))?;

    let mut pixels = Vec::with_capacity(total_pixels);
    for _ in 0..total_pixels {
        let r = read_ppm_number(data, &mut pos)? as u8;
        let g = read_ppm_number(data, &mut pos)? as u8;
        let b = read_ppm_number(data, &mut pos)? as u8;
        let argb = 0xFF000000 | ((r as u32) << 16) | ((g as u32) << 8) | (b as u32);
        pixels.push(argb);
    }

    Ok(DecodedImage {
        width,
        height,
        pixels,
    })
}

fn validate_dimensions(width: u32, height: u32, limits: &EngineLimits) -> Result<(), EngineError> {
    if width == 0 || height == 0 {
        return Err(EngineError::ImageDecodeFailed(
            "Image dimensions cannot be zero".into(),
        ));
    }
    if width > limits.max_image_dimension || height > limits.max_image_dimension {
        return Err(EngineError::ImageDecodeFailed(format!(
            "Image dimension {}x{} exceeds limit {}",
            width, height, limits.max_image_dimension
        )));
    }
    let pixels = (width as u64).checked_mul(height as u64).ok_or_else(|| {
        EngineError::ImageDecodeFailed("Overflow calculating image pixels".into())
    })?;

    if pixels > limits.max_image_pixels as u64 {
        return Err(EngineError::ImageDecodeFailed(format!(
            "Image pixel count {} exceeds limit {}",
            pixels, limits.max_image_pixels
        )));
    }

    let bytes = pixels.checked_mul(4).ok_or_else(|| {
        EngineError::ImageDecodeFailed("Overflow calculating image memory".into())
    })?;

    if bytes > limits.max_decompressed_bytes as u64 {
        return Err(EngineError::ImageDecodeFailed(format!(
            "Decompressed image memory {} bytes exceeds limit {}",
            bytes, limits.max_decompressed_bytes
        )));
    }

    Ok(())
}

/// Reads next non-comment ASCII integer from PPM data.
fn read_ppm_number(data: &[u8], pos: &mut usize) -> Result<u32, EngineError> {
    skip_whitespace_and_comments(data, pos);

    let start = *pos;
    while *pos < data.len() && data[*pos].is_ascii_digit() {
        *pos += 1;
    }

    if start == *pos {
        return Err(EngineError::ImageDecodeFailed(
            "Unexpected end of data or invalid token while parsing PPM number".into(),
        ));
    }

    let num_str = std::str::from_utf8(&data[start..*pos])
        .map_err(|_| EngineError::ImageDecodeFailed("Non-utf8 token in PPM header".into()))?;

    num_str
        .parse::<u32>()
        .map_err(|_| EngineError::ImageDecodeFailed("Failed to parse integer in PPM header".into()))
}

fn skip_whitespace_and_comments(data: &[u8], pos: &mut usize) {
    while *pos < data.len() {
        let b = data[*pos];
        if b.is_ascii_whitespace() {
            *pos += 1;
        } else if b == b'#' {
            // Comment extends to end of line
            while *pos < data.len() && data[*pos] != b'\n' {
                *pos += 1;
            }
        } else {
            break;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_decode_ppm_p6() {
        let limits = EngineLimits::default();
        // 2x2 binary PPM
        let mut data = Vec::new();
        data.extend_from_slice(b"P6\n2 2\n255\n");
        data.extend_from_slice(&[
            255, 0, 0, // red
            0, 255, 0, // green
            0, 0, 255, // blue
            255, 255, 255, // white
        ]);

        let img = decode_image(&data, &limits).unwrap();
        assert_eq!(img.width, 2);
        assert_eq!(img.height, 2);
        assert_eq!(img.pixels.len(), 4);
        assert_eq!(img.get_pixel(0, 0), Some(0xFFFF0000));
        assert_eq!(img.get_pixel(1, 0), Some(0xFF00FF00));
        assert_eq!(img.get_pixel(0, 1), Some(0xFF0000FF));
        assert_eq!(img.get_pixel(1, 1), Some(0xFFFFFFFF));
    }

    #[test]
    fn test_dimension_limits_enforced() {
        let limits = EngineLimits {
            max_image_dimension: 100,
            ..Default::default()
        };
        let data = b"P6\n200 200\n255\n";
        let res = decode_image(data, &limits);
        assert!(matches!(res, Err(EngineError::ImageDecodeFailed(_))));
    }

    #[test]
    fn test_truncated_data_rejected() {
        let limits = EngineLimits::default();
        let data = b"P6\n10 10\n255\n12345";
        let res = decode_image(data, &limits);
        assert!(matches!(res, Err(EngineError::ImageDecodeFailed(_))));
    }
}
