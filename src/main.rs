// src/main.rs - ShapeComplexity command line: `measure` (EA, MC per leaf image; figures, curves, morphospace) and
// `port-check`

use std::fs;
use std::io::Write;
use std::path::PathBuf;
use std::process::ExitCode;
use std::time::Instant;

use clap::{Parser, Subcommand, ValueEnum};
use rayon::prelude::*;

use shapecomplexity::batch::{
    file_label, flagged, image_files, leaf_stem, leaf_title, measure_file_leaves, name_clash, port_check, write_file,
    Columns,
    LeafExport,
};
use shapecomplexity::figure::{group_of, morphospace_svg, MorphPoint};
use shapecomplexity::v2::MaskSource;

#[derive(Parser, Debug)]
#[command(author, version, about = "ShapeComplexity: edge amount (EA) and macro-shape complexity (MC) of leaf outlines")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand, Debug)]
enum Command {
    /// Measure EA, MC, blade elongation and petiole length for leaf images; one CSV row per leaf. Optionally writes a
    /// Fig. 1-style picture and the curves of each leaf, and the EA-MC morphospace.
    Measure {
        /// Image files or folders (png, jpg, jpeg, tif, tiff, bmp).
        #[arg(required = true)]
        inputs: Vec<PathBuf>,
        /// Which pixels are leaf: `auto` = decide per image (alpha if it has transparent pixels, else dark);
        /// `alpha` = opaque pixels of an image with a transparent background; `dark` = dark pixels of a binary image
        /// (dark leaf on a light background). The rule applied is written to the column mask_source.
        #[arg(long, value_enum, default_value = "auto")]
        mask_source: MaskSourceArg,
        /// Output CSV file (default: standard output).
        #[arg(short, long)]
        output: Option<PathBuf>,
        /// Also search subfolders of input folders.
        #[arg(short, long)]
        recursive: bool,
        /// Number of threads (default: all cores).
        #[arg(short = 'j', long)]
        threads: Option<usize>,
        /// Measure every region with sqrt(area) >= 250 px as a leaf (one row each; columns leaf, leaf_x, leaf_y)
        /// instead of the largest region only.
        #[arg(long)]
        all_leaves: bool,
        /// Image scale in pixels per mm; adds the column area_mm2 (whole leaf, petiole included).
        #[arg(long, value_parser = positive)]
        px_per_mm: Option<f64>,
        /// Folder for one Fig. 1-style picture (SVG) per leaf: <file>[_leaf<n>].svg, keeping the input's subfolders.
        #[arg(long)]
        figures: Option<PathBuf>,
        /// Folder for the curves of each leaf: <file>[_leaf<n>]_hs.csv (margin profile s, h) and _ek.csv (fit error
        /// k, e), all in units of sqrt(A); keeping the input's subfolders.
        #[arg(long)]
        curves: Option<PathBuf>,
        /// EA-MC scatter (SVG) of all measured leaves, coloured by subfolder (one colour each if there are at most
        /// three).
        #[arg(long)]
        morphospace: Option<PathBuf>,
        /// Subfolder shown in colour in the morphospace when there are more than three (the others grey).
        #[arg(long, requires = "morphospace")]
        highlight: Option<String>,
    },
    /// Run the pipeline on a manifest for the check against the Python reference implementation.
    PortCheck {
        /// CSV with columns key,path,mode (mode `raw` or `normalised`).
        manifest: PathBuf,
        /// Output CSV.
        out: PathBuf,
        /// Folder for the normalised and petiole-free masks.
        mask_dir: PathBuf,
    },
}

#[derive(Clone, Copy, Debug, ValueEnum)]
enum MaskSourceArg {
    Auto,
    Alpha,
    Dark,
}

impl From<MaskSourceArg> for MaskSource {
    fn from(a: MaskSourceArg) -> Self {
        match a {
            MaskSourceArg::Auto => MaskSource::Auto,
            MaskSourceArg::Alpha => MaskSource::Alpha,
            MaskSourceArg::Dark => MaskSource::Dark,
        }
    }
}

fn positive(s: &str) -> Result<f64, String> {
    match s.parse::<f64>() {
        Ok(v) if v > 0.0 && v.is_finite() => Ok(v),
        _ => Err(format!("{s} is not a positive number")),
    }
}

/// Results-table rows and morphospace points of one image.
type ImageRows = (Vec<String>, Vec<MorphPoint>);

/// What `measure` writes besides the results table.
struct Outputs {
    table: Option<PathBuf>,
    leaf_files: LeafExport,
    morphospace: Option<PathBuf>,
    highlight: Option<String>,
}

fn run_measure(
    inputs: &[PathBuf],
    source: MaskSource,
    recursive: bool,
    all_leaves: bool,
    cols: Columns,
    out: &Outputs,
) -> Result<usize, String> {
    // (file path, label in the table, name of its per-leaf files: path relative to the input folder or file name)
    let mut files: Vec<(PathBuf, String, String)> = Vec::new();
    for input in inputs {
        if input.is_dir() {
            for f in image_files(input, recursive)? {
                let label = file_label(&f, input);
                files.push((f, label.clone(), label));
            }
        } else if input.is_file() {
            let name = input.file_name().map_or(String::new(), |n| n.to_string_lossy().into_owned());
            files.push((input.clone(), input.to_string_lossy().into_owned(), name));
        } else {
            return Err(format!("{}: no such file or folder", input.display()));
        }
    }
    if files.is_empty() {
        return Err("no image files found".into());
    }
    if !out.leaf_files.is_empty() {
        if let Some(e) = name_clash(files.iter().map(|(_, _, rel)| rel.as_str())) {
            return Err(e);
        }
    }
    eprintln!("Measuring {} image(s), mask source: {}", files.len(), source.as_str());
    let t0 = Instant::now();
    let rows: Vec<Result<ImageRows, String>> = files
        .par_iter()
        .map(|(path, label, rel)| {
            let (leaves, check) = measure_file_leaves(path, source, all_leaves)?;
            for w in check.warnings() {
                eprintln!("warning: {label}: {w}");
            }
            for leaf in &leaves {
                let n = all_leaves.then_some(leaf.index);
                out.leaf_files.write(&leaf_stem(rel, n), &leaf_title(label, n), &leaf.measurement, &leaf.trace)?;
            }
            let points = leaves
                .iter()
                .map(|leaf| MorphPoint {
                    ea: leaf.measurement.ea,
                    mc: leaf.measurement.mc,
                    group: group_of(label).to_string(),
                    flagged: flagged(&leaf.measurement, &check),
                })
                .collect();
            Ok((leaves.iter().map(|leaf| cols.row(label, leaf)).collect(), points))
        })
        .collect();
    let mut table: Box<dyn Write> = match &out.table {
        Some(p) => Box::new(fs::File::create(p).map_err(|e| format!("{}: {e}", p.display()))?),
        None => Box::new(std::io::stdout().lock()),
    };
    writeln!(table, "{}", cols.header()).map_err(|e| e.to_string())?;
    let mut failed = 0;
    let mut points = Vec::new();
    for r in rows {
        match r {
            Ok((lines, p)) => {
                for line in lines {
                    writeln!(table, "{line}").map_err(|e| e.to_string())?;
                }
                points.extend(p);
            }
            Err(e) => {
                eprintln!("error: {e}");
                failed += 1;
            }
        }
    }
    if let Some(path) = &out.morphospace {
        let hl = out.highlight.as_deref();
        if let Some(h) = hl.filter(|h| !points.iter().any(|p| p.group == *h)) {
            eprintln!("warning: --highlight: no measured leaf in a subfolder named {h}");
        }
        write_file(path, &morphospace_svg(&points, hl))?;
    }
    eprintln!("Done: {} image(s) measured, {failed} failed, {:.1} s", files.len() - failed, t0.elapsed().as_secs_f64());
    Ok(failed)
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    let result = match cli.command {
        Command::Measure {
            inputs,
            mask_source,
            output,
            recursive,
            threads,
            all_leaves,
            px_per_mm,
            figures,
            curves,
            morphospace,
            highlight,
        } => {
            if let Some(n) = threads {
                rayon::ThreadPoolBuilder::new().num_threads(n).build_global().expect("thread pool");
            }
            let cols = Columns { leaves: all_leaves, px_per_mm };
            let out = Outputs { table: output, leaf_files: LeafExport { figures, curves }, morphospace, highlight };
            run_measure(&inputs, mask_source.into(), recursive, all_leaves, cols, &out)
        }
        Command::PortCheck { manifest, out, mask_dir } => port_check(&manifest, &out, &mask_dir),
    };
    match result {
        Ok(0) => ExitCode::SUCCESS,
        Ok(n) => {
            eprintln!("{n} file(s) failed");
            ExitCode::FAILURE
        }
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::from(2)
        }
    }
}
