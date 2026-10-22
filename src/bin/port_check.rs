// src/bin/port_check.rs - runs the v2 pipeline on a manifest for the port check against the Python reference
//
// Usage: port_check <manifest.csv> <out.csv> <mask_dir>
//   manifest.csv: key,path,mode (mode raw = RGBA alpha > 127, normalised = 1-bit cached mask)
//   out.csv: key,EA,MC,elongation,petiole_len,sqrtA_native,EA_flag,MC_flag,seconds
//   mask_dir: <key>__norm.png (raw input only) and <key>__lp.png
// Contract: PORTING.md §13 of the analysis archive (check_port.py).

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::Instant;

use rayon::prelude::*;
use shapecomplexity::v2::{load_alpha_mask, load_normalised_mask, measure_normalised_traced, normalise, Mask};

fn save_mask(m: &Mask, p: &Path) -> Result<(), String> {
    let data: Vec<u8> = m.data.iter().map(|v| if *v { 255 } else { 0 }).collect();
    image::GrayImage::from_raw(m.cols as u32, m.rows as u32, data)
        .ok_or("mask size")?
        .save(p)
        .map_err(|e| format!("{}: {e}", p.display()))
}

fn run_one(key: &str, path: &str, mode: &str, mask_dir: &Path) -> Result<String, String> {
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

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() != 4 {
        eprintln!("usage: port_check <manifest.csv> <out.csv> <mask_dir>");
        std::process::exit(2);
    }
    let text = fs::read_to_string(&args[1]).expect("manifest");
    let mut lines = text.lines();
    let head: Vec<&str> = lines.next().expect("empty manifest").split(',').collect();
    let col = |n: &str| head.iter().position(|h| *h == n).unwrap_or_else(|| panic!("manifest has no column {n}"));
    let (ik, ip, im) = (col("key"), col("path"), col("mode"));
    let rows: Vec<Vec<&str>> = lines.filter(|l| !l.is_empty()).map(|l| l.split(',').collect()).collect();
    let mask_dir = PathBuf::from(&args[3]);
    fs::create_dir_all(&mask_dir).expect("mask dir");
    let out: Vec<Result<String, String>> = rows.par_iter().map(|r| run_one(r[ik], r[ip], r[im], &mask_dir)).collect();
    let mut f = fs::File::create(&args[2]).expect("out.csv");
    writeln!(f, "key,EA,MC,elongation,petiole_len,sqrtA_native,EA_flag,MC_flag,seconds").unwrap();
    let mut failed = 0;
    for o in out {
        match o {
            Ok(line) => writeln!(f, "{line}").unwrap(),
            Err(e) => {
                eprintln!("{e}");
                failed += 1;
            }
        }
    }
    if failed > 0 {
        eprintln!("{failed} mask(s) failed");
    }
}
