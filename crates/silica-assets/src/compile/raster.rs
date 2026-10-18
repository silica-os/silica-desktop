//! Pixels: PNG and SVG to premultiplied RGBA, the size ladder and the encodings.
//! Integer arithmetic only, so the output is identical on every machine.

#![allow(clippy::chunks_exact_to_as_chunks)]

use crate::format::{Encoding, QOI_SIDES, RAW_SIDES};

use super::CompileError;

/// The side the sources are drawn at (the 1024 canvas of silica-icons).
pub const CANVAS: u32 = 1024;

/// A square image of premultiplied RGBA pixels.
#[derive(Clone)]
pub struct Image {
    pub side: u32,
    pub rgba: Vec<u8>,
}

fn premultiply(px: &mut [u8]) {
    let a = u32::from(px.get(3).copied().unwrap_or(255));
    for c in px.iter_mut().take(3) {
        *c = ((u32::from(*c) * a + 127) / 255) as u8;
    }
}

/// Decodes a PNG into premultiplied RGBA.
pub fn decode_png(bytes: &[u8]) -> Result<(u32, u32, Vec<u8>), CompileError> {
    let mut decoder = png::Decoder::new(std::io::Cursor::new(bytes));
    decoder.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
    let mut reader = decoder
        .read_info()
        .map_err(|e| CompileError::new(format!("png: {e}")))?;
    let mut buffer = vec![0u8; reader.output_buffer_size()];
    let info = reader
        .next_frame(&mut buffer)
        .map_err(|e| CompileError::new(format!("png: {e}")))?;
    let data = buffer.get(..info.buffer_size()).unwrap_or(&[]);
    let mut rgba = Vec::with_capacity((info.width * info.height * 4) as usize);
    match info.color_type {
        png::ColorType::Rgba => rgba.extend_from_slice(data),
        png::ColorType::Rgb => {
            for px in data.chunks_exact(3) {
                rgba.extend_from_slice(px);
                rgba.push(255);
            }
        }
        png::ColorType::GrayscaleAlpha => {
            for px in data.chunks_exact(2) {
                let g = px.first().copied().unwrap_or(0);
                rgba.extend_from_slice(&[g, g, g, px.get(1).copied().unwrap_or(255)]);
            }
        }
        png::ColorType::Grayscale => {
            for &g in data {
                rgba.extend_from_slice(&[g, g, g, 255]);
            }
        }
        png::ColorType::Indexed => {
            return Err(CompileError::new("png: indexed color not expanded"));
        }
    }
    for px in rgba.chunks_exact_mut(4) {
        premultiply(px);
    }
    Ok((info.width, info.height, rgba))
}

/// Straight (non-premultiplied) RGBA8 pixels of a `side` square image.
pub fn image_from_straight(side: u32, rgba: &[u8]) -> Image {
    let mut rgba = rgba.to_vec();
    for px in rgba.chunks_exact_mut(4) {
        premultiply(px);
    }
    Image { side, rgba }
}

/// A rendered PNG at the canvas size.
pub fn image_from_png(bytes: &[u8]) -> Result<Image, CompileError> {
    let (w, h, rgba) = decode_png(bytes)?;
    if w != CANVAS || h != CANVAS {
        return Err(CompileError::new(format!(
            "rendered PNG is {w}x{h}, expected {CANVAS}x{CANVAS}"
        )));
    }
    Ok(Image { side: CANVAS, rgba })
}

/// An SVG drawn on the canvas.
pub fn image_from_svg(bytes: &[u8]) -> Result<Image, CompileError> {
    let options = resvg::usvg::Options::default();
    let tree = resvg::usvg::Tree::from_data(bytes, &options)
        .map_err(|e| CompileError::new(format!("svg: {e}")))?;
    let size = tree.size();
    if size.width() <= 0.0 || size.height() <= 0.0 {
        return Err(CompileError::new("svg: empty size"));
    }
    let mut pixmap = resvg::tiny_skia::Pixmap::new(CANVAS, CANVAS)
        .ok_or_else(|| CompileError::new("cannot allocate the canvas"))?;
    let transform = resvg::tiny_skia::Transform::from_scale(
        CANVAS as f32 / size.width(),
        CANVAS as f32 / size.height(),
    );
    // resvg asserts on some filter regions; a source it cannot draw is an error naming the
    // file (the caller adds it), not an aborted build.
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        resvg::render(&tree, transform, &mut pixmap.as_mut());
    }))
    .map_err(|_| CompileError::new("resvg cannot render this svg (it panicked)"))?;
    Ok(Image {
        side: CANVAS,
        rgba: pixmap.take(),
    })
}

/// Draws `top` over `bottom` (both premultiplied, same size).
pub fn over(bottom: &mut Image, top: &Image) {
    for (b, t) in bottom
        .rgba
        .chunks_exact_mut(4)
        .zip(top.rgba.chunks_exact(4))
    {
        let inv = 255 - u32::from(t.get(3).copied().unwrap_or(0));
        for (bc, tc) in b.iter_mut().zip(t) {
            *bc = (u32::from(*tc) + (u32::from(*bc) * inv + 127) / 255).min(255) as u8;
        }
    }
}

/// Halves the side with a 2x2 box filter.
pub fn halve(image: &Image) -> Image {
    let side = image.side / 2;
    let stride = image.side as usize * 4;
    let mut rgba = Vec::with_capacity((side * side * 4) as usize);
    for y in 0..side as usize {
        for x in 0..side as usize {
            for c in 0..4 {
                let at = |dy: usize, dx: usize| -> u32 {
                    image
                        .rgba
                        .get((2 * y + dy) * stride + (2 * x + dx) * 4 + c)
                        .copied()
                        .map_or(0, u32::from)
                };
                rgba.push(((at(0, 0) + at(0, 1) + at(1, 0) + at(1, 1) + 2) / 4) as u8);
            }
        }
    }
    Image { side, rgba }
}

/// One stored rendition.
pub struct Rendition {
    pub side: u16,
    pub encoding: Encoding,
    pub bytes: Vec<u8>,
}

/// The renditions of a canvas-size image at every stored side: raw for 16..=128, QOI for
/// 256..=1024.
pub fn ladder(canvas: &Image) -> Result<Vec<Rendition>, CompileError> {
    let mut current = canvas.clone();
    let mut out = Vec::new();
    while current.side >= u32::from(RAW_SIDES[0]) {
        let side = current.side as u16;
        if QOI_SIDES.contains(&side) {
            let bytes = qoi::encode_to_vec(&current.rgba, current.side, current.side)
                .map_err(|e| CompileError::new(format!("qoi: {e}")))?;
            out.push(Rendition {
                side,
                encoding: Encoding::Qoi,
                bytes,
            });
        } else if RAW_SIDES.contains(&side) {
            out.push(Rendition {
                side,
                encoding: Encoding::Rgba8Premul,
                bytes: current.rgba.clone(),
            });
        }
        if current.side == u32::from(RAW_SIDES[0]) {
            break;
        }
        current = halve(&current);
    }
    out.sort_by_key(|r| r.side);
    Ok(out)
}
