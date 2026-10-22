// src/batch.rs - shared by the CLI and the GUI: finding images, measuring files, the results table, the port check

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::Instant;

use rayon::prelude::*;

use crate::figure::{error_curve_csv, leaf_svg, margin_profile_csv};
use crate::v2::{
    leaf_regions, load_alpha_mask, load_normalised_mask, mask_from_image, measure_normalised_traced, measure_traced,
    normalise, Flag, Mask, MaskSource, Measurement, Trace,
};

/// Version written into every results row.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Image file types read as input.
pub const IMAGE_EXTENSIONS: &[&str] = &["png", "jpg", "jpeg", "tif", "tiff", "bmp"];

/// Image files in a folder, sorted by path. Folders named `ShapeComplexityResults` (GUI export) are skipped.
pub fn image_files(dir: &Path, recursive: bool) -> Result<Vec<PathBuf>, String> {
    let mut out = Vec::new();
    collect(dir, recursive, &mut out)?;
    out.sort();
    Ok(out)
}

fn collect(dir: &Path, recursive: bool, out: &mut Vec<PathBuf>) -> Result<(), String> {
    let entries = fs::read_dir(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            if recursive && path.file_name().is_none_or(|n| n != "ShapeComplexityResults") {
                collect(&path, recursive, out)?;
            }
        } else if path
            .extension()
            .and_then(|e| e.to_str())
            .is_some_and(|e| IMAGE_EXTENSIONS.iter().any(|x| x.eq_ignore_ascii_case(e)))
        {
            out.push(path);
        }
    }
    Ok(())
}

/// Mask check (D115): above this share of leaf pixels outside the kept region, warn.
pub const DROPPED_WARN: f64 = 0.01;
/// Mask check (D115), `dark` input only: above this share of grey pixels (32..=223) among the opaque pixels that are
/// not white (<= 223), warn that the image is probably not a binary mask.
pub const GREY_WARN: f64 = 0.5;

/// Mask check of one input (D114 B, D115). Warnings only: no value depends on it.
#[derive(Clone, Debug, PartialEq)]
pub struct MaskCheck {
    /// Share of the leaf pixels outside the measured region(s) (dropped by the method).
    pub dropped_share: f64,
    /// `dark` input: share of grey pixels among the opaque pixels that are not white; None for `alpha` input.
    pub grey_share: Option<f64>,
}

impl MaskCheck {
    /// Plain-text warnings; empty if the mask looks fine.
    pub fn warnings(&self) -> Vec<String> {
        let mut w = Vec::new();
        if self.dropped_share > DROPPED_WARN {
            w.push(format!(
                "{:.1} % of the leaf pixels lie outside the measured region(s) and were not measured",
                100.0 * self.dropped_share
            ));
        }
        if let Some(g) = self.grey_share.filter(|g| *g > GREY_WARN) {
            w.push(format!(
                "{:.0} % of the non-white pixels are grey: the image is probably not a binary mask",
                100.0 * g
            ));
        }
        w
    }
}

/// Share of grey pixels (32..=223) among the opaque (alpha > 127) pixels that are not white (<= 223), on the same
/// grey values as the `dark` rule. 0 if every pixel is white.
pub fn grey_share(img: &image::DynamicImage) -> f64 {
    let luma = img.to_luma8();
    let alpha: Option<Vec<bool>> =
        img.color().has_alpha().then(|| img.to_rgba8().pixels().map(|p| p.0[3] > 127).collect());
    grey_share_of(luma.pixels().enumerate().filter(|(i, _)| alpha.as_ref().is_none_or(|a| a[*i])).map(|(_, p)| p.0[0]))
}

/// [`grey_share`] of a list of grey values (opaque pixels only).
pub fn grey_share_of(grey: impl Iterator<Item = u8>) -> f64 {
    let (mut grey_px, mut not_white) = (0usize, 0usize);
    for v in grey {
        if v <= 223 {
            not_white += 1;
            if v >= 32 {
                grey_px += 1;
            }
        }
    }
    if not_white == 0 {
        0.0
    } else {
        grey_px as f64 / not_white as f64
    }
}

/// One measured leaf of an image.
#[derive(Clone, Debug)]
pub struct Leaf {
    /// 1, 2, ... in raster order of each region's first pixel (always 1 for the largest-region run).
    pub index: usize,
    /// Mean pixel centre (x, y) of the region in input pixels, pixel-edge coordinates; NaN for the largest-region run.
    pub centroid: [f64; 2],
    /// Pixels of the measured region (whole leaf, petiole included).
    pub px: usize,
    /// The rule applied to the image (`Alpha` or `Dark`; `Auto` resolved, D117).
    pub source: MaskSource,
    pub measurement: Measurement,
    pub trace: Trace,
}

/// Measure the leaves of one image file, with the image's mask check: the largest region (default), or every leaf
/// region of D116 (`all_leaves`). Errors: unreadable file, no alpha channel (alpha mode), no leaf pixels.
pub fn measure_file_leaves(path: &Path, source: MaskSource, all_leaves: bool) -> Result<(Vec<Leaf>, MaskCheck), String> {
    let img = image::open(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let source = source.resolve(&img);
    let raw = mask_from_image(&img, source).map_err(|e| format!("{}: {e}", path.display()))?;
    let empty = || format!("{}: no leaf pixels ({} mask)", path.display(), source.as_str());
    let grey_share = (source == MaskSource::Dark).then(|| grey_share(&img));
    if !all_leaves {
        let (m, trace) = measure_traced(&raw).ok_or_else(empty)?;
        let p = trace.placement.as_ref().expect("raw input has a placement");
        let check = MaskCheck { dropped_share: p.dropped_share(), grey_share };
        let leaf = Leaf { index: 1, centroid: [f64::NAN; 2], px: p.kept_px, source, measurement: m, trace };
        return Ok((vec![leaf], check));
    }
    let total = raw.data.iter().filter(|v| **v).count();
    let mut leaves = Vec::new();
    for (i, region) in leaf_regions(&raw).into_iter().enumerate() {
        let (m, mut trace) = measure_traced(&region.mask).ok_or_else(empty)?;
        // placement in the frame of the whole input
        if let Some(p) = trace.placement.as_mut() {
            p.input_size = [raw.rows, raw.cols];
            p.crop[0] += region.offset[0];
            p.crop[1] += region.offset[1];
            p.leaf_px = total;
        }
        leaves.push(Leaf { index: i + 1, centroid: region.centroid, px: region.px, source, measurement: m, trace });
    }
    if leaves.is_empty() {
        return Err(empty());
    }
    let measured: usize = leaves.iter().map(|l| l.px).sum();
    let check = MaskCheck { dropped_share: 1.0 - measured as f64 / total as f64, grey_share };
    Ok((leaves, check))
}

/// Measure one image file (largest region), with its mask check. Use [`measure_file_leaves`] for the rule applied.
pub fn measure_file(path: &Path, source: MaskSource) -> Result<(Measurement, Trace, MaskCheck), String> {
    let (mut leaves, check) = measure_file_leaves(path, source, false)?;
    let leaf = leaves.remove(0);
    Ok((leaf.measurement, leaf.trace, check))
}

/// Flag other than ok or a mask-check warning: a leaf to look at again (hollow in the morphospace).
pub fn flagged(m: &Measurement, check: &MaskCheck) -> bool {
    m.ea_flag != Flag::Ok || m.mc_flag != Flag::Ok || !check.warnings().is_empty()
}

// --- per-leaf files (D118, D119) --------------------------------------------------------------------------------

/// Name of a leaf's files without extension (D119): `rel` (the image's path relative to its input folder, `/`
/// separators, or its file name) without the extension, plus `_leaf<n>` when every leaf of the image was measured.
pub fn leaf_stem(rel: &str, leaf: Option<usize>) -> String {
    let stem = match rel.rsplit_once('.') {
        Some((s, ext)) if !ext.contains('/') && !s.is_empty() && !s.ends_with('/') => s,
        _ => rel,
    };
    match leaf {
        Some(i) => format!("{stem}_leaf{i}"),
        None => stem.to_string(),
    }
}

/// Error message if two images would write files of the same name (`rel` as in [`leaf_stem`]), e.g. `a.png` and
/// `a.jpg` in one folder.
pub fn name_clash<'a>(rels: impl IntoIterator<Item = &'a str>) -> Option<String> {
    let mut seen = std::collections::HashMap::new();
    for rel in rels {
        let stem = leaf_stem(rel, None);
        if let Some(other) = seen.insert(stem.clone(), rel) {
            return Some(format!("{other} and {rel} would write files of the same name ({stem}); rename one"));
        }
    }
    None
}

/// Title of a leaf's figure: the `file` label, plus the leaf number when every leaf of the image was measured.
pub fn leaf_title(label: &str, leaf: Option<usize>) -> String {
    match leaf {
        Some(i) => format!("{label}, leaf {i}"),
        None => label.to_string(),
    }
}

/// Folders for the per-leaf files; None = not written.
#[derive(Clone, Debug, Default)]
pub struct LeafExport {
    /// Fig. 1-style SVG per leaf: `<stem>.svg`.
    pub figures: Option<PathBuf>,
    /// Curve tables per leaf: `<stem>_hs.csv` (s, h) and `<stem>_ek.csv` (k, e).
    pub curves: Option<PathBuf>,
}

impl LeafExport {
    pub fn is_empty(&self) -> bool {
        self.figures.is_none() && self.curves.is_none()
    }

    /// Write the files of one leaf; subfolders of `stem` are created.
    pub fn write(&self, stem: &str, title: &str, m: &Measurement, t: &Trace) -> Result<(), String> {
        if let Some(dir) = &self.figures {
            write_file(&dir.join(format!("{stem}.svg")), &leaf_svg(title, m, t))?;
        }
        if let Some(dir) = &self.curves {
            write_file(&dir.join(format!("{stem}_hs.csv")), &margin_profile_csv(t))?;
            write_file(&dir.join(format!("{stem}_ek.csv")), &error_curve_csv(t))?;
        }
        Ok(())
    }
}

/// Write a text file, creating its folder.
pub fn write_file(path: &Path, text: &str) -> Result<(), String> {
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    }
    fs::write(path, text).map_err(|e| format!("{}: {e}", path.display()))
}

/// Header of the results table.
pub const CSV_HEADER: &str = "file,EA,MC,elongation,petiole_len,sqrtA_native,EA_flag,MC_flag,mask_source,version";

/// Optional columns of the results table (D116); the default has none.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Columns {
    /// `leaf`, `leaf_x`, `leaf_y` after `file` (all leaves of an image measured).
    pub leaves: bool,
    /// `area_mm2` after `sqrtA_native`: pixels of the measured region / px_per_mm^2.
    pub px_per_mm: Option<f64>,
}

impl Columns {
    pub fn header(&self) -> String {
        let mut h: Vec<&str> = CSV_HEADER.split(',').collect();
        if self.px_per_mm.is_some() {
            h.insert(6, "area_mm2");
        }
        if self.leaves {
            h.splice(1..1, ["leaf", "leaf_x", "leaf_y"]);
        }
        h.join(",")
    }

    /// One row of the results table for a leaf of the image `file`.
    pub fn row(&self, file: &str, leaf: &Leaf) -> String {
        self.row_of(file, leaf.index, leaf.centroid, leaf.px, leaf.source, &leaf.measurement)
    }

    /// [`Columns::row`] from the parts of a leaf: number, centroid, pixels, rule applied, values.
    pub fn row_of(
        &self,
        file: &str,
        index: usize,
        centroid: [f64; 2],
        px: usize,
        source: MaskSource,
        m: &Measurement,
    ) -> String {
        let area = self.px_per_mm.map(|k| px as f64 / (k * k));
        table_row(file, m, source, self.leaves.then_some((index, centroid)), area)
    }
}

fn num(v: f64) -> String {
    if v.is_nan() {
        "NA".to_string()
    } else {
        v.to_string()
    }
}

/// One row of the results table (default columns); `source` = the rule applied to the image. Missing values are
/// written as `NA`; numbers in full precision.
pub fn csv_row(file: &str, m: &Measurement, source: MaskSource) -> String {
    table_row(file, m, source, None, None)
}

fn table_row(
    file: &str,
    m: &Measurement,
    source: MaskSource,
    leaf: Option<(usize, [f64; 2])>,
    area: Option<f64>,
) -> String {
    let mut f = vec![csv_field(file)];
    if let Some((i, c)) = leaf {
        f.extend([i.to_string(), num(c[0]), num(c[1])]);
    }
    f.extend([num(m.ea), num(m.mc), num(m.elongation), num(m.petiole_len), num(m.sqrt_a_native)]);
    if let Some(a) = area {
        f.push(num(a));
    }
    f.extend([m.ea_flag.as_str(), m.mc_flag.as_str(), source.as_str(), VERSION].map(String::from));
    f.join(",")
}

/// Quote a text field if it holds a comma, quote or line break (RFC 4180).
fn csv_field(s: &str) -> String {
    if s.contains([',', '"', '\n', '\r']) {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s.to_string()
    }
}

/// Name of a file in the results table: its path relative to `root` with `/` separators, or the full path.
pub fn file_label(path: &Path, root: &Path) -> String {
    match path.strip_prefix(root) {
        Ok(rel) if !rel.as_os_str().is_empty() => {
            rel.components().map(|c| c.as_os_str().to_string_lossy()).collect::<Vec<_>>().join("/")
        }
        _ => path.to_string_lossy().into_owned(),
    }
}

// --- port check -------------------------------------------------------------------------------------------------

fn save_mask(m: &Mask, p: &Path) -> Result<(), String> {
    let data: Vec<u8> = m.data.iter().map(|v| if *v { 255 } else { 0 }).collect();
    image::GrayImage::from_raw(m.cols as u32, m.rows as u32, data)
        .ok_or("mask size")?
        .save(p)
        .map_err(|e| format!("{}: {e}", p.display()))
}

fn port_check_one(key: &str, path: &str, mode: &str, mask_dir: &Path) -> Result<String, String> {
    let t0 = Instant::now();
    let (m, native) = match mode {
        "raw" => {
            let raw = load_alpha_mask(Path::new(path))?;
            let (m, native) = normalise(&raw).ok_or(format!("{key}: empty mask"))?;
            save_mask(&m, &mask_dir.join(format!("{key}__norm.png")))?;
            (m, Some(native))
        }
        "normalised" => (load_normalised_mask(Path::new(path))?, None),
        _ => return Err(format!("{key}: unknown mode {mode}")),
    };
    let (r, trace) = measure_normalised_traced(&m, native);
    let secs = t0.elapsed().as_secs_f64();
    save_mask(&trace.lp, &mask_dir.join(format!("{key}__lp.png")))?;
    Ok(format!(
        "{key},{},{},{},{},{},{},{},{secs}",
        r.ea, r.mc, r.elongation, r.petiole_len, r.sqrt_a_native, r.ea_flag.as_str(), r.mc_flag.as_str()
    ))
}

/// Run the pipeline on a manifest for the check against the Python reference (contract: PORTING.md §13 of the
/// analysis archive). `manifest`: key,path,mode (raw = alpha > 127, normalised = 1-bit cached mask).
/// Writes `out` (key,EA,MC,elongation,petiole_len,sqrtA_native,EA_flag,MC_flag,seconds; NaN as `NaN`) and, per key,
/// `<key>__norm.png` (raw only) and `<key>__lp.png` in `mask_dir`. Returns the number of failed rows.
pub fn port_check(manifest: &Path, out: &Path, mask_dir: &Path) -> Result<usize, String> {
    let text = fs::read_to_string(manifest).map_err(|e| format!("{}: {e}", manifest.display()))?;
    let mut lines = text.lines();
    let head: Vec<&str> = lines.next().ok_or("empty manifest")?.split(',').collect();
    let col = |n: &str| head.iter().position(|h| *h == n).ok_or(format!("manifest has no column {n}"));
    let (ik, ip, im) = (col("key")?, col("path")?, col("mode")?);
    let rows: Vec<Vec<&str>> = lines.filter(|l| !l.is_empty()).map(|l| l.split(',').collect()).collect();
    fs::create_dir_all(mask_dir).map_err(|e| format!("{}: {e}", mask_dir.display()))?;
    let res: Vec<Result<String, String>> =
        rows.par_iter().map(|r| port_check_one(r[ik], r[ip], r[im], mask_dir)).collect();
    let mut f = fs::File::create(out).map_err(|e| format!("{}: {e}", out.display()))?;
    writeln!(f, "key,EA,MC,elongation,petiole_len,sqrtA_native,EA_flag,MC_flag,seconds").map_err(|e| e.to_string())?;
    let mut failed = 0;
    for r in res {
        match r {
            Ok(line) => writeln!(f, "{line}").map_err(|e| e.to_string())?,
            Err(e) => {
                eprintln!("{e}");
                failed += 1;
            }
        }
    }
    Ok(failed)
}
