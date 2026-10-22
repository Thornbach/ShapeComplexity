// src/main.rs - ShapeComplexity command line: `measure` (EA, MC per leaf image) and `port-check`

use std::fs;
use std::io::Write;
use std::path::PathBuf;
use std::process::ExitCode;
use std::time::Instant;

use clap::{Parser, Subcommand, ValueEnum};
use rayon::prelude::*;

use shapecomplexity::batch::{csv_row, file_label, image_files, measure_file, port_check, CSV_HEADER};
use shapecomplexity::v2::MaskSource;

#[derive(Parser, Debug)]
#[command(author, version, about = "ShapeComplexity: edge amount (EA) and macro-shape complexity (MC) of leaf outlines")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand, Debug)]
enum Command {
    /// Measure EA, MC, blade elongation and petiole length for leaf images; one CSV row per image.
    Measure {
        /// Image files or folders (png, jpg, jpeg, tif, tiff, bmp).
        #[arg(required = true)]
        inputs: Vec<PathBuf>,
        /// Which pixels are leaf: `alpha` = opaque pixels of an image with a transparent background;
        /// `dark` = dark pixels of a binary image (dark leaf on a light background). Required: there is no default.
        #[arg(long, value_enum)]
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
    Alpha,
    Dark,
}

impl From<MaskSourceArg> for MaskSource {
    fn from(a: MaskSourceArg) -> Self {
        match a {
            MaskSourceArg::Alpha => MaskSource::Alpha,
            MaskSourceArg::Dark => MaskSource::Dark,
        }
    }
}

fn run_measure(
    inputs: &[PathBuf],
    source: MaskSource,
    output: Option<&PathBuf>,
    recursive: bool,
) -> Result<usize, String> {
    // (file path, label in the table)
    let mut files: Vec<(PathBuf, String)> = Vec::new();
    for input in inputs {
        if input.is_dir() {
            for f in image_files(input, recursive)? {
                let label = file_label(&f, input);
                files.push((f, label));
            }
        } else if input.is_file() {
            files.push((input.clone(), input.to_string_lossy().into_owned()));
        } else {
            return Err(format!("{}: no such file or folder", input.display()));
        }
    }
    if files.is_empty() {
        return Err("no image files found".into());
    }
    eprintln!("Measuring {} image(s), mask source: {}", files.len(), source.as_str());
    let t0 = Instant::now();
    let rows: Vec<Result<String, String>> = files
        .par_iter()
        .map(|(path, label)| measure_file(path, source).map(|(m, _)| csv_row(label, &m)))
        .collect();
    let mut out: Box<dyn Write> = match output {
        Some(p) => Box::new(fs::File::create(p).map_err(|e| format!("{}: {e}", p.display()))?),
        None => Box::new(std::io::stdout().lock()),
    };
    writeln!(out, "{CSV_HEADER}").map_err(|e| e.to_string())?;
    let mut failed = 0;
    for r in rows {
        match r {
            Ok(line) => writeln!(out, "{line}").map_err(|e| e.to_string())?,
            Err(e) => {
                eprintln!("error: {e}");
                failed += 1;
            }
        }
    }
    eprintln!("Done: {} measured, {failed} failed, {:.1} s", files.len() - failed, t0.elapsed().as_secs_f64());
    Ok(failed)
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    let result = match cli.command {
        Command::Measure { inputs, mask_source, output, recursive, threads } => {
            if let Some(n) = threads {
                rayon::ThreadPoolBuilder::new().num_threads(n).build_global().expect("thread pool");
            }
            run_measure(&inputs, mask_source.into(), output.as_ref(), recursive)
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
