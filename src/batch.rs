// src/batch.rs - shared by the CLI and the GUI: finding images, measuring files, the results table, the port check

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::Instant;

use rayon::prelude::*;

use crate::v2::{
    load_alpha_mask, load_mask, load_normalised_mask, measure_normalised_traced, measure_traced, normalise, Mask,
    MaskSource, Measurement, Trace,
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

/// Measure one image file. Errors: unreadable file, no alpha channel (alpha mode), no leaf pixels.
pub fn measure_file(path: &Path, source: MaskSource) -> Result<(Measurement, Trace), String> {
    let raw = load_mask(path, source)?;
    measure_traced(&raw).ok_or_else(|| format!("{}: no leaf pixels ({} mask)", path.display(), source.as_str()))
}

/// Header of the results table.
pub const CSV_HEADER: &str = "file,EA,MC,elongation,petiole_len,sqrtA_native,EA_flag,MC_flag,version";

/// One row of the results table. Missing values are written as `NA`; numbers in full precision.
pub fn csv_row(file: &str, m: &Measurement) -> String {
    let num = |v: f64| if v.is_nan() { "NA".to_string() } else { v.to_string() };
    format!(
        "{},{},{},{},{},{},{},{},{VERSION}",
        csv_field(file),
        num(m.ea),
        num(m.mc),
        num(m.elongation),
        num(m.petiole_len),
        num(m.sqrt_a_native),
        m.ea_flag.as_str(),
        m.mc_flag.as_str()
    )
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
