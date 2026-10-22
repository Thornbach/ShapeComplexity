// src/v2/mod.rs - ShapeComplexity v2: edge amount (EA) and macro-shape complexity (MC)
//
// Port of the Python reference implementation `shapecomplexity_v2.py` (analysis archive of the paper).
// Every step copies the exact library algorithm the reference uses (Pillow 11.0.0, SciPy 1.13.1,
// scikit-image 0.24.0, NumPy 1.26.4); the rules are written down in PORTING.md of the analysis archive.
// Acceptance: |difference| <= 1e-9 for EA, MC, elongation and petiole length on every check mask.

pub mod contour;
pub mod fill;
pub mod mask;
pub mod metrics;
pub mod morph;
pub mod resize;

use std::path::Path;

use contour::Point;

/// Normalised sqrt(leaf area), px.
pub const SQRT_A: f64 = 1000.0;
/// Edge/macro split scale as a fraction of sqrt(A) (blade-line sigma and opening radius).
pub const R_STAR: f64 = 0.03;
/// Slack (px) of the opening at the outline.
pub const TOL: f64 = 1.5;
/// Gaussian sigma (px) on the mask before the 0.5 contour.
pub const SIGMA_PRE: f64 = 1.5;
pub const PAD_NORM: usize = 4;
pub const PAD_CONTOUR: usize = 8;
/// Petiole: minimum length (fraction of sqrt(A)) and minimum major/minor axis ratio.
pub const PET_LEN: f64 = 0.10;
pub const PET_ELONG: f64 = 4.0;
/// Harmonic cap of the elliptic Fourier reconstruction.
pub const K_MAX: usize = 64;
/// MC tolerance (fraction of sqrt(A)).
pub const TAU: f64 = R_STAR / 2.0;
/// Hausdorff stop rule (fraction of sqrt(A)); changes run time only.
pub const STOP: f64 = R_STAR / 8.0;
/// Resolution floors on the native sqrt(A) in px.
pub const FLOOR_MIN: f64 = 250.0;
pub const FLOOR_EA: f64 = 500.0;

/// A row-major 2-D array. `rows` = image height, `cols` = image width.
#[derive(Clone, Debug, PartialEq)]
pub struct Grid<T> {
    pub rows: usize,
    pub cols: usize,
    pub data: Vec<T>,
}

impl<T: Copy + Default> Grid<T> {
    pub fn new(rows: usize, cols: usize) -> Self {
        Grid { rows, cols, data: vec![T::default(); rows * cols] }
    }

    pub fn from_vec(rows: usize, cols: usize, data: Vec<T>) -> Self {
        assert_eq!(data.len(), rows * cols, "grid size does not match the data");
        Grid { rows, cols, data }
    }

    #[inline]
    pub fn get(&self, r: usize, c: usize) -> T {
        self.data[r * self.cols + c]
    }

    #[inline]
    pub fn set(&mut self, r: usize, c: usize, v: T) {
        self.data[r * self.cols + c] = v;
    }

    /// Swap rows and columns.
    pub fn transpose(&self) -> Self {
        let mut out = Grid::new(self.cols, self.rows);
        for r in 0..self.rows {
            for c in 0..self.cols {
                out.set(c, r, self.get(r, c));
            }
        }
        out
    }
}

/// A leaf mask: true = leaf.
pub type Mask = Grid<bool>;

/// Sum in NumPy's order (`np.sum` on a contiguous f64 array): blocks of 8192 added in order,
/// each block summed pairwise.
pub fn np_sum(a: &[f64]) -> f64 {
    let mut res = 0.0;
    for block in a.chunks(8192) {
        res += pairwise_sum(block);
    }
    res
}

fn pairwise_sum(a: &[f64]) -> f64 {
    let n = a.len();
    if n < 8 {
        let mut res = 0.0;
        for v in a {
            res += *v;
        }
        res
    } else if n <= 128 {
        let mut r = [a[0], a[1], a[2], a[3], a[4], a[5], a[6], a[7]];
        let mut i = 8;
        while i < n - n % 8 {
            for k in 0..8 {
                r[k] += a[i + k];
            }
            i += 8;
        }
        let mut res = ((r[0] + r[1]) + (r[2] + r[3])) + ((r[4] + r[5]) + (r[6] + r[7]));
        for v in &a[i..] {
            res += *v;
        }
        res
    } else {
        let mut m = n / 2;
        m -= m % 8;
        pairwise_sum(&a[..m]) + pairwise_sum(&a[m..])
    }
}

/// Python `round()` on a float: half to even.
pub fn round_half_even(x: f64) -> i64 {
    x.round_ties_even() as i64
}

// --- input -----------------------------------------------------------------------------------------------------

/// Leaf mask from the alpha channel of an image (alpha > 127). Images without alpha are refused.
pub fn load_alpha_mask(path: &Path) -> Result<Mask, String> {
    let img = image::open(path).map_err(|e| format!("{}: {e}", path.display()))?;
    if !img.color().has_alpha() {
        return Err(format!("{}: no alpha channel; the leaf must be the opaque part of the image", path.display()));
    }
    let rgba = img.to_rgba8();
    let (w, h) = (rgba.width() as usize, rgba.height() as usize);
    Ok(Grid::from_vec(h, w, rgba.pixels().map(|p| p.0[3] > 127).collect()))
}

/// Leaf mask from a binary image with a dark leaf on a light background: grey value < 128 (the `image` crate's
/// luma of the colour channels). Pixels with alpha <= 127 never count as leaf. Meant for black-and-white masks;
/// on a photograph the result is whatever falls below the threshold.
pub fn load_dark_mask(path: &Path) -> Result<Mask, String> {
    let img = image::open(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let luma = img.to_luma8();
    let alpha: Option<Vec<bool>> =
        img.color().has_alpha().then(|| img.to_rgba8().pixels().map(|p| p.0[3] > 127).collect());
    let (w, h) = (luma.width() as usize, luma.height() as usize);
    let data = luma.pixels().enumerate().map(|(i, p)| p.0[0] < 128 && alpha.as_ref().is_none_or(|a| a[i])).collect();
    Ok(Grid::from_vec(h, w, data))
}

/// Which pixels of an input image are leaf. There is no default: the user always says which rule applies.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MaskSource {
    /// Opaque pixels (alpha > 127) of an image with a transparent background, e.g. a cut-out.
    Alpha,
    /// Dark pixels (grey value < 128) of a binary image with a dark leaf on a light background.
    Dark,
}

impl MaskSource {
    pub fn as_str(&self) -> &'static str {
        match self {
            MaskSource::Alpha => "alpha",
            MaskSource::Dark => "dark",
        }
    }
}

/// Leaf mask of an image file by the chosen rule.
pub fn load_mask(path: &Path, source: MaskSource) -> Result<Mask, String> {
    match source {
        MaskSource::Alpha => load_alpha_mask(path),
        MaskSource::Dark => load_dark_mask(path),
    }
}

/// An already normalised mask (1-bit PNG; any non-zero pixel = leaf).
pub fn load_normalised_mask(path: &Path) -> Result<Mask, String> {
    let g = image::open(path).map_err(|e| format!("{}: {e}", path.display()))?.to_luma8();
    let (w, h) = (g.width() as usize, g.height() as usize);
    Ok(Grid::from_vec(h, w, g.into_raw().iter().map(|v| *v > 0).collect()))
}

// --- pipeline --------------------------------------------------------------------------------------------------

/// Largest component, crop, isotropic Pillow-bilinear scale to sqrt(A) = SQRT_A, threshold, pad.
/// Returns the normalised mask and the native sqrt(leaf area) in px; None for an empty mask.
pub fn normalise(raw: &Mask) -> Option<(Mask, f64)> {
    let m = mask::largest_component(raw);
    let (r0, r1, c0, c1) = mask::bbox(&m)?;
    let m = mask::crop(&m, r0, r1, c0, c1);
    let native = (m.data.iter().filter(|v| **v).count() as f64).sqrt();
    let f = SQRT_A / native;
    let img = Grid::from_vec(m.rows, m.cols, m.data.iter().map(|v| if *v { 255u8 } else { 0 }).collect());
    let out_cols = round_half_even(m.cols as f64 * f) as usize;
    let out_rows = round_half_even(m.rows as f64 * f) as usize;
    let r = resize::resize_bilinear(&img, out_cols, out_rows);
    let scaled = Grid::from_vec(r.rows, r.cols, r.data.iter().map(|v| *v > 127).collect());
    Some((mask::pad(&scaled, PAD_NORM), native))
}

/// Resolution flag of a value (D44).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Flag {
    Ok,
    BelowRecommended,
    NotResolved,
    /// Normalised input: the native size is not known.
    Unknown,
}

impl Flag {
    pub fn as_str(&self) -> &'static str {
        match self {
            Flag::Ok => "ok",
            Flag::BelowRecommended => "below_recommended",
            Flag::NotResolved => "not_resolved",
            Flag::Unknown => "unknown",
        }
    }
}

/// Flags for EA and MC from the native sqrt(A) in px.
pub fn flags(native: Option<f64>) -> (Flag, Flag) {
    match native {
        None => (Flag::Unknown, Flag::Unknown),
        Some(n) if n < FLOOR_MIN => (Flag::NotResolved, Flag::NotResolved),
        Some(n) if n < FLOOR_EA => (Flag::BelowRecommended, Flag::Ok),
        Some(_) => (Flag::Ok, Flag::Ok),
    }
}

/// All v2 numbers of one leaf.
#[derive(Clone, Debug)]
pub struct Measurement {
    /// Edge amount (NaN if not resolved).
    pub ea: f64,
    /// Macro-shape complexity (NaN if not resolved).
    pub mc: f64,
    /// Major / minor axis ratio of the blade.
    pub elongation: f64,
    /// Petiole length / sqrt(A); 0 = no petiole found.
    pub petiole_len: f64,
    /// sqrt(leaf area) in px before normalisation (NaN if unknown).
    pub sqrt_a_native: f64,
    pub ea_flag: Flag,
    pub mc_flag: Flag,
}

/// Intermediate stages of one measurement, for drawing and checks.
#[derive(Clone, Debug)]
pub struct Trace {
    /// Normalised mask.
    pub norm: Mask,
    /// Mask after petiole removal (L').
    pub lp: Mask,
    /// Outline c and blade line BL, in the frame of `lp` padded by PAD_CONTOUR px.
    pub outline: Vec<Point>,
    pub blade_line: Vec<Point>,
    /// Margin profile h(s) along the blade line, px (EA is the extra length of this profile).
    pub margin_profile: Vec<f64>,
    /// Error curve e(k) / sqrt(A) of the whitened blade line, k = 1, 2, ... (MC sums its log excess over TAU).
    /// Empty if MC was not computed.
    pub error_curve: Vec<f64>,
}

/// All v2 numbers for a normalised mask; `native` = sqrt(A) in px before normalisation (for the flags).
pub fn measure_normalised_traced(m: &Mask, native: Option<f64>) -> (Measurement, Trace) {
    let (lp, petiole_len) = morph::remove_petiole(m);
    let (c, step) = contour::resample(&contour::outline(&lp), 1.0);
    let sig = R_STAR * SQRT_A / step;
    let rows: Vec<f64> = c.iter().map(|p| p[0]).collect();
    let cols: Vec<f64> = c.iter().map(|p| p[1]).collect();
    let (br, bc) = (contour::gaussian_filter1d_wrap(&rows, sig), contour::gaussian_filter1d_wrap(&cols, sig));
    let bl: Vec<Point> = br.iter().zip(&bc).map(|(r, c)| [*r, *c]).collect();
    let pad = PAD_CONTOUR as f64;
    let bl_frame: Vec<Point> = bl.iter().map(|p| [p[0] - pad, p[1] - pad]).collect();
    let (white, elongation) = metrics::whiten(&bl_frame, lp.rows, lp.cols);
    let (ea_flag, mc_flag) = flags(native);
    let h = metrics::margin_profile(&c, &bl);
    let ea = if ea_flag != Flag::NotResolved { metrics::edge_amount_from_profile(&h, &bl) } else { f64::NAN };
    let e = if mc_flag != Flag::NotResolved { metrics::error_curve(&white) } else { Vec::new() };
    let mc = if mc_flag != Flag::NotResolved { metrics::macro_complexity_from_curve(&e) } else { f64::NAN };
    let meas = Measurement { ea, mc, elongation, petiole_len, sqrt_a_native: native.unwrap_or(f64::NAN), ea_flag, mc_flag };
    (meas, Trace { norm: m.clone(), lp, outline: c, blade_line: bl, margin_profile: h, error_curve: e })
}

/// All v2 numbers for a raw leaf mask (any size). None for an empty mask.
pub fn measure_traced(raw: &Mask) -> Option<(Measurement, Trace)> {
    let (m, native) = normalise(raw)?;
    Some(measure_normalised_traced(&m, Some(native)))
}

/// All v2 numbers for a raw leaf mask (any size). None for an empty mask.
pub fn measure(raw: &Mask) -> Option<Measurement> {
    measure_traced(raw).map(|(m, _)| m)
}
