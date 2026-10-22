// analysis.rs - runs the library pipeline for the GUI and builds textures
use std::path::Path;

use eframe::egui;
use image::imageops;
use shapecomplexity::gui_api::{analyze, FILLED_RGBA, LEAF_RGBA, REMOVED_RGBA};
use shapecomplexity::MaskSource;

use crate::state::{AnalysisResult, PackedMask, CLASS_BACKGROUND, CLASS_FILLED, CLASS_LEAF, CLASS_REMOVED};

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

    /// The one library pipeline (`measure_traced`), reduced to what the GUI draws.
    pub fn analyze_image(&self, image_path: &Path, source: MaskSource) -> Result<AnalysisResult, String> {
        let r = analyze(image_path, source)?;
        let (norm, lp) = (&r.trace.norm, &r.trace.lp);
        let classes = norm.data.iter().zip(&lp.data).map(|(n, l)| match (*n, *l) {
            (true, true) => CLASS_LEAF,
            (true, false) => CLASS_REMOVED,
            (false, true) => CLASS_FILLED,
            (false, false) => CLASS_BACKGROUND,
        });
        Ok(AnalysisResult {
            measurement: r.measurement,
            mask: PackedMask::from_classes(norm.cols, norm.rows, classes),
            outline: r.outline_xy,
            blade_line: r.blade_line_xy,
            margin_profile: r.trace.margin_profile,
            error_curve: r.trace.error_curve,
        })
    }
}

impl Default for AnalysisEngine {
    fn default() -> Self {
        Self::new()
    }
}

/// Texture of a stored mask: leaf grey, removed petiole orange, filled holes light grey, background transparent.
pub fn mask_texture(mask: &PackedMask, name: String, ctx: &egui::Context) -> egui::TextureHandle {
    let mut rgba = Vec::with_capacity(mask.width * mask.height * 4);
    for i in 0..mask.width * mask.height {
        rgba.extend_from_slice(&match mask.class(i) {
            CLASS_LEAF => LEAF_RGBA,
            CLASS_REMOVED => REMOVED_RGBA,
            CLASS_FILLED => FILLED_RGBA,
            _ => [0, 0, 0, 0],
        });
    }
    let color = egui::ColorImage::from_rgba_unmultiplied([mask.width, mask.height], &rgba);
    ctx.load_texture(name, color, egui::TextureOptions::LINEAR)
}
