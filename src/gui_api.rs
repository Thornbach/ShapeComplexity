//! Egui-free, front-end-agnostic analysis entry point.
//!
//! Runs the one v2 pipeline (`v2::measure_traced`) and returns plain data: the numbers, an RGBA picture of the
//! normalised mask and the outline and blade line as point lists in the same pixel frame. Any front-end can draw
//! these without sharing `image`-crate types across version boundaries.

use std::path::Path;

use crate::batch::measure_file;
use crate::v2::{measure_traced, Grid, MaskSource, Measurement, Trace, PAD_CONTOUR};

/// A straight (non-premultiplied) RGBA buffer.
pub struct MorphOverlay {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>, // width * height * 4
}

/// Colours of the mask picture (RGBA).
pub const LEAF_RGBA: [u8; 4] = [70, 70, 70, 255];
/// Pixels of the normalised mask that are not in L' (the petiole, if one was found).
pub const REMOVED_RGBA: [u8; 4] = [230, 120, 0, 255];
/// Pixels of L' that are not in the normalised mask (filled holes).
pub const FILLED_RGBA: [u8; 4] = [170, 170, 170, 255];

/// Full analysis output for one image.
pub struct MorphResult {
    pub measurement: Measurement,
    pub trace: Trace,
    /// Normalised mask: leaf, removed petiole and filled holes on a transparent background.
    pub mask_image: MorphOverlay,
    /// Outline and blade line as (x, y) in pixel units of `mask_image` (0 = left/top edge of the picture).
    pub outline_xy: Vec<[f32; 2]>,
    pub blade_line_xy: Vec<[f32; 2]>,
}

/// Run the analysis on one image file.
pub fn analyze(image_path: &Path, source: MaskSource) -> Result<MorphResult, String> {
    let (measurement, trace) = measure_file(image_path, source)?;
    Ok(build(measurement, trace))
}

/// Same as [`analyze`] but from an RGBA buffer already in memory (`width * height * 4` bytes).
pub fn analyze_rgba(rgba: &[u8], width: u32, height: u32, source: MaskSource) -> Result<MorphResult, String> {
    let (w, h) = (width as usize, height as usize);
    if rgba.len() != w * h * 4 {
        return Err("analyze_rgba: buffer is not width*height*4 bytes".into());
    }
    let data = rgba
        .chunks_exact(4)
        .map(|p| match source {
            MaskSource::Alpha => p[3] > 127,
            MaskSource::Dark => {
                let luma = image::Rgb([p[0], p[1], p[2]]);
                image::Pixel::to_luma(&luma).0[0] < 128 && p[3] > 127
            }
        })
        .collect();
    let (measurement, trace) = measure_traced(&Grid::from_vec(h, w, data)).ok_or("no leaf pixels")?;
    Ok(build(measurement, trace))
}

fn build(measurement: Measurement, trace: Trace) -> MorphResult {
    let (norm, lp) = (&trace.norm, &trace.lp);
    let mut rgba = Vec::with_capacity(norm.data.len() * 4);
    for (n, l) in norm.data.iter().zip(&lp.data) {
        rgba.extend_from_slice(&match (*n, *l) {
            (true, true) => LEAF_RGBA,
            (true, false) => REMOVED_RGBA,
            (false, true) => FILLED_RGBA,
            (false, false) => [0, 0, 0, 0],
        });
    }
    let mask_image = MorphOverlay { width: norm.cols as u32, height: norm.rows as u32, rgba };
    // contour points are pixel centres (row, col) in a frame padded by PAD_CONTOUR
    let off = 0.5 - PAD_CONTOUR as f64;
    let to_xy = |pts: &[[f64; 2]]| pts.iter().map(|p| [(p[1] + off) as f32, (p[0] + off) as f32]).collect();
    let outline_xy = to_xy(&trace.outline);
    let blade_line_xy = to_xy(&trace.blade_line);
    MorphResult { measurement, trace, mask_image, outline_xy, blade_line_xy }
}
