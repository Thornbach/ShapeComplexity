// Application State Management
use std::collections::HashMap;
use std::path::{Path, PathBuf};

use eframe::egui;
use shapecomplexity::batch::image_files;
use shapecomplexity::{MaskSource, Measurement};

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum AnalysisStatus {
    NotStarted,
    Running,
    Completed,
    Failed,
}

/// Pixel classes of the normalised mask picture (see `PackedMask`).
pub const CLASS_BACKGROUND: u8 = 0;
pub const CLASS_LEAF: u8 = 1;
pub const CLASS_REMOVED: u8 = 2;
pub const CLASS_FILLED: u8 = 3;

/// Normalised mask stored at 2 bits per pixel (about 0.3 MB per leaf), so that large batches fit in memory.
/// The texture is built only for the image on screen.
#[derive(Clone)]
pub struct PackedMask {
    pub width: usize,
    pub height: usize,
    bits: Vec<u8>,
}

impl PackedMask {
    pub fn from_classes(width: usize, height: usize, classes: impl Iterator<Item = u8>) -> Self {
        let mut bits = vec![0u8; (width * height).div_ceil(4)];
        for (i, c) in classes.enumerate() {
            bits[i / 4] |= (c & 3) << (2 * (i % 4));
        }
        Self { width, height, bits }
    }

    pub fn class(&self, i: usize) -> u8 {
        (self.bits[i / 4] >> (2 * (i % 4))) & 3
    }
}

/// What the GUI keeps of one analysis.
#[derive(Clone)]
pub struct AnalysisResult {
    pub measurement: Measurement,
    pub mask: PackedMask,
    /// Outline and blade line as (x, y) in pixel units of the mask picture.
    pub outline: Vec<[f32; 2]>,
    pub blade_line: Vec<[f32; 2]>,
    /// h(s), px, one value per outline point.
    pub margin_profile: Vec<f64>,
    /// e(k) / sqrt(A), k = 1, 2, ...; empty if MC was not computed.
    pub error_curve: Vec<f64>,
}

pub struct ImageInfo {
    pub path: PathBuf,
    /// Path relative to the workspace folder (the `file` column of the export).
    pub label: String,
    pub thumbnail: Option<egui::TextureHandle>,
    pub status: AnalysisStatus,
    pub selected: bool,
}

pub struct AppState {
    // Workspace
    pub workspace_dir: Option<PathBuf>,
    pub images: Vec<ImageInfo>,
    pub current_image_index: Option<usize>,

    /// Which pixels are leaf. None until the user chooses: there is no default.
    pub mask_source: Option<MaskSource>,

    // Analysis
    pub analysis_results: HashMap<PathBuf, AnalysisResult>,
    pub errors: HashMap<PathBuf, String>,
    pub analysis_in_progress: bool,
    pub batch_processing: bool,
    pub current_batch_index: usize,
    pub total_batch_count: usize,

    /// Texture of the mask on screen, with the image it belongs to.
    pub mask_texture: Option<(PathBuf, egui::TextureHandle)>,

    // UI State
    /// Index of the outline point picked in the h(s) plot.
    pub selected_point: Option<usize>,
    pub show_outline: bool,
    pub show_blade_line: bool,
    pub zoom_level: f32,
    pub pan_offset: egui::Vec2,

    pub last_error: Option<String>,
    pub last_message: Option<String>,
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            workspace_dir: None,
            images: Vec::new(),
            current_image_index: None,
            mask_source: None,
            analysis_results: HashMap::new(),
            errors: HashMap::new(),
            analysis_in_progress: false,
            batch_processing: false,
            current_batch_index: 0,
            total_batch_count: 0,
            mask_texture: None,
            selected_point: None,
            show_outline: true,
            show_blade_line: true,
            zoom_level: 1.0,
            pan_offset: egui::Vec2::ZERO,
            last_error: None,
            last_message: None,
        }
    }
}

impl AppState {
    pub fn current_image(&self) -> Option<&ImageInfo> {
        self.current_image_index.and_then(|idx| self.images.get(idx))
    }

    pub fn current_result(&self) -> Option<&AnalysisResult> {
        self.current_image().and_then(|img| self.analysis_results.get(&img.path))
    }

    pub fn load_workspace(&mut self, dir: PathBuf) -> Result<(), String> {
        let files = image_files(&dir, true)?;
        self.workspace_dir = Some(dir.clone());
        self.images.clear();
        self.current_image_index = None;
        self.clear_results();
        for path in files {
            let label = shapecomplexity::batch::file_label(&path, &dir);
            self.images.push(ImageInfo {
                path,
                label,
                thumbnail: None,
                status: AnalysisStatus::NotStarted,
                selected: false,
            });
        }
        if !self.images.is_empty() {
            self.current_image_index = Some(0);
        }
        Ok(())
    }

    /// Results depend on the mask source, so a new source drops them.
    pub fn set_mask_source(&mut self, source: MaskSource) {
        if self.mask_source != Some(source) {
            self.mask_source = Some(source);
            self.clear_results();
        }
    }

    fn clear_results(&mut self) {
        self.analysis_results.clear();
        self.errors.clear();
        self.mask_texture = None;
        self.selected_point = None;
        for img in &mut self.images {
            img.status = AnalysisStatus::NotStarted;
        }
    }

    pub fn set_result(&mut self, path: &Path, result: Result<AnalysisResult, String>) {
        let status = match result {
            Ok(r) => {
                self.errors.remove(path);
                self.analysis_results.insert(path.to_path_buf(), r);
                AnalysisStatus::Completed
            }
            Err(e) => {
                self.analysis_results.remove(path);
                self.errors.insert(path.to_path_buf(), e);
                AnalysisStatus::Failed
            }
        };
        if let Some(img) = self.images.iter_mut().find(|i| i.path == path) {
            img.status = status;
        }
        if self.mask_texture.as_ref().is_some_and(|(p, _)| p == path) {
            self.mask_texture = None;
        }
    }

    pub fn select_image(&mut self, index: usize) {
        if index < self.images.len() {
            self.current_image_index = Some(index);
            self.selected_point = None;
            self.reset_view();
        }
    }

    pub fn reset_view(&mut self) {
        self.zoom_level = 1.0;
        self.pan_offset = egui::Vec2::ZERO;
    }

    pub fn select_all(&mut self) {
        for img in &mut self.images {
            img.selected = true;
        }
    }

    pub fn deselect_all(&mut self) {
        for img in &mut self.images {
            img.selected = false;
        }
    }

    pub fn get_selected_images(&self) -> Vec<PathBuf> {
        self.images.iter().filter(|img| img.selected).map(|img| img.path.clone()).collect()
    }

    pub fn selected_count(&self) -> usize {
        self.images.iter().filter(|img| img.selected).count()
    }
}
