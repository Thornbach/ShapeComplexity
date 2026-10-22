// Application State Management
use std::collections::HashMap;
use std::path::{Path, PathBuf};

use eframe::egui;
use shapecomplexity::batch::{image_files, MaskCheck};
use shapecomplexity::v2::{Flag, Placement};
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

/// What the GUI keeps of one analysed leaf.
#[derive(Clone)]
pub struct AnalysisResult {
    /// 1, 2, ... (D116); 1 for the largest-region run.
    pub leaf: usize,
    /// Mean pixel centre (x, y) of the leaf region in input pixels; NaN for the largest-region run.
    pub centroid: [f64; 2],
    /// Pixels of the measured region (for the area).
    pub px: usize,
    /// The rule applied to the image (`Auto` resolved, D117).
    pub source: MaskSource,
    pub measurement: Measurement,
    pub mask: PackedMask,
    /// Outline and blade line as (x, y) in pixel units of the mask picture.
    pub outline: Vec<[f32; 2]>,
    pub blade_line: Vec<[f32; 2]>,
    /// h(s), px, one value per outline point.
    pub margin_profile: Vec<f64>,
    /// e(k) / sqrt(A), k = 1, 2, ...; empty if MC was not computed.
    pub error_curve: Vec<f64>,
    /// Whitened blade line (row, col), px at sqrt(A) = 1000; the outline the Fourier fits approximate.
    pub white: Vec<[f64; 2]>,
    /// Where the normalised mask lies in the input image.
    pub placement: Option<Placement>,
    /// Mask check (D115): dropped area, non-binary `dark` input.
    pub check: MaskCheck,
}

impl AnalysisResult {
    /// Reasons to look at this leaf again: resolution flags other than ok, and mask-check warnings.
    pub fn concerns(&self) -> Vec<String> {
        let m = &self.measurement;
        let mut v = Vec::new();
        for (name, flag) in [("EA", m.ea_flag), ("MC", m.mc_flag)] {
            if flag != Flag::Ok {
                v.push(format!("{name} flag: {} (√A native {:.0} px)", flag.as_str(), m.sqrt_a_native));
            }
        }
        v.extend(self.check.warnings());
        v
    }
}

/// Input image on screen (at most 2048 px a side) and its full size (width, height); Err if it could not be read.
pub type InputTexture = Result<(egui::TextureHandle, [f32; 2]), String>;

/// What the right-hand panel shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CentralView {
    /// Values and Fig. 1 panels of the leaf on screen.
    Leaf,
    /// EA-MC scatter of all analysed leaves.
    Morphospace,
}

/// What the image view shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ViewMode {
    /// The normalised mask (sqrt(A) = 1000 px), as measured.
    Mask,
    /// The input image with the normalised mask placed over it.
    Input,
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

    /// Which pixels are leaf; `Auto` (default, D117) decides per image.
    pub mask_source: MaskSource,

    /// Measure every leaf region of an image (D116) instead of the largest region. Results are cleared when it changes.
    pub all_leaves: bool,
    /// Image scale for the leaf area (export column `area_mm2`); None = not given.
    pub px_per_mm: Option<f64>,
    pub px_per_mm_text: String,

    // Analysis: the leaves of each image (one unless `all_leaves`)
    pub analysis_results: HashMap<PathBuf, Vec<AnalysisResult>>,
    /// Leaf on screen (index into the image's results).
    pub current_leaf: usize,
    pub errors: HashMap<PathBuf, String>,
    pub analysis_in_progress: bool,
    pub batch_processing: bool,
    pub current_batch_index: usize,
    pub total_batch_count: usize,

    /// Texture of the mask on screen, with its image, leaf and whether it shows the margin band.
    pub mask_texture: Option<(PathBuf, usize, bool, egui::TextureHandle)>,
    /// Texture of the input image on screen, with the image it belongs to.
    pub input_texture: Option<(PathBuf, InputTexture)>,
    /// Elliptic Fourier fit on screen: image, leaf, number of harmonics, points (row, col).
    pub efd_fit: Option<(PathBuf, usize, usize, Vec<[f64; 2]>)>,

    // UI State
    /// Index of the outline point picked in the h(s) plot.
    pub selected_point: Option<usize>,
    pub view_mode: ViewMode,
    pub show_outline: bool,
    pub show_blade_line: bool,
    /// Colour the margin between the outline and the blade line (Fig. 1 b).
    pub show_band: bool,
    /// Runs of outline points (first, last index) inside the image view; empty if the whole outline is visible.
    pub visible_runs: Vec<(usize, usize)>,
    /// Harmonics of the Fourier fit shown (Fig. 1 d, e).
    pub efd_k: usize,
    pub central_view: CentralView,
    /// Subfolder coloured in the morphospace when there are more than three.
    pub highlight: Option<String>,
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
            mask_source: MaskSource::Auto,
            all_leaves: false,
            px_per_mm: None,
            px_per_mm_text: String::new(),
            analysis_results: HashMap::new(),
            current_leaf: 0,
            errors: HashMap::new(),
            analysis_in_progress: false,
            batch_processing: false,
            current_batch_index: 0,
            total_batch_count: 0,
            mask_texture: None,
            input_texture: None,
            efd_fit: None,
            selected_point: None,
            view_mode: ViewMode::Mask,
            show_outline: true,
            show_blade_line: true,
            show_band: true,
            visible_runs: Vec::new(),
            efd_k: 3,
            central_view: CentralView::Leaf,
            highlight: None,
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

    /// Leaves of the image on screen.
    pub fn current_leaves(&self) -> &[AnalysisResult] {
        self.current_image().and_then(|img| self.analysis_results.get(&img.path)).map_or(&[], |v| v.as_slice())
    }

    pub fn current_result(&self) -> Option<&AnalysisResult> {
        self.current_leaves().get(self.current_leaf)
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
        if self.mask_source != source {
            self.mask_source = source;
            self.clear_results();
        }
    }

    /// Results depend on which regions are leaves, so a change drops them.
    pub fn set_all_leaves(&mut self, all: bool) {
        if self.all_leaves != all {
            self.all_leaves = all;
            self.clear_results();
        }
    }

    /// Show leaf `leaf` of image `index`.
    pub fn open_leaf(&mut self, index: usize, leaf: usize) {
        self.select_image(index);
        self.select_leaf(leaf);
    }

    pub fn select_leaf(&mut self, i: usize) {
        if i != self.current_leaf && i < self.current_leaves().len() {
            self.current_leaf = i;
            self.selected_point = None;
            self.visible_runs.clear();
        }
    }

    fn clear_results(&mut self) {
        self.analysis_results.clear();
        self.errors.clear();
        self.mask_texture = None;
        self.efd_fit = None;
        self.selected_point = None;
        self.current_leaf = 0;
        self.visible_runs.clear();
        for img in &mut self.images {
            img.status = AnalysisStatus::NotStarted;
        }
    }

    pub fn set_result(&mut self, path: &Path, result: Result<Vec<AnalysisResult>, String>) {
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
        if self.mask_texture.as_ref().is_some_and(|(p, ..)| p == path) {
            self.mask_texture = None;
        }
        if self.efd_fit.as_ref().is_some_and(|(p, ..)| p == path) {
            self.efd_fit = None;
        }
        if self.current_image().is_some_and(|i| i.path == path) {
            self.current_leaf = 0;
        }
    }

    pub fn select_image(&mut self, index: usize) {
        if index < self.images.len() {
            self.current_image_index = Some(index);
            self.current_leaf = 0;
            self.selected_point = None;
            self.visible_runs.clear();
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
