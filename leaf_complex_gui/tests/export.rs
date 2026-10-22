// leaf_complex_gui/tests/export.rs - the GUI exports the same figure and curve files as the command line (D118)

use std::path::Path;

use leaf_complex_gui::analysis::AnalysisEngine;
use shapecomplexity::batch::measure_file_leaves;
use shapecomplexity::figure::{error_curve_csv, leaf_svg, margin_profile_csv};
use shapecomplexity::MaskSource;

#[test]
fn gui_trace_gives_the_command_line_files() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../tests/fixtures/public/leaves/oak__august__10L1_R1_u/input.png");
    if !path.exists() {
        eprintln!("skipped: {} not found", path.display());
        return;
    }
    let gui = AnalysisEngine::new().analyze_image(&path, MaskSource::Auto, false).unwrap().remove(0);
    let (mut leaves, _) = measure_file_leaves(&path, MaskSource::Auto, false).unwrap();
    let cli = leaves.remove(0);
    let t = gui.trace();
    assert_eq!(t.norm, cli.trace.norm);
    assert_eq!(t.lp, cli.trace.lp);
    assert_eq!(leaf_svg("x", &gui.measurement, &t), leaf_svg("x", &cli.measurement, &cli.trace));
    assert_eq!(margin_profile_csv(&t), margin_profile_csv(&cli.trace));
    assert_eq!(error_curve_csv(&t), error_curve_csv(&cli.trace));
}
