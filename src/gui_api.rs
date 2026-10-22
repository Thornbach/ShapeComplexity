//! Egui-free, front-end-agnostic analysis entry point.
//!
//! Runs the one v2 pipeline (`v2::measure_traced`) and returns plain data: the numbers, an RGBA picture of the
//! normalised mask and the outline and blade line as point lists in the same pixel frame. Any front-end can draw
//! these without sharing `image`-crate types across version boundaries.

use std::path::Path;

use crate::batch::{grey_share_of, measure_file_leaves, Leaf, MaskCheck};
use crate::v2::{measure_traced, Grid, MaskSource, Measurement, Trace, PAD_CONTOUR};

/// A straight (non-premultiplied) RGBA buffer.
pub struct MorphOverlay {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>, // width * height * 4
}

/// Colours of the mask picture (RGBA), as in Fig. 1 of the paper; drawn on white.
pub const LEAF_RGBA: [u8; 4] = [154, 152, 144, 255];
/// Pixels of the normalised mask that are not in L' (the petiole, if one was found).
pub const REMOVED_RGBA: [u8; 4] = [11, 11, 11, 255];
/// Pixels of L' that are not in the normalised mask (filled holes).
pub const FILLED_RGBA: [u8; 4] = [205, 203, 195, 255];

/// Full analysis output for one leaf of an image.
pub struct MorphResult {
    /// 1, 2, ... (see `batch::Leaf`); 1 for the largest-region run.
    pub leaf: usize,
    /// Mean pixel centre (x, y) of the leaf region in input pixels; NaN for the largest-region run.
    pub centroid: [f64; 2],
    /// Pixels of the measured region.
    pub px: usize,
    /// The rule applied (`Auto` resolved, D117).
    pub source: MaskSource,
    pub measurement: Measurement,
    pub trace: Trace,
    /// Mask check (dropped area, non-binary `dark` input); warnings only.
    pub check: MaskCheck,
    /// Normalised mask: leaf, removed petiole and filled holes on a transparent background.
    pub mask_image: MorphOverlay,
    /// Outline and blade line as (x, y) in pixel units of `mask_image` (0 = left/top edge of the picture).
    pub outline_xy: Vec<[f32; 2]>,
    pub blade_line_xy: Vec<[f32; 2]>,
}

/// Run the analysis on one image file (largest region).
pub fn analyze(image_path: &Path, source: MaskSource) -> Result<MorphResult, String> {
    Ok(analyze_leaves(image_path, source, false)?.remove(0))
}

/// Run the analysis on the leaves of one image file: the largest region, or every leaf region (`all_leaves`, D116).
pub fn analyze_leaves(image_path: &Path, source: MaskSource, all_leaves: bool) -> Result<Vec<MorphResult>, String> {
    let (leaves, check) = measure_file_leaves(image_path, source, all_leaves)?;
    Ok(leaves.into_iter().map(|leaf| build(leaf, check.clone())).collect())
}

/// Same as [`analyze`] but from an RGBA buffer already in memory (`width * height * 4` bytes).
pub fn analyze_rgba(rgba: &[u8], width: u32, height: u32, source: MaskSource) -> Result<MorphResult, String> {
    let (w, h) = (width as usize, height as usize);
    if rgba.len() != w * h * 4 {
        return Err("analyze_rgba: buffer is not width*height*4 bytes".into());
    }
    let grey = |p: &[u8]| image::Pixel::to_luma(&image::Rgb([p[0], p[1], p[2]])).0[0];
    let source = match source {
        MaskSource::Auto if rgba.chunks_exact(4).any(|p| p[3] <= 127) => MaskSource::Alpha,
        MaskSource::Auto => MaskSource::Dark,
        s => s,
    };
    let data = rgba
        .chunks_exact(4)
        .map(|p| match source {
            MaskSource::Alpha => p[3] > 127,
            _ => grey(p) < 128 && p[3] > 127,
        })
        .collect();
    let (measurement, trace) = measure_traced(&Grid::from_vec(h, w, data)).ok_or("no leaf pixels")?;
    let check = MaskCheck {
        dropped_share: trace.placement.as_ref().map_or(0.0, |p| p.dropped_share()),
        grey_share: (source == MaskSource::Dark)
            .then(|| grey_share_of(rgba.chunks_exact(4).filter(|p| p[3] > 127).map(grey))),
    };
    let px = trace.placement.as_ref().map_or(0, |p| p.kept_px);
    Ok(build(Leaf { index: 1, centroid: [f64::NAN; 2], px, source, measurement, trace }, check))
}

fn build(leaf: Leaf, check: MaskCheck) -> MorphResult {
    let Leaf { index, centroid, px, source, measurement, trace } = leaf;
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
    MorphResult { leaf: index, centroid, px, source, measurement, trace, check, mask_image, outline_xy, blade_line_xy }
}
