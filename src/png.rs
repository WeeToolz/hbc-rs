//! Bounded, pure Rust PNG decoding for HBC icons and theme images.

use alloc::vec::Vec;
use miniz_oxide::inflate::decompress_to_vec_zlib_with_limit;

const PNG_SIGNATURE: &[u8; 8] = b"\x89PNG\r\n\x1a\n";
const IHDR: [u8; 4] = *b"IHDR";
const PLTE: [u8; 4] = *b"PLTE";
const IDAT: [u8; 4] = *b"IDAT";
const IEND: [u8; 4] = *b"IEND";
const TRNS: [u8; 4] = *b"tRNS";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PngLimits {
    pub max_width: u32,
    pub max_height: u32,
    pub max_pixels: usize,
    pub max_input_bytes: usize,
}

impl Default for PngLimits {
    fn default() -> Self {
        Self {
            max_width: 4096,
            max_height: 4096,
            max_pixels: 8 * 1024 * 1024,
            max_input_bytes: 20 * 1024 * 1024,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RgbaImage {
    pub width: u32,
    pub height: u32,
    /// Linear RGBA8 rows, from top to bottom.
    pub pixels: Vec<u8>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PngError {
    InputTooLarge,
    InvalidSignature,
    Truncated,
    InvalidChunk,
    InvalidCrc,
    MissingHeader,
    MissingImageData,
    MissingEnd,
    InvalidDimensions,
    UnsupportedFormat,
    InvalidPalette,
    InvalidTransparency,
    UnknownCriticalChunk,
    InvalidCompressedData,
    InvalidScanline,
    InvalidPaletteIndex,
    AllocationFailed,
    InvalidTextureDimensions,
}

#[derive(Clone, Copy)]
struct Header {
    width: usize,
    height: usize,
    bit_depth: u8,
    color_type: u8,
    interlace: u8,
}

struct Chunks {
    header: Header,
    palette: Option<Vec<u8>>,
    transparency: Option<Vec<u8>>,
    compressed: Vec<u8>,
}

/// Decode a non-animated PNG to RGBA8 after applying dimension and size caps.
pub fn decode_png(data: &[u8], limits: PngLimits) -> Result<RgbaImage, PngError> {
    let chunks = parse_chunks(data, limits)?;
    let header = chunks.header;
    let pixels = header
        .width
        .checked_mul(header.height)
        .filter(|pixels| *pixels <= limits.max_pixels)
        .ok_or(PngError::InvalidDimensions)?;
    let output_len = pixels.checked_mul(4).ok_or(PngError::InvalidDimensions)?;
    let expected_inflated = inflated_size(header)?;
    let inflated = decompress_to_vec_zlib_with_limit(&chunks.compressed, expected_inflated)
        .map_err(|_| PngError::InvalidCompressedData)?;
    if inflated.len() != expected_inflated {
        return Err(PngError::InvalidCompressedData);
    }

    let mut rgba = Vec::new();
    rgba.try_reserve_exact(output_len)
        .map_err(|_| PngError::AllocationFailed)?;
    rgba.resize(output_len, 0);
    decode_passes(&chunks, &inflated, &mut rgba)?;
    Ok(RgbaImage {
        width: header.width as u32,
        height: header.height as u32,
        pixels: rgba,
    })
}

/// Reorder RGBA8 pixels into GX's 4x4 tiled RGBA8 texture layout.
pub fn to_gx_rgba8(image: &RgbaImage) -> Result<Vec<u8>, PngError> {
    let width = usize::try_from(image.width).map_err(|_| PngError::InvalidTextureDimensions)?;
    let height = usize::try_from(image.height).map_err(|_| PngError::InvalidTextureDimensions)?;
    let expected_len = width
        .checked_mul(height)
        .and_then(|pixels| pixels.checked_mul(4))
        .ok_or(PngError::InvalidTextureDimensions)?;
    if image.width == 0
        || image.height == 0
        || image.width % 4 != 0
        || image.height % 4 != 0
        || image.pixels.len() != expected_len
    {
        return Err(PngError::InvalidTextureDimensions);
    }
    let mut output = Vec::new();
    output
        .try_reserve_exact(image.pixels.len())
        .map_err(|_| PngError::AllocationFailed)?;
    for tile_y in (0..height).step_by(4) {
        for tile_x in (0..width).step_by(4) {
            // GX stores each 4x4 tile as an A/R plane followed by a G/B plane.
            for row in 0..4 {
                for column in 0..4 {
                    let offset = ((tile_y + row) * width + tile_x + column) * 4;
                    output.extend_from_slice(&[image.pixels[offset + 3], image.pixels[offset]]);
                }
            }
            for row in 0..4 {
                for column in 0..4 {
                    let offset = ((tile_y + row) * width + tile_x + column) * 4;
                    output.extend_from_slice(&[image.pixels[offset + 1], image.pixels[offset + 2]]);
                }
            }
        }
    }
    Ok(output)
}

fn parse_chunks(data: &[u8], limits: PngLimits) -> Result<Chunks, PngError> {
    if data.len() > limits.max_input_bytes {
        return Err(PngError::InputTooLarge);
    }
    if data.get(..8) != Some(PNG_SIGNATURE) {
        return Err(PngError::InvalidSignature);
    }

    let mut position = 8usize;
    let mut header = None;
    let mut palette = None;
    let mut transparency = None;
    let mut compressed = Vec::new();
    let mut data_finished = false;
    let mut saw_idat = false;
    let mut saw_iend = false;

    while position < data.len() {
        let chunk_start = position;
        let length = read_u32(data, position)? as usize;
        position = position.checked_add(4).ok_or(PngError::InvalidChunk)?;
        let chunk_type: [u8; 4] = data
            .get(position..position.checked_add(4).ok_or(PngError::InvalidChunk)?)
            .ok_or(PngError::Truncated)?
            .try_into()
            .map_err(|_| PngError::Truncated)?;
        position += 4;
        let payload_end = position.checked_add(length).ok_or(PngError::InvalidChunk)?;
        let payload = data.get(position..payload_end).ok_or(PngError::Truncated)?;
        let crc_end = payload_end.checked_add(4).ok_or(PngError::InvalidChunk)?;
        let expected_crc = read_u32(data, payload_end)?;
        let mut hasher = crc32fast::Hasher::new();
        hasher.update(&chunk_type);
        hasher.update(payload);
        if hasher.finalize() != expected_crc {
            return Err(PngError::InvalidCrc);
        }
        position = crc_end;

        if header.is_none() && chunk_type != IHDR {
            return Err(PngError::MissingHeader);
        }
        if saw_idat && chunk_type != IDAT {
            data_finished = true;
        }
        if data_finished && chunk_type == IDAT {
            return Err(PngError::InvalidChunk);
        }

        match chunk_type {
            IHDR => {
                if header.is_some() || chunk_start != 8 || payload.len() != 13 {
                    return Err(PngError::InvalidChunk);
                }
                header = Some(parse_header(payload, limits)?);
            }
            PLTE => {
                if saw_idat
                    || palette.is_some()
                    || payload.is_empty()
                    || payload.len() > 768
                    || payload.len() % 3 != 0
                {
                    return Err(PngError::InvalidPalette);
                }
                palette = Some(payload.to_vec());
            }
            TRNS => {
                if saw_idat || transparency.is_some() {
                    return Err(PngError::InvalidTransparency);
                }
                transparency = Some(payload.to_vec());
            }
            IDAT => {
                if saw_iend {
                    return Err(PngError::InvalidChunk);
                }
                saw_idat = true;
                let new_len = compressed
                    .len()
                    .checked_add(payload.len())
                    .filter(|len| *len <= limits.max_input_bytes)
                    .ok_or(PngError::InputTooLarge)?;
                compressed
                    .try_reserve(payload.len())
                    .map_err(|_| PngError::AllocationFailed)?;
                compressed.extend_from_slice(payload);
                debug_assert_eq!(compressed.len(), new_len);
            }
            IEND => {
                if payload.len() != 0 || !saw_idat {
                    return Err(PngError::InvalidChunk);
                }
                saw_iend = true;
                if position != data.len() {
                    return Err(PngError::InvalidChunk);
                }
                break;
            }
            _ => {
                if chunk_type[0] & 0x20 == 0 {
                    return Err(PngError::UnknownCriticalChunk);
                }
            }
        }
    }

    let header = header.ok_or(PngError::MissingHeader)?;
    if !saw_idat || compressed.is_empty() {
        return Err(PngError::MissingImageData);
    }
    if !saw_iend {
        return Err(PngError::MissingEnd);
    }
    validate_palette_and_transparency(header, palette.as_deref(), transparency.as_deref())?;
    Ok(Chunks {
        header,
        palette,
        transparency,
        compressed,
    })
}

fn parse_header(payload: &[u8], limits: PngLimits) -> Result<Header, PngError> {
    let width = read_u32(payload, 0)? as usize;
    let height = read_u32(payload, 4)? as usize;
    let bit_depth = payload[8];
    let color_type = payload[9];
    if width == 0
        || height == 0
        || width > limits.max_width as usize
        || height > limits.max_height as usize
        || width
            .checked_mul(height)
            .map_or(true, |count| count > limits.max_pixels)
    {
        return Err(PngError::InvalidDimensions);
    }
    if payload[10] != 0 || payload[11] != 0 || payload[12] > 1 {
        return Err(PngError::UnsupportedFormat);
    }
    let depth_valid = match color_type {
        0 => matches!(bit_depth, 1 | 2 | 4 | 8 | 16),
        2 => matches!(bit_depth, 8 | 16),
        3 => matches!(bit_depth, 1 | 2 | 4 | 8),
        4 => matches!(bit_depth, 8 | 16),
        6 => matches!(bit_depth, 8 | 16),
        _ => false,
    };
    if !depth_valid {
        return Err(PngError::UnsupportedFormat);
    }
    Ok(Header {
        width,
        height,
        bit_depth,
        color_type,
        interlace: payload[12],
    })
}

fn validate_palette_and_transparency(
    header: Header,
    palette: Option<&[u8]>,
    transparency: Option<&[u8]>,
) -> Result<(), PngError> {
    if header.color_type == 3 {
        let palette = palette.ok_or(PngError::InvalidPalette)?;
        let colors = palette.len() / 3;
        if colors > (1usize << header.bit_depth) {
            return Err(PngError::InvalidPalette);
        }
        if transparency.is_some_and(|alpha| alpha.len() > colors) {
            return Err(PngError::InvalidTransparency);
        }
    } else if let Some(transparency) = transparency {
        let required = match header.color_type {
            0 => 2,
            2 => 6,
            _ => return Err(PngError::InvalidTransparency),
        };
        if transparency.len() != required {
            return Err(PngError::InvalidTransparency);
        }
    }
    if let Some(palette) = palette {
        if header.color_type == 0 || header.color_type == 4 {
            return Err(PngError::InvalidPalette);
        }
        if palette.len() % 3 != 0 {
            return Err(PngError::InvalidPalette);
        }
    }
    Ok(())
}

fn inflated_size(header: Header) -> Result<usize, PngError> {
    let passes: &[(usize, usize, usize, usize)] = if header.interlace == 0 {
        &[(0, 0, 1, 1)]
    } else {
        &[
            (0, 0, 8, 8),
            (4, 0, 8, 8),
            (0, 4, 4, 8),
            (2, 0, 4, 4),
            (0, 2, 2, 4),
            (1, 0, 2, 2),
            (0, 1, 1, 2),
        ]
    };
    let channels = usize::from(channels(header.color_type));
    passes
        .iter()
        .try_fold(0usize, |total, (x, y, dx, dy)| {
            let width = pass_extent(header.width, *x, *dx);
            let height = pass_extent(header.height, *y, *dy);
            if width == 0 || height == 0 {
                return Some(total);
            }
            let row_bytes = width
                .checked_mul(channels)?
                .checked_mul(usize::from(header.bit_depth))?
                .checked_add(7)?
                / 8;
            total.checked_add(height.checked_mul(row_bytes.checked_add(1)?)?)
        })
        .ok_or(PngError::InvalidDimensions)
}

fn decode_passes(chunks: &Chunks, inflated: &[u8], output: &mut [u8]) -> Result<(), PngError> {
    let header = chunks.header;
    let passes: &[(usize, usize, usize, usize)] = if header.interlace == 0 {
        &[(0, 0, 1, 1)]
    } else {
        &[
            (0, 0, 8, 8),
            (4, 0, 8, 8),
            (0, 4, 4, 8),
            (2, 0, 4, 4),
            (0, 2, 2, 4),
            (1, 0, 2, 2),
            (0, 1, 1, 2),
        ]
    };
    let bytes_per_pixel =
        (usize::from(channels(header.color_type)) * usize::from(header.bit_depth) + 7) / 8;
    let bytes_per_pixel = bytes_per_pixel.max(1);
    let mut cursor = 0usize;
    for &(start_x, start_y, step_x, step_y) in passes {
        let width = pass_extent(header.width, start_x, step_x);
        let height = pass_extent(header.height, start_y, step_y);
        if width == 0 || height == 0 {
            continue;
        }
        let row_bytes = width
            .checked_mul(usize::from(channels(header.color_type)))
            .and_then(|value| value.checked_mul(usize::from(header.bit_depth)))
            .and_then(|value| value.checked_add(7))
            .ok_or(PngError::InvalidDimensions)?
            / 8;
        let mut previous = Vec::new();
        previous
            .try_reserve_exact(row_bytes)
            .map_err(|_| PngError::AllocationFailed)?;
        previous.resize(row_bytes, 0);
        let mut current = Vec::new();
        current
            .try_reserve_exact(row_bytes)
            .map_err(|_| PngError::AllocationFailed)?;
        current.resize(row_bytes, 0);

        for row_index in 0..height {
            let filter = *inflated.get(cursor).ok_or(PngError::InvalidScanline)?;
            cursor += 1;
            let encoded_end = cursor
                .checked_add(row_bytes)
                .ok_or(PngError::InvalidScanline)?;
            let encoded = inflated
                .get(cursor..encoded_end)
                .ok_or(PngError::InvalidScanline)?;
            unfilter_row(filter, encoded, &previous, &mut current, bytes_per_pixel)?;
            write_row(
                chunks, &current, start_x, start_y, step_x, step_y, row_index, output,
            )?;
            cursor = encoded_end;
            core::mem::swap(&mut current, &mut previous);
        }
    }
    if cursor != inflated.len() {
        return Err(PngError::InvalidScanline);
    }
    Ok(())
}

fn unfilter_row(
    filter: u8,
    encoded: &[u8],
    previous: &[u8],
    output: &mut [u8],
    bytes_per_pixel: usize,
) -> Result<(), PngError> {
    if encoded.len() != output.len() || previous.len() != output.len() {
        return Err(PngError::InvalidScanline);
    }
    for index in 0..encoded.len() {
        let left = if index >= bytes_per_pixel {
            output[index - bytes_per_pixel]
        } else {
            0
        };
        let above = previous[index];
        let upper_left = if index >= bytes_per_pixel {
            previous[index - bytes_per_pixel]
        } else {
            0
        };
        let predictor = match filter {
            0 => 0,
            1 => left,
            2 => above,
            3 => ((u16::from(left) + u16::from(above)) / 2) as u8,
            4 => paeth(left, above, upper_left),
            _ => return Err(PngError::InvalidScanline),
        };
        output[index] = encoded[index].wrapping_add(predictor);
    }
    Ok(())
}

fn paeth(left: u8, above: u8, upper_left: u8) -> u8 {
    let left = i32::from(left);
    let above = i32::from(above);
    let upper_left = i32::from(upper_left);
    let estimate = left + above - upper_left;
    let left_distance = (estimate - left).abs();
    let above_distance = (estimate - above).abs();
    let upper_left_distance = (estimate - upper_left).abs();
    if left_distance <= above_distance && left_distance <= upper_left_distance {
        left as u8
    } else if above_distance <= upper_left_distance {
        above as u8
    } else {
        upper_left as u8
    }
}

fn write_row(
    chunks: &Chunks,
    row: &[u8],
    start_x: usize,
    start_y: usize,
    step_x: usize,
    step_y: usize,
    row_index: usize,
    output: &mut [u8],
) -> Result<(), PngError> {
    let header = chunks.header;
    let channels = usize::from(channels(header.color_type));
    let width = pass_extent(header.width, start_x, step_x);
    for column in 0..width {
        let sample_start = column * channels;
        let sample = |offset| read_sample(row, sample_start + offset, header.bit_depth);
        let (red, green, blue, alpha) = match header.color_type {
            0 => {
                let gray_sample = sample(0)?;
                let gray = sample_to_u8(gray_sample, header.bit_depth);
                let transparent = chunks
                    .transparency
                    .as_ref()
                    .is_some_and(|value| u16::from_be_bytes([value[0], value[1]]) == gray_sample);
                (gray, gray, gray, if transparent { 0 } else { 255 })
            }
            2 => {
                let red_sample = sample(0)?;
                let green_sample = sample(1)?;
                let blue_sample = sample(2)?;
                let transparent = chunks.transparency.as_ref().is_some_and(|value| {
                    u16::from_be_bytes([value[0], value[1]]) == red_sample
                        && u16::from_be_bytes([value[2], value[3]]) == green_sample
                        && u16::from_be_bytes([value[4], value[5]]) == blue_sample
                });
                (
                    sample_to_u8(red_sample, header.bit_depth),
                    sample_to_u8(green_sample, header.bit_depth),
                    sample_to_u8(blue_sample, header.bit_depth),
                    if transparent { 0 } else { 255 },
                )
            }
            3 => {
                let palette_index = usize::from(sample(0)?);
                let palette = chunks.palette.as_ref().ok_or(PngError::InvalidPalette)?;
                let index = palette_index * 3;
                if index + 2 >= palette.len() {
                    return Err(PngError::InvalidPaletteIndex);
                }
                let alpha = chunks
                    .transparency
                    .as_ref()
                    .and_then(|values| values.get(palette_index))
                    .copied()
                    .unwrap_or(255);
                (
                    palette[index],
                    palette[index + 1],
                    palette[index + 2],
                    alpha,
                )
            }
            4 => {
                let gray = sample_to_u8(sample(0)?, header.bit_depth);
                let alpha = sample_to_u8(sample(1)?, header.bit_depth);
                (gray, gray, gray, alpha)
            }
            6 => (
                sample_to_u8(sample(0)?, header.bit_depth),
                sample_to_u8(sample(1)?, header.bit_depth),
                sample_to_u8(sample(2)?, header.bit_depth),
                sample_to_u8(sample(3)?, header.bit_depth),
            ),
            _ => return Err(PngError::UnsupportedFormat),
        };
        let x = start_x + column * step_x;
        let y = start_y + row_index * step_y;
        let target = (y * header.width + x) * 4;
        output[target..target + 4].copy_from_slice(&[red, green, blue, alpha]);
    }
    Ok(())
}

fn channels(color_type: u8) -> u8 {
    match color_type {
        0 | 3 => 1,
        2 => 3,
        4 => 2,
        6 => 4,
        _ => 0,
    }
}

fn read_sample(row: &[u8], sample_index: usize, bit_depth: u8) -> Result<u16, PngError> {
    match bit_depth {
        1 | 2 | 4 => {
            let bit_offset = sample_index
                .checked_mul(usize::from(bit_depth))
                .ok_or(PngError::InvalidScanline)?;
            let byte = *row.get(bit_offset / 8).ok_or(PngError::InvalidScanline)?;
            let shift = 8 - usize::from(bit_depth) - bit_offset % 8;
            Ok(u16::from((byte >> shift) & ((1 << bit_depth) - 1)))
        }
        8 => Ok(u16::from(
            *row.get(sample_index).ok_or(PngError::InvalidScanline)?,
        )),
        16 => {
            let offset = sample_index
                .checked_mul(2)
                .ok_or(PngError::InvalidScanline)?;
            let bytes = row
                .get(offset..offset + 2)
                .ok_or(PngError::InvalidScanline)?;
            Ok(u16::from_be_bytes([bytes[0], bytes[1]]))
        }
        _ => Err(PngError::UnsupportedFormat),
    }
}

fn sample_to_u8(sample: u16, bit_depth: u8) -> u8 {
    match bit_depth {
        16 => (sample >> 8) as u8,
        8 => sample as u8,
        depth => {
            let maximum = (1u16 << depth) - 1;
            ((u32::from(sample) * 255 + u32::from(maximum) / 2) / u32::from(maximum)) as u8
        }
    }
}

fn pass_extent(size: usize, start: usize, step: usize) -> usize {
    if size <= start {
        0
    } else {
        (size - start + step - 1) / step
    }
}

fn read_u32(data: &[u8], offset: usize) -> Result<u32, PngError> {
    let bytes = data
        .get(offset..offset.checked_add(4).ok_or(PngError::Truncated)?)
        .ok_or(PngError::Truncated)?;
    Ok(u32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn chunk(kind: &[u8; 4], payload: &[u8], out: &mut Vec<u8>) {
        out.extend_from_slice(&(payload.len() as u32).to_be_bytes());
        out.extend_from_slice(kind);
        out.extend_from_slice(payload);
        let mut hasher = crc32fast::Hasher::new();
        hasher.update(kind);
        hasher.update(payload);
        out.extend_from_slice(&hasher.finalize().to_be_bytes());
    }

    fn png(width: u32, height: u32, depth: u8, color: u8, interlace: u8, raw: &[u8]) -> Vec<u8> {
        let mut zlib = vec![0x78, 0x01];
        let mut offset = 0;
        while offset < raw.len() {
            let count = (raw.len() - offset).min(65535);
            let final_block = offset + count == raw.len();
            zlib.push(u8::from(final_block));
            zlib.extend_from_slice(&(count as u16).to_le_bytes());
            zlib.extend_from_slice(&(!(count as u16)).to_le_bytes());
            zlib.extend_from_slice(&raw[offset..offset + count]);
            offset += count;
        }
        let mut a = 1u32;
        let mut b = 0u32;
        for byte in raw {
            a = (a + u32::from(*byte)) % 65521;
            b = (b + a) % 65521;
        }
        zlib.extend_from_slice(&((b << 16) | a).to_be_bytes());
        let mut out = PNG_SIGNATURE.to_vec();
        let mut ihdr = Vec::new();
        ihdr.extend_from_slice(&width.to_be_bytes());
        ihdr.extend_from_slice(&height.to_be_bytes());
        ihdr.extend_from_slice(&[depth, color, 0, 0, interlace]);
        chunk(&IHDR, &ihdr, &mut out);
        chunk(&IDAT, &zlib, &mut out);
        chunk(&IEND, &[], &mut out);
        out
    }

    fn rgba_limits(width: u32, height: u32) -> PngLimits {
        PngLimits {
            max_width: width,
            max_height: height,
            max_pixels: (width * height) as usize,
            max_input_bytes: 1024 * 1024,
        }
    }

    #[test]
    fn decodes_rgba_and_preserves_alpha() {
        let image = decode_png(
            &png(1, 1, 8, 6, 0, &[0, 220, 80, 40, 128]),
            rgba_limits(1, 1),
        )
        .unwrap();
        assert_eq!(image.pixels, [220, 80, 40, 128]);
    }

    #[test]
    fn expands_packed_grayscale_and_palette_transparency() {
        let gray = decode_png(&png(4, 1, 2, 0, 0, &[0, 0b00_01_10_11]), rgba_limits(4, 1)).unwrap();
        assert_eq!(
            gray.pixels,
            [0, 0, 0, 255, 85, 85, 85, 255, 170, 170, 170, 255, 255, 255, 255, 255]
        );

        let raw = [0, 0b00_01_10_00];
        let mut data = PNG_SIGNATURE.to_vec();
        let mut ihdr = Vec::new();
        ihdr.extend_from_slice(&4u32.to_be_bytes());
        ihdr.extend_from_slice(&1u32.to_be_bytes());
        ihdr.extend_from_slice(&[2, 3, 0, 0, 0]);
        chunk(&IHDR, &ihdr, &mut data);
        chunk(&PLTE, &[255, 0, 0, 0, 255, 0, 0, 0, 255], &mut data);
        chunk(&TRNS, &[255, 128, 0], &mut data);
        let compressed = png(4, 1, 2, 3, 0, &raw);
        let idat_start = PNG_SIGNATURE.len() + 25;
        let idat_length =
            u32::from_be_bytes(compressed[idat_start..idat_start + 4].try_into().unwrap()) as usize;
        let idat_payload = &compressed[idat_start + 8..idat_start + 8 + idat_length];
        chunk(&IDAT, idat_payload, &mut data);
        chunk(&IEND, &[], &mut data);
        let palette_image = decode_png(&data, rgba_limits(4, 1)).unwrap();
        assert_eq!(
            palette_image.pixels,
            [255, 0, 0, 255, 0, 255, 0, 128, 0, 0, 255, 0, 255, 0, 0, 255]
        );
    }

    #[test]
    fn decodes_all_png_row_filters() {
        let first = [10, 20, 30];
        let second = [20, 40, 60];
        let mut raw = vec![0];
        raw.extend_from_slice(&first);
        raw.push(1);
        raw.extend_from_slice(&[20, 40, 60]);
        raw.push(2);
        raw.extend_from_slice(&[0, 0, 0]);
        raw.push(3);
        raw.extend_from_slice(&[10, 20, 30]);
        raw.push(4);
        raw.extend_from_slice(&[0, 0, 0]);
        let image = decode_png(&png(1, 5, 8, 2, 0, &raw), rgba_limits(1, 5)).unwrap();
        let expected = [
            [10, 20, 30, 255],
            [20, 40, 60, 255],
            [20, 40, 60, 255],
            [20, 40, 60, 255],
            [20, 40, 60, 255],
        ]
        .concat();
        assert_eq!(image.pixels, expected);
        assert_eq!(second, [20, 40, 60]);
    }

    #[test]
    fn decodes_adam7_single_pixel_and_rejects_truncated_or_bad_crc_data() {
        let image = decode_png(&png(1, 1, 8, 6, 1, &[0, 1, 2, 3, 4]), rgba_limits(1, 1)).unwrap();
        assert_eq!(image.pixels, [1, 2, 3, 4]);
        let mut truncated = png(1, 1, 8, 6, 0, &[0, 1, 2, 3, 4]);
        truncated.pop();
        assert!(decode_png(&truncated, rgba_limits(1, 1)).is_err());
        let mut bad_crc = png(1, 1, 8, 6, 0, &[0, 1, 2, 3, 4]);
        bad_crc[29] ^= 1;
        assert_eq!(
            decode_png(&bad_crc, rgba_limits(1, 1)),
            Err(PngError::InvalidCrc)
        );
    }

    #[test]
    fn converts_rgba_pixels_to_gx_tiled_ar_gb_planes() {
        let image = RgbaImage {
            width: 4,
            height: 4,
            pixels: (0..16u8)
                .flat_map(|i| [i, i + 40, i + 80, i + 120])
                .collect(),
        };
        let tiled = to_gx_rgba8(&image).unwrap();
        assert_eq!(&tiled[..8], &[120, 0, 121, 1, 122, 2, 123, 3]);
        assert_eq!(&tiled[32..40], &[40, 80, 41, 81, 42, 82, 43, 83]);
        assert_eq!(tiled.len(), 64);
    }

    #[test]
    fn rejects_dimensions_before_decode_and_non_tiled_texture_sizes() {
        assert_eq!(
            decode_png(&png(2, 1, 8, 6, 0, &[0, 0, 0, 0, 255]), rgba_limits(1, 1)),
            Err(PngError::InvalidDimensions)
        );
        let image = RgbaImage {
            width: 1,
            height: 1,
            pixels: vec![0; 4],
        };
        assert_eq!(to_gx_rgba8(&image), Err(PngError::InvalidTextureDimensions));
    }
}
