// tests/synthetic.rs - behaviour of the v2 method on synthetic shapes (as `test_v2.py unit` of the analysis archive)
// and of the input paths shared by the CLI and the GUI.
//
// Shapes as in `synth_shapes.py` (WP1): polar outlines r(t) = R0 (1 + a_l cos(n_l t)) + a_t tri(n_t t), ellipses,
// and teeth of fixed arc-length period along the outward normal; rendered with 4 x 4 supersampling on a square
// canvas with a 10 % margin. The rendering need not match Python's pixel for pixel: these tests check behaviour.

use std::f64::consts::PI;
use std::path::PathBuf;

use shapecomplexity::batch::{csv_row, measure_file, CSV_HEADER};
use shapecomplexity::v2::fill::polygon_fill;
use shapecomplexity::v2::{flags, measure, Flag, Grid, Mask, MaskSource, Measurement};

const NPTS: usize = 8000;
const MARGIN: f64 = 0.10;
const SS: usize = 4;

type Xy = Vec<[f64; 2]>;

fn tri(x: f64) -> f64 {
    2.0 / PI * x.sin().asin()
}

fn angles() -> impl Iterator<Item = f64> {
    (0..NPTS).map(|i| 2.0 * PI * i as f64 / NPTS as f64)
}

/// Polar outline with lobes (n_l, depth) and teeth (n_t, depth); depths as fractions of sqrt(area).
fn polar(n_l: f64, lobe_depth: f64, n_t: f64, tooth_depth: f64) -> Xy {
    let r0 = 1.0 / PI.sqrt();
    angles()
        .map(|t| {
            let r = r0 * (1.0 + 0.5 * lobe_depth / r0 * (n_l * t).cos()) + 0.5 * tooth_depth * tri(n_t * t);
            [r * t.cos(), r * t.sin()]
        })
        .collect()
}

fn ellipse(aspect: f64) -> Xy {
    angles().map(|t| [aspect * t.cos(), t.sin()]).collect()
}

/// Teeth of arc-length period (circle perimeter / 40) along the outward normal of a counter-clockwise outline.
fn arc_teeth(xy: &Xy, tooth_depth: f64) -> Xy {
    if tooth_depth == 0.0 {
        return xy.clone();
    }
    let n = xy.len();
    let seg: Vec<f64> =
        (0..n).map(|i| (xy[(i + 1) % n][0] - xy[i][0]).hypot(xy[(i + 1) % n][1] - xy[i][1])).collect();
    let perimeter: f64 = seg.iter().sum();
    let teeth = (perimeter / (2.0 * PI.sqrt() / 40.0)).round().max(1.0);
    let mut s = 0.0;
    (0..n)
        .map(|i| {
            let d = [xy[(i + 1) % n][0] - xy[(i + n - 1) % n][0], xy[(i + 1) % n][1] - xy[(i + n - 1) % n][1]];
            let len = d[0].hypot(d[1]);
            let off = 0.5 * tooth_depth * tri(2.0 * PI * teeth * s / perimeter);
            s += seg[i];
            [xy[i][0] + d[1] / len * off, xy[i][1] - d[0] / len * off]
        })
        .collect()
}

fn area(xy: &Xy) -> f64 {
    let n = xy.len();
    0.5 * (0..n).map(|i| xy[i][0] * xy[(i + 1) % n][1] - xy[(i + 1) % n][0] * xy[i][1]).sum::<f64>().abs()
}

/// Binary mask of the outline at sqrt(area) = `sqrt_a` px.
fn render(xy: &Xy, sqrt_a: f64) -> Mask {
    let k = sqrt_a / area(xy).sqrt();
    let p: Xy = xy.iter().map(|q| [q[0] * k, q[1] * k]).collect();
    let lo = [p.iter().map(|q| q[0]).fold(f64::INFINITY, f64::min), p.iter().map(|q| q[1]).fold(f64::INFINITY, f64::min)];
    let p: Xy = p.iter().map(|q| [q[0] - lo[0], q[1] - lo[1]]).collect();
    let hi = [p.iter().map(|q| q[0]).fold(0.0, f64::max), p.iter().map(|q| q[1]).fold(0.0, f64::max)];
    let side = (hi[0].max(hi[1]) * (1.0 + 2.0 * MARGIN)).ceil() as usize;
    let off = [(side as f64 - hi[0]) / 2.0, (side as f64 - hi[1]) / 2.0];
    let xs: Vec<f64> = p.iter().map(|q| (q[0] + off[0]) * SS as f64).collect();
    let ys: Vec<f64> = p.iter().map(|q| (q[1] + off[1]) * SS as f64).collect();
    let big = polygon_fill(side * SS, side * SS, &xs, &ys);
    let mut m = Grid::new(side, side);
    for r in 0..side {
        for c in 0..side {
            let n = (0..SS * SS).filter(|i| big.get(r * SS + i / SS, c * SS + i % SS)).count();
            m.set(r, c, 2 * n > SS * SS);
        }
    }
    m
}

fn run(xy: &Xy, sqrt_a: f64) -> Measurement {
    measure(&render(xy, sqrt_a)).expect("leaf pixels")
}

#[test]
fn circle_has_no_edge_and_no_macro_shape() {
    let m = run(&polar(0.0, 0.0, 0.0, 0.0), 1000.0);
    println!("circle EA {} MC {} elongation {} petiole {}", m.ea, m.mc, m.elongation, m.petiole_len);
    assert!(m.ea < 0.01);
    assert_eq!(m.mc, 0.0);
    assert!((m.elongation - 1.0).abs() < 0.01);
    assert_eq!(m.petiole_len, 0.0);
}

#[test]
fn ellipses_have_mc_zero_and_their_aspect_ratio() {
    for a in [1.5, 2.0, 3.0, 4.0] {
        let m = run(&ellipse(a), 1000.0);
        println!("ellipse {a}: MC {} elongation {}", m.mc, m.elongation);
        assert_eq!(m.mc, 0.0, "ellipse {a}");
        assert!((m.elongation / a - 1.0).abs() < 0.02, "ellipse {a}");
    }
}

#[test]
fn teeth_raise_ea_and_lobes_raise_mc() {
    let circle = polar(0.0, 0.0, 0.0, 0.0);
    let ea: Vec<f64> = [0.0, 0.01, 0.02, 0.04].iter().map(|d| run(&arc_teeth(&circle, *d), 1000.0).ea).collect();
    let mc: Vec<f64> = [3.0, 5.0, 7.0].iter().map(|n| run(&polar(*n, 0.2, 0.0, 0.0), 1000.0).mc).collect();
    println!("teeth EA {ea:?}; lobes MC {mc:?}");
    assert!(ea.windows(2).all(|w| w[1] > w[0]), "EA must rise with tooth depth: {ea:?}");
    assert!(mc.windows(2).all(|w| w[1] > w[0]), "MC must rise with lobe number: {mc:?}");
}

#[test]
fn resolution_flags() {
    for (size, want) in [(200.0, Flag::NotResolved), (300.0, Flag::BelowRecommended), (600.0, Flag::Ok)] {
        let m = run(&polar(5.0, 0.2, 0.0, 0.0), size);
        println!("{size} px: {:?} {:?} sqrtA {}", m.ea_flag, m.mc_flag, m.sqrt_a_native);
        assert_eq!(m.ea_flag, want);
        assert_eq!(m.ea.is_nan(), want == Flag::NotResolved);
        assert_eq!(m.mc.is_nan(), want == Flag::NotResolved);
    }
    assert_eq!(flags(None), (Flag::Unknown, Flag::Unknown));
    assert_eq!(flags(Some(499.9)), (Flag::BelowRecommended, Flag::Ok));
    assert_eq!(flags(Some(500.0)), (Flag::Ok, Flag::Ok));
}

// --- input paths of the CLI and GUI --------------------------------------------------------------------------------

fn temp_png(name: &str) -> PathBuf {
    std::env::temp_dir().join(format!("shapecomplexity_test_{}_{name}.png", std::process::id()))
}

#[test]
fn alpha_and_dark_inputs_give_the_same_result() {
    let m = render(&polar(5.0, 0.2, 0.0, 0.0), 300.0);
    let (w, h) = (m.cols as u32, m.rows as u32);
    // cut-out: black leaf, transparent background
    let rgba = image::RgbaImage::from_fn(w, h, |x, y| {
        image::Rgba(if m.get(y as usize, x as usize) { [0, 0, 0, 255] } else { [255, 255, 255, 0] })
    });
    // binary: black leaf on white, no alpha
    let grey = image::GrayImage::from_fn(w, h, |x, y| image::Luma([if m.get(y as usize, x as usize) { 0 } else { 255 }]));
    let (pa, pd) = (temp_png("alpha"), temp_png("dark"));
    rgba.save(&pa).unwrap();
    grey.save(&pd).unwrap();
    let (a, _) = measure_file(&pa, MaskSource::Alpha).unwrap();
    let (d, _) = measure_file(&pd, MaskSource::Dark).unwrap();
    // the dark rule also works on the cut-out (transparent pixels never count as leaf)
    let (d2, _) = measure_file(&pa, MaskSource::Dark).unwrap();
    // an image without alpha is refused in alpha mode
    let refused = measure_file(&pd, MaskSource::Alpha);
    let _ = std::fs::remove_file(&pa);
    let _ = std::fs::remove_file(&pd);
    for x in [&d, &d2] {
        assert_eq!(a.ea.to_bits(), x.ea.to_bits());
        assert_eq!(a.mc.to_bits(), x.mc.to_bits());
        assert_eq!(a.elongation.to_bits(), x.elongation.to_bits());
        assert_eq!(a.sqrt_a_native.to_bits(), x.sqrt_a_native.to_bits());
    }
    assert!(refused.is_err());
}

#[test]
fn results_row() {
    let m = run(&polar(5.0, 0.2, 0.0, 0.0), 200.0);
    let row = csv_row("a,b/leaf.png", &m);
    let header: Vec<&str> = CSV_HEADER.split(',').collect();
    assert_eq!(header, ["file", "EA", "MC", "elongation", "petiole_len", "sqrtA_native", "EA_flag", "MC_flag", "version"]);
    assert!(row.starts_with("\"a,b/leaf.png\",NA,NA,"), "{row}");
    assert!(row.ends_with(&format!(",not_resolved,not_resolved,{}", env!("CARGO_PKG_VERSION"))), "{row}");
}
