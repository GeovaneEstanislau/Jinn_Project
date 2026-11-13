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
    } else if data.starts_with(b"qoif") {
        decode_qoi(data, limits)
    } else if data.starts_with(b"BM") {
        decode_bmp(data, limits)
    } else {
        Err(EngineError::ImageDecodeFailed(
            "Unsupported image format (supported: PPM P3/P6, QOI, BMP)".into(),
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

/// Decodes a QOI (Quite OK Image format) binary.
fn decode_qoi(data: &[u8], limits: &EngineLimits) -> Result<DecodedImage, EngineError> {
    if data.len() < 14 {
        return Err(EngineError::ImageDecodeFailed("QOI data too small for header".into()));
    }
    if !data.starts_with(b"qoif") {
        return Err(EngineError::ImageDecodeFailed("Invalid QOI magic header".into()));
    }

    let width = u32::from_be_bytes([data[4], data[5], data[6], data[7]]);
    let height = u32::from_be_bytes([data[8], data[9], data[10], data[11]]);
    let channels = data[12];
    let _colorspace = data[13];

    if channels != 3 && channels != 4 {
        return Err(EngineError::ImageDecodeFailed("QOI channels must be 3 (RGB) or 4 (RGBA)".into()));
    }

    validate_dimensions(width, height, limits)?;

    let total_pixels = (width as usize)
        .checked_mul(height as usize)
        .ok_or_else(|| EngineError::ImageDecodeFailed("Pixel count arithmetic overflow in QOI".into()))?;

    let mut pixels = Vec::with_capacity(total_pixels);
    let mut index: [u32; 64] = [0; 64];

    let mut r: u8 = 0;
    let mut g: u8 = 0;
    let mut b: u8 = 0;
    let mut a: u8 = 255;

    let mut pos = 14usize;
    let end_limit = data.len().saturating_sub(8);

    let pack_pixel = |r: u8, g: u8, b: u8, a: u8| -> u32 {
        ((a as u32) << 24) | ((r as u32) << 16) | ((g as u32) << 8) | (b as u32)
    };

    let qoi_hash = |r: u8, g: u8, b: u8, a: u8| -> usize {
        (r as usize * 3 + g as usize * 5 + b as usize * 7 + a as usize * 11) % 64
    };

    while pos < end_limit && pixels.len() < total_pixels {
        let b1 = data[pos];
        pos += 1;

        if b1 == 0xFE {
            // QOI_OP_RGB
            if pos + 3 > data.len() {
                return Err(EngineError::ImageDecodeFailed("Truncated QOI_OP_RGB".into()));
            }
            r = data[pos];
            g = data[pos + 1];
            b = data[pos + 2];
            pos += 3;
            let px = pack_pixel(r, g, b, a);
            index[qoi_hash(r, g, b, a)] = px;
            pixels.push(px);
        } else if b1 == 0xFF {
            // QOI_OP_RGBA
            if pos + 4 > data.len() {
                return Err(EngineError::ImageDecodeFailed("Truncated QOI_OP_RGBA".into()));
            }
            r = data[pos];
            g = data[pos + 1];
            b = data[pos + 2];
            a = data[pos + 3];
            pos += 4;
            let px = pack_pixel(r, g, b, a);
            index[qoi_hash(r, g, b, a)] = px;
            pixels.push(px);
        } else {
            let tag = b1 & 0xC0;
            match tag {
                0x00 => {
                    // QOI_OP_INDEX
                    let idx = (b1 & 0x3F) as usize;
                    let px = index[idx];
                    a = ((px >> 24) & 0xFF) as u8;
                    r = ((px >> 16) & 0xFF) as u8;
                    g = ((px >> 8) & 0xFF) as u8;
                    b = (px & 0xFF) as u8;
                    pixels.push(px);
                }
                0x40 => {
                    // QOI_OP_DIFF
                    let dr = ((b1 >> 4) & 0x03) as i8 - 2;
                    let dg = ((b1 >> 2) & 0x03) as i8 - 2;
                    let db = (b1 & 0x03) as i8 - 2;
                    r = r.wrapping_add(dr as u8);
                    g = g.wrapping_add(dg as u8);
                    b = b.wrapping_add(db as u8);
                    let px = pack_pixel(r, g, b, a);
                    index[qoi_hash(r, g, b, a)] = px;
                    pixels.push(px);
                }
                0x80 => {
                    // QOI_OP_LUMA
                    if pos >= data.len() {
                        return Err(EngineError::ImageDecodeFailed("Truncated QOI_OP_LUMA".into()));
                    }
                    let b2 = data[pos];
                    pos += 1;
                    let dg = (b1 & 0x3F) as i8 - 32;
                    let dr_dg = ((b2 >> 4) & 0x0F) as i8 - 8;
                    let db_dg = (b2 & 0x0F) as i8 - 8;
                    let dr = dr_dg + dg;
                    let db = db_dg + dg;
                    r = r.wrapping_add(dr as u8);
                    g = g.wrapping_add(dg as u8);
                    b = b.wrapping_add(db as u8);
                    let px = pack_pixel(r, g, b, a);
                    index[qoi_hash(r, g, b, a)] = px;
                    pixels.push(px);
                }
                0xC0 => {
                    // QOI_OP_RUN
                    let run = ((b1 & 0x3F) as usize) + 1;
                    let px = pack_pixel(r, g, b, a);
                    for _ in 0..run {
                        if pixels.len() < total_pixels {
                            pixels.push(px);
                        }
                    }
                }
                _ => unreachable!(),
            }
        }
    }

    if pixels.len() != total_pixels {
        return Err(EngineError::ImageDecodeFailed(format!(
            "QOI incomplete: expected {} pixels, got {}",
            total_pixels,
            pixels.len()
        )));
    }

    Ok(DecodedImage {
        width,
        height,
        pixels,
    })
}

/// Decodes an uncompressed Windows Bitmap (BMP) 24-bit or 32-bit.
fn decode_bmp(data: &[u8], limits: &EngineLimits) -> Result<DecodedImage, EngineError> {
    if data.len() < 54 {
        return Err(EngineError::ImageDecodeFailed("BMP data too short for headers".into()));
    }
    if !data.starts_with(b"BM") {
        return Err(EngineError::ImageDecodeFailed("Invalid BMP magic marker".into()));
    }

    let pixel_offset = u32::from_le_bytes([data[10], data[11], data[12], data[13]]) as usize;
    let dib_header_size = u32::from_le_bytes([data[14], data[15], data[16], data[17]]);

    if dib_header_size < 40 {
        return Err(EngineError::ImageDecodeFailed("Unsupported legacy BMP DIB header (<40 bytes)".into()));
    }

    let raw_w = i32::from_le_bytes([data[18], data[19], data[20], data[21]]);
    let raw_h = i32::from_le_bytes([data[22], data[23], data[24], data[25]]);
    let planes = u16::from_le_bytes([data[26], data[27]]);
    let bpp = u16::from_le_bytes([data[28], data[29]]);
    let compression = u32::from_le_bytes([data[30], data[31], data[32], data[33]]);

    if planes != 1 {
        return Err(EngineError::ImageDecodeFailed("BMP planes must be 1".into()));
    }
    if compression != 0 {
        return Err(EngineError::ImageDecodeFailed("Compressed BMP (RLE) is not supported in v0.1.0".into()));
    }
    if bpp != 24 && bpp != 32 {
        return Err(EngineError::ImageDecodeFailed(format!(
            "BMP depth must be 24 or 32 bpp (got {})",
            bpp
        )));
    }
    if raw_w <= 0 || raw_h == 0 {
        return Err(EngineError::ImageDecodeFailed("Invalid BMP dimensions".into()));
    }

    let width = raw_w as u32;
    let is_top_down = raw_h < 0;
    let height = raw_h.unsigned_abs();

    validate_dimensions(width, height, limits)?;

    let total_pixels = (width as usize)
        .checked_mul(height as usize)
        .ok_or_else(|| EngineError::ImageDecodeFailed("Pixel count arithmetic overflow in BMP".into()))?;

    let row_bytes = match bpp {
        24 => ((width as usize * 3) + 3) & !3,
        32 => width as usize * 4,
        _ => unreachable!(),
    };

    let required_size = pixel_offset
        .checked_add(row_bytes.checked_mul(height as usize).unwrap_or(usize::MAX))
        .ok_or_else(|| EngineError::ImageDecodeFailed("BMP file size arithmetic overflow".into()))?;

    if data.len() < required_size {
        return Err(EngineError::ImageDecodeFailed(format!(
            "Truncated BMP data: file is {} bytes, requires at least {}",
            data.len(),
            required_size
        )));
    }

    let mut pixels = vec![0u32; total_pixels];

    for y_idx in 0..height as usize {
        let dest_y = if is_top_down {
            y_idx
        } else {
            (height as usize - 1) - y_idx
        };

        let row_start = pixel_offset + (y_idx * row_bytes);
        let row_slice = &data[row_start..row_start + row_bytes];

        for x_idx in 0..width as usize {
            let px = if bpp == 24 {
                let p = x_idx * 3;
                let b = row_slice[p] as u32;
                let g = row_slice[p + 1] as u32;
                let r = row_slice[p + 2] as u32;
                0xFF00_0000 | (r << 16) | (g << 8) | b
            } else {
                let p = x_idx * 4;
                let b = row_slice[p] as u32;
                let g = row_slice[p + 1] as u32;
                let r = row_slice[p + 2] as u32;
                let mut a = row_slice[p + 3] as u32;
                if a == 0 {
                    a = 0xFF;
                }
                (a << 24) | (r << 16) | (g << 8) | b
            };

            pixels[dest_y * (width as usize) + x_idx] = px;
        }
    }

    Ok(DecodedImage {
        width,
        height,
        pixels,
    })
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

    #[test]
    fn test_decode_bmp_24bit() {
        let limits = EngineLimits::default();
        // 2x2 uncompressed 24-bit BMP
        let mut data = Vec::new();
        // File header (14 bytes)
        data.extend_from_slice(b"BM");
        let file_size: u32 = 14 + 40 + (2 * 4 * 2); // 2 rows * 8 bytes (padded)
        data.extend_from_slice(&file_size.to_le_bytes());
        data.extend_from_slice(&[0, 0, 0, 0]); // reserved
        let offset: u32 = 54;
        data.extend_from_slice(&offset.to_le_bytes());

        // DIB Header BITMAPINFOHEADER (40 bytes)
        data.extend_from_slice(&40u32.to_le_bytes());
        data.extend_from_slice(&2i32.to_le_bytes()); // width = 2
        data.extend_from_slice(&2i32.to_le_bytes()); // height = 2 (bottom-up)
        data.extend_from_slice(&1u16.to_le_bytes()); // planes = 1
        data.extend_from_slice(&24u16.to_le_bytes()); // bpp = 24
        data.extend_from_slice(&0u32.to_le_bytes()); // compression = 0
        data.extend_from_slice(&0u32.to_le_bytes()); // image size
        data.extend_from_slice(&2835u32.to_le_bytes()); // x ppm
        data.extend_from_slice(&2835u32.to_le_bytes()); // y ppm
        data.extend_from_slice(&0u32.to_le_bytes()); // colors used
        data.extend_from_slice(&0u32.to_le_bytes()); // important colors

        // Bottom row (y=1 in image): Blue (0,0,255), White (255,255,255) + 2 pad bytes
        data.extend_from_slice(&[255, 0, 0]); // B, G, R -> Blue
        data.extend_from_slice(&[255, 255, 255]); // White
        data.extend_from_slice(&[0, 0]); // padding to 8 bytes

        // Top row (y=0 in image): Red (255,0,0), Green (0,255,0) + 2 pad bytes
        data.extend_from_slice(&[0, 0, 255]); // Red
        data.extend_from_slice(&[0, 255, 0]); // Green
        data.extend_from_slice(&[0, 0]); // padding to 8 bytes

        let img = decode_image(&data, &limits).unwrap();
        assert_eq!(img.width, 2);
        assert_eq!(img.height, 2);
        assert_eq!(img.get_pixel(0, 0), Some(0xFFFF0000)); // Red
        assert_eq!(img.get_pixel(1, 0), Some(0xFF00FF00)); // Green
        assert_eq!(img.get_pixel(0, 1), Some(0xFF0000FF)); // Blue
        assert_eq!(img.get_pixel(1, 1), Some(0xFFFFFFFF)); // White
    }

    #[test]
    fn test_decode_qoi_2x2() {
        let limits = EngineLimits::default();
        let mut data = Vec::new();
        // Magic
        data.extend_from_slice(b"qoif");
        // Width 2, Height 2
        data.extend_from_slice(&2u32.to_be_bytes());
        data.extend_from_slice(&2u32.to_be_bytes());
        data.push(3); // 3 channels RGB
        data.push(0); // colorspace sRGB

        // Pixel 0,0: QOI_OP_RGB Red (255, 0, 0)
        data.push(0xFE);
        data.extend_from_slice(&[255, 0, 0]);

        // Pixel 1,0: QOI_OP_RGB Green (0, 255, 0)
        data.push(0xFE);
        data.extend_from_slice(&[0, 255, 0]);

        // Pixel 0,1: QOI_OP_RGB Blue (0, 0, 255)
        data.push(0xFE);
        data.extend_from_slice(&[0, 0, 255]);

        // Pixel 1,1: QOI_OP_RGB White (255, 255, 255)
        data.push(0xFE);
        data.extend_from_slice(&[255, 255, 255]);

        // 8 bytes end marker (7x 0x00, 1x 0x01)
        data.extend_from_slice(&[0, 0, 0, 0, 0, 0, 0, 1]);

        let img = decode_image(&data, &limits).unwrap();
        assert_eq!(img.width, 2);
        assert_eq!(img.height, 2);
        assert_eq!(img.get_pixel(0, 0), Some(0xFFFF0000));
        assert_eq!(img.get_pixel(1, 0), Some(0xFF00FF00));
        assert_eq!(img.get_pixel(0, 1), Some(0xFF0000FF));
        assert_eq!(img.get_pixel(1, 1), Some(0xFFFFFFFF));
    }
}
