// analysis.rs - runs the library pipeline for the GUI and builds textures
use std::path::Path;

use eframe::egui;
use image::imageops;
use shapecomplexity::gui_api::{analyze_leaves, FILLED_RGBA, LEAF_RGBA, REMOVED_RGBA};
use shapecomplexity::v2::fill::polygon_fill;
use shapecomplexity::v2::PAD_CONTOUR;
use shapecomplexity::MaskSource;

use crate::state::{AnalysisResult, PackedMask, CLASS_BACKGROUND, CLASS_FILLED, CLASS_LEAF, CLASS_REMOVED};

/// Margin band (Fig. 1 b): blade inside the blade line, leaf outside it, background inside it.
pub const BLADE_RGBA: [u8; 4] = [225, 224, 217, 255];
pub const BAND_OUT_RGBA: [u8; 4] = [235, 104, 52, 255];
pub const BAND_IN_RGBA: [u8; 4] = [247, 195, 168, 255];
/// Longest side of the input-image texture, px.
const INPUT_TEXTURE_MAX: u32 = 2048;

pub struct AnalysisEngine;

impl AnalysisEngine {
    pub fn new() -> Self {
        Self
    }

    /// Small texture of the input image for the strip at the bottom of the GUI.
    pub fn generate_thumbnail(&self, image_path: &Path, ctx: &egui::Context) -> Option<egui::TextureHandle> {
        let img = image::open(image_path).ok()?.to_rgba8();
        // fit inside 120 x 90, keeping the aspect ratio
        let (w, h) = img.dimensions();
        let s = (120.0 / w as f32).min(90.0 / h as f32).min(1.0);
        let thumb = imageops::thumbnail(&img, ((w as f32 * s) as u32).max(1), ((h as f32 * s) as u32).max(1));
        let size = [thumb.width() as usize, thumb.height() as usize];
        let color = egui::ColorImage::from_rgba_unmultiplied(size, thumb.as_raw());
        Some(ctx.load_texture(format!("{}_thumb", image_path.display()), color, egui::TextureOptions::default()))
    }

    /// The one library pipeline (`measure_traced`), reduced to what the GUI draws: the largest region, or every
    /// leaf region (`all_leaves`, D116).
    pub fn analyze_image(&self, image_path: &Path, source: MaskSource, all_leaves: bool) -> Result<Vec<AnalysisResult>, String> {
        let leaves = analyze_leaves(image_path, source, all_leaves)?;
        Ok(leaves
            .into_iter()
            .map(|r| {
                let (norm, lp) = (&r.trace.norm, &r.trace.lp);
                let classes = norm.data.iter().zip(&lp.data).map(|(n, l)| match (*n, *l) {
                    (true, true) => CLASS_LEAF,
                    (true, false) => CLASS_REMOVED,
                    (false, true) => CLASS_FILLED,
                    (false, false) => CLASS_BACKGROUND,
                });
                AnalysisResult {
                    leaf: r.leaf,
                    centroid: r.centroid,
                    px: r.px,
                    source: r.source,
                    measurement: r.measurement,
                    mask: PackedMask::from_classes(norm.cols, norm.rows, classes),
                    outline: r.trace.outline,
                    blade_line: r.trace.blade_line,
                    margin_profile: r.trace.margin_profile,
                    error_curve: r.trace.error_curve,
                    white: r.trace.white,
                    placement: r.trace.placement,
                    check: r.check,
                }
            })
            .collect())
    }
}

impl Default for AnalysisEngine {
    fn default() -> Self {
        Self::new()
    }
}

/// Texture of a stored mask. Without the band: leaf grey, removed petiole black, filled holes light grey. With the
/// band (Fig. 1 b): blade inside the blade line light grey, leaf outside it orange, background inside it peach.
/// Background transparent.
pub fn mask_texture(r: &AnalysisResult, band: bool, name: String, ctx: &egui::Context) -> egui::TextureHandle {
    let mask = &r.mask;
    // blade line: padded trace frame -> pixel centres of the mask
    let inside = band.then(|| {
        let pad = PAD_CONTOUR as f64;
        let xs: Vec<f64> = r.blade_line.iter().map(|p| p[1] - pad).collect();
        let ys: Vec<f64> = r.blade_line.iter().map(|p| p[0] - pad).collect();
        polygon_fill(mask.width, mask.height, &xs, &ys)
    });
    let mut rgba = Vec::with_capacity(mask.width * mask.height * 4);
    for i in 0..mask.width * mask.height {
        let class = mask.class(i);
        rgba.extend_from_slice(&match (class, inside.as_ref().map(|m| m.data[i])) {
            (CLASS_REMOVED, _) => REMOVED_RGBA,
            (CLASS_LEAF | CLASS_FILLED, Some(true)) => BLADE_RGBA,
            (CLASS_LEAF | CLASS_FILLED, Some(false)) => BAND_OUT_RGBA,
            (CLASS_BACKGROUND, Some(true)) => BAND_IN_RGBA,
            (CLASS_LEAF, None) => LEAF_RGBA,
            (CLASS_FILLED, None) => FILLED_RGBA,
            _ => [0, 0, 0, 0],
        });
    }
    let color = egui::ColorImage::from_rgba_unmultiplied([mask.width, mask.height], &rgba);
    ctx.load_texture(name, color, egui::TextureOptions::LINEAR)
}

/// Texture of an input image, at most INPUT_TEXTURE_MAX px a side, and the full image size (width, height).
pub fn input_texture(path: &Path, ctx: &egui::Context) -> Result<(egui::TextureHandle, [f32; 2]), String> {
    let img = image::open(path).map_err(|e| format!("{}: {e}", path.display()))?.to_rgba8();
    let (w, h) = img.dimensions();
    let s = (INPUT_TEXTURE_MAX as f32 / w.max(h) as f32).min(1.0);
    let img = if s < 1.0 {
        let (tw, th) = (((w as f32 * s) as u32).max(1), ((h as f32 * s) as u32).max(1));
        imageops::resize(&img, tw, th, imageops::FilterType::Triangle)
    } else {
        img
    };
    let color = egui::ColorImage::from_rgba_unmultiplied([img.width() as usize, img.height() as usize], img.as_raw());
    let tex = ctx.load_texture(format!("{}_input", path.display()), color, egui::TextureOptions::LINEAR);
    Ok((tex, [w as f32, h as f32]))
}
