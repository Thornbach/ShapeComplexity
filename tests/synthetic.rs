// tests/synthetic.rs - behaviour of the v2 method on synthetic shapes (as `test_v2.py unit` of the analysis archive)
// and of the input paths shared by the CLI and the GUI.
//
// Shapes as in `synth_shapes.py` (WP1): polar outlines r(t) = R0 (1 + a_l cos(n_l t)) + a_t tri(n_t t), ellipses,
// and teeth of fixed arc-length period along the outward normal; rendered with 4 x 4 supersampling on a square
// canvas with a 10 % margin. The rendering need not match Python's pixel for pixel: these tests check behaviour.

use std::f64::consts::PI;
use std::path::PathBuf;

use shapecomplexity::batch::{
    csv_row, grey_share_of, leaf_stem, leaf_title, measure_file, measure_file_leaves, name_clash, Columns, MaskCheck,
    CSV_HEADER,
};
use shapecomplexity::figure::{arc_length, error_curve_csv, leaf_svg, margin_profile_csv, morphospace_svg, MorphPoint};
use shapecomplexity::v2::fill::polygon_fill;
use shapecomplexity::v2::metrics::efd_reconstruct;
use shapecomplexity::v2::{flags, measure, measure_traced, Flag, Grid, Mask, MaskSource, Measurement, PAD_CONTOUR, SQRT_A};

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
    let (a, _, _) = measure_file(&pa, MaskSource::Alpha).unwrap();
    let (d, _, _) = measure_file(&pd, MaskSource::Dark).unwrap();
    // the dark rule also works on the cut-out (transparent pixels never count as leaf)
    let (d2, _, _) = measure_file(&pa, MaskSource::Dark).unwrap();
    // an image without alpha is refused in alpha mode
    let refused = measure_file(&pd, MaskSource::Alpha);
    // auto (D117): the cut-out resolves to alpha, the binary image and a fully opaque RGBA mask to dark
    let opaque = image::RgbaImage::from_fn(w, h, |x, y| {
        image::Rgba(if m.get(y as usize, x as usize) { [0, 0, 0, 255] } else { [255, 255, 255, 255] })
    });
    let po = temp_png("opaque");
    opaque.save(&po).unwrap();
    let auto: Vec<_> = [&pa, &pd, &po]
        .iter()
        .map(|p| {
            let (mut l, _) = measure_file_leaves(p, MaskSource::Auto, false).unwrap();
            let l = l.remove(0);
            (l.source, l.measurement)
        })
        .collect();
    for p in [&pa, &pd, &po] {
        let _ = std::fs::remove_file(p);
    }
    assert_eq!(auto.iter().map(|a| a.0).collect::<Vec<_>>(), [MaskSource::Alpha, MaskSource::Dark, MaskSource::Dark]);
    let (a1, a2, a3) = (&auto[0].1, &auto[1].1, &auto[2].1);
    for x in [&d, &d2, a1, a2, a3] {
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
    let row = csv_row("a,b/leaf.png", &m, MaskSource::Dark);
    let header: Vec<&str> = CSV_HEADER.split(',').collect();
    assert_eq!(
        header,
        ["file", "EA", "MC", "elongation", "petiole_len", "sqrtA_native", "EA_flag", "MC_flag", "mask_source", "version"]
    );
    assert!(row.starts_with("\"a,b/leaf.png\",NA,NA,"), "{row}");
    assert!(row.ends_with(&format!(",not_resolved,not_resolved,dark,{}", env!("CARGO_PKG_VERSION"))), "{row}");
}

// --- display and mask-check helpers (D114, D115): they must not change any value ------------------------------------

#[test]
fn efd_reconstruction_gives_the_first_error() {
    let (_, t) = measure_traced(&render(&polar(5.0, 0.2, 0.0, 0.0), 600.0)).unwrap();
    let w = &t.white;
    let rec = efd_reconstruct(w, 1);
    assert_eq!(rec.len(), w.len());
    // e(1) by brute force: L4 mean of the two nearest-distance sets
    let near = |a: &[[f64; 2]], b: &[[f64; 2]]| -> Vec<f64> {
        a.iter().map(|p| b.iter().map(|q| (p[0] - q[0]).hypot(p[1] - q[1])).fold(f64::INFINITY, f64::min)).collect()
    };
    let m4 = |d: &[f64]| d.iter().map(|v| v.powi(4)).sum::<f64>() / d.len() as f64;
    let e1 = (0.5 * (m4(&near(&rec, w)) + m4(&near(w, &rec)))).powf(0.25) / SQRT_A;
    println!("e(1): curve {} brute force {e1}", t.error_curve[0]);
    assert!((e1 / t.error_curve[0] - 1.0).abs() < 1e-9);
    // many harmonics come close to the line
    let far = near(&efd_reconstruct(w, 64), w).into_iter().fold(0.0, f64::max);
    assert!(far < 2.0, "max distance at k = 64: {far} px");
}

#[test]
fn placement_maps_the_outline_onto_the_input_and_counts_dropped_pixels() {
    let leaf = render(&polar(5.0, 0.2, 0.0, 0.0), 300.0);
    // leaf at (37, 53) in a larger image, plus a 10 x 10 speck that the method drops
    let (r0, c0) = (37, 53);
    let mut raw = Mask::new(leaf.rows + 80, leaf.cols + 120);
    for r in 0..leaf.rows {
        for c in 0..leaf.cols {
            raw.set(r + r0, c + c0, leaf.get(r, c));
        }
    }
    for r in 2..12 {
        for c in 2..12 {
            raw.set(r, c, true);
        }
    }
    let (m, t) = measure_traced(&raw).unwrap();
    let p = t.placement.as_ref().unwrap();
    let leaf_px = leaf.data.iter().filter(|v| **v).count();
    assert_eq!(p.input_size, [raw.rows, raw.cols]);
    assert_eq!((p.leaf_px, p.kept_px), (leaf_px + 100, leaf_px));
    assert!((p.dropped_share() - 100.0 / (leaf_px + 100) as f64).abs() < 1e-15);
    assert_eq!(m.sqrt_a_native, (leaf_px as f64).sqrt());
    // outline in the picture frame of the normalised mask -> input; every point near a leaf/background edge
    let edge = |y: f64, x: f64| -> f64 {
        let mut best = f64::INFINITY;
        for r in (y as usize).saturating_sub(4)..(y as usize + 5).min(raw.rows - 1) {
            for c in (x as usize).saturating_sub(4)..(x as usize + 5).min(raw.cols - 1) {
                if raw.get(r, c) != raw.get(r, c + 1) {
                    best = best.min((x - (c + 1) as f64).abs().hypot(y - (r as f64 + 0.5)));
                }
                if raw.get(r, c) != raw.get(r + 1, c) {
                    best = best.min((x - (c as f64 + 0.5)).hypot(y - (r + 1) as f64));
                }
            }
        }
        best
    };
    let off = 0.5 - PAD_CONTOUR as f64;
    let worst = t
        .outline
        .iter()
        .map(|q| {
            let (x, y) = p.to_input(q[1] + off, q[0] + off);
            edge(y, x)
        })
        .fold(0.0, f64::max);
    println!("crop {:?}, worst outline-to-edge distance {worst} input px", p.crop);
    assert!(worst < 1.0, "{worst}");
}

#[test]
fn mask_check_warnings() {
    assert_eq!(grey_share_of([0u8, 0, 255, 255].into_iter()), 0.0);
    assert_eq!(grey_share_of([255u8; 5].into_iter()), 0.0);
    assert_eq!(grey_share_of([0u8, 31, 32, 223, 224].into_iter()), 0.5);
    let check = |dropped, grey| MaskCheck { dropped_share: dropped, grey_share: grey }.warnings().len();
    assert_eq!(check(0.01, Some(0.5)), 0);
    assert_eq!(check(0.0101, None), 1);
    assert_eq!(check(0.0, Some(0.51)), 1);
    assert_eq!(check(0.2, Some(0.9)), 2);
}

// --- several leaves per image, leaf area, optional columns (D116) ------------------------------------------------

/// Copy `m` into `canvas` at (r0, c0).
fn place(canvas: &mut Mask, m: &Mask, r0: usize, c0: usize) {
    for r in 0..m.rows {
        for c in 0..m.cols {
            if m.get(r, c) {
                canvas.set(r + r0, c + c0, true);
            }
        }
    }
}

fn save_dark(m: &Mask, name: &str) -> PathBuf {
    let p = temp_png(name);
    image::GrayImage::from_fn(m.cols as u32, m.rows as u32, |x, y| {
        image::Luma([if m.get(y as usize, x as usize) { 0 } else { 255 }])
    })
    .save(&p)
    .unwrap();
    p
}

#[test]
fn all_leaves_measures_each_region_alone() {
    let big = render(&polar(5.0, 0.2, 0.0, 0.0), 400.0);
    let mid = render(&ellipse(2.0), 300.0);
    let small = render(&polar(0.0, 0.0, 0.0, 0.0), 100.0);
    // raster order of first pixels: mid (top right), then big, then small
    let mut img = Mask::new(big.rows + mid.rows + 40, big.cols + mid.cols + small.cols + 60);
    place(&mut img, &mid, 5, big.cols + 20);
    place(&mut img, &big, mid.rows + 20, 10);
    place(&mut img, &small, mid.rows + 30, big.cols + mid.cols + 40);
    let path = save_dark(&img, "leaves");
    let (one, _, check1) = measure_file(&path, MaskSource::Dark).unwrap();
    let (leaves, check) = measure_file_leaves(&path, MaskSource::Dark, true).unwrap();
    let _ = std::fs::remove_file(&path);
    let count = |m: &Mask| m.data.iter().filter(|v| **v).count();
    let (nb, nm, ns) = (count(&big), count(&mid), count(&small));
    assert_eq!(leaves.len(), 2);
    assert_eq!(leaves.iter().map(|l| (l.index, l.px)).collect::<Vec<_>>(), [(1, nm), (2, nb)]);
    // the largest region gives the default row bit for bit
    let b = &leaves[1].measurement;
    for (x, y) in [(b.ea, one.ea), (b.mc, one.mc), (b.elongation, one.elongation), (b.sqrt_a_native, one.sqrt_a_native)] {
        assert_eq!(x.to_bits(), y.to_bits());
    }
    assert!((leaves[0].measurement.elongation - 2.0).abs() < 0.05);
    // dropped share: the default drops mid and small, all leaves only small
    let total = (nb + nm + ns) as f64;
    assert!((check1.dropped_share - (nm + ns) as f64 / total).abs() < 1e-15);
    assert!((check.dropped_share - ns as f64 / total).abs() < 1e-15);
    // centroid and placement in the frame of the whole image
    let cy = (0..mid.rows).flat_map(|r| (0..mid.cols).map(move |c| (r, c))).filter(|&(r, c)| mid.get(r, c));
    let (sr, sc) = cy.fold((0.0, 0.0), |a, (r, c)| (a.0 + r as f64, a.1 + c as f64));
    let want = [sc / nm as f64 + (big.cols + 20) as f64 + 0.5, sr / nm as f64 + 5.0 + 0.5];
    assert!((leaves[0].centroid[0] - want[0]).abs() < 1e-9 && (leaves[0].centroid[1] - want[1]).abs() < 1e-9);
    let p = leaves[0].trace.placement.as_ref().unwrap();
    assert_eq!(p.input_size, [img.rows, img.cols]);
    // crop box = bounding box of `mid` in the image
    let first_row = (0..mid.rows).find(|&r| (0..mid.cols).any(|c| mid.get(r, c))).unwrap();
    let first_col = (0..mid.cols).find(|&c| (0..mid.rows).any(|r| mid.get(r, c))).unwrap();
    assert_eq!(p.crop[..2], [5 + first_row, big.cols + 20 + first_col]);
}

#[test]
fn all_leaves_falls_back_to_the_largest_small_region() {
    let a = render(&polar(0.0, 0.0, 0.0, 0.0), 150.0);
    let b = render(&polar(0.0, 0.0, 0.0, 0.0), 200.0);
    let mut img = Mask::new(a.rows.max(b.rows) + 20, a.cols + b.cols + 30);
    place(&mut img, &a, 10, 10);
    place(&mut img, &b, 10, a.cols + 20);
    let path = save_dark(&img, "small");
    let (leaves, _) = measure_file_leaves(&path, MaskSource::Dark, true).unwrap();
    let _ = std::fs::remove_file(&path);
    assert_eq!(leaves.len(), 1);
    assert_eq!(leaves[0].px, b.data.iter().filter(|v| **v).count());
    assert_eq!(leaves[0].measurement.ea_flag, Flag::NotResolved);
}

#[test]
fn optional_columns() {
    assert_eq!(Columns::default().header(), CSV_HEADER);
    let cols = Columns { leaves: true, px_per_mm: Some(20.0) };
    assert_eq!(
        cols.header(),
        "file,leaf,leaf_x,leaf_y,EA,MC,elongation,petiole_len,sqrtA_native,area_mm2,EA_flag,MC_flag,mask_source,version"
    );
    let img = render(&polar(5.0, 0.2, 0.0, 0.0), 300.0);
    let path = save_dark(&img, "cols");
    let (leaves, _) = measure_file_leaves(&path, MaskSource::Dark, true).unwrap();
    let (default, _) = measure_file_leaves(&path, MaskSource::Dark, false).unwrap();
    let _ = std::fs::remove_file(&path);
    let row = cols.row("leaf.png", &leaves[0]);
    let f: Vec<&str> = row.split(',').collect();
    assert_eq!(f.len(), 14);
    assert_eq!(f[12], "dark");
    assert_eq!(f[1], "1");
    assert_eq!(f[9].parse::<f64>().unwrap(), leaves[0].px as f64 / 400.0);
    assert_eq!(Columns::default().row("leaf.png", &default[0]), csv_row("leaf.png", &default[0].measurement, MaskSource::Dark));
    // the same leaf, with and without the option: identical values
    assert_eq!(f[4..9].join(","), csv_row("x", &default[0].measurement, MaskSource::Dark).split(',').skip(1).take(5).collect::<Vec<_>>().join(","));
}

// --- export (D118, D119): drawing and tables only, no value changes -----------------------------------------------

/// Parse a two-column table with a header.
fn table(text: &str, header: &str) -> Vec<(f64, f64)> {
    let mut lines = text.lines();
    assert_eq!(lines.next(), Some(header));
    lines
        .map(|l| {
            let (a, b) = l.split_once(',').unwrap();
            (a.parse().unwrap(), b.parse().unwrap())
        })
        .collect()
}

#[test]
fn curve_tables_hold_the_trace_values() {
    let (m, t) = measure_traced(&render(&arc_teeth(&polar(5.0, 0.2, 0.0, 0.0), 0.01), 600.0)).unwrap();
    let hs = table(&margin_profile_csv(&t), "s,h");
    assert_eq!(hs.len(), t.margin_profile.len());
    let s = arc_length(&t.blade_line);
    for (i, (si, hi)) in hs.iter().enumerate() {
        // the text form of an f64 reads back exactly
        assert_eq!(*hi, t.margin_profile[i] / SQRT_A);
        assert_eq!(*si, s[i] / SQRT_A);
    }
    assert_eq!(hs[0].0, 0.0);
    assert!(hs.windows(2).all(|w| w[1].0 >= w[0].0));
    let ek = table(&error_curve_csv(&t), "k,e");
    assert_eq!(ek.len(), t.error_curve.len());
    for (i, (k, e)) in ek.iter().enumerate() {
        assert_eq!((*k, *e), ((i + 1) as f64, t.error_curve[i]));
    }
    assert!(m.mc > 0.0);
}

#[test]
fn leaf_figure_draws_every_panel() {
    let (m, t) = measure_traced(&render(&arc_teeth(&polar(5.0, 0.2, 0.0, 0.0), 0.01), 600.0)).unwrap();
    let svg = leaf_svg("oak/leaf <1> & co", &m, &t);
    assert!(svg.starts_with("<svg xmlns=\"http://www.w3.org/2000/svg\""));
    assert!(svg.ends_with("</svg>\n"));
    for want in ["(a)", "(b)", "(c)", "(d)", "(e)", "oak/leaf &lt;1&gt; &amp; co", &format!("EA = {:.3}", m.ea), &format!("MC = {:.2}", m.mc)] {
        assert!(svg.contains(want), "{want}");
    }
    assert!(!svg.contains("NaN") && !svg.contains("inf"));
    // a leaf too small for MC: (e) says so, the e(k) table is empty, EA is NA
    let (small, ts) = measure_traced(&render(&polar(5.0, 0.2, 0.0, 0.0), 200.0)).unwrap();
    assert_eq!(small.mc_flag, Flag::NotResolved);
    let svg = leaf_svg("small", &small, &ts);
    assert!(svg.contains("MC not computed") && svg.contains("EA = NA"));
    assert!(!svg.contains("NaN"));
    assert_eq!(error_curve_csv(&ts), "k,e\n");
}

#[test]
fn export_file_names() {
    assert_eq!(leaf_stem("oak/a.png", None), "oak/a");
    assert_eq!(leaf_stem("oak/a.png", Some(2)), "oak/a_leaf2");
    assert_eq!(leaf_stem("dir.v2/a", None), "dir.v2/a");
    assert_eq!(leaf_stem("x/.hidden", None), "x/.hidden");
    assert_eq!(leaf_stem("a.b.tif", None), "a.b");
    assert_eq!(leaf_title("oak/a.png", Some(3)), "oak/a.png, leaf 3");
    assert!(name_clash(["oak/a.png", "oak/a.jpg"]).is_some());
    assert!(name_clash(["oak/a.png", "elm/a.png", "a.png"]).is_none());
}

#[test]
fn morphospace_counts_leaves_without_values() {
    let p = |ea: f64, mc: f64, g: &str| MorphPoint { ea, mc, group: g.into(), flagged: false };
    let svg = morphospace_svg(&[p(0.1, 2.0, "a"), p(0.2, 5.0, "b"), p(f64::NAN, 1.0, "a")], None);
    assert!(svg.contains("2 leaves; 1 not shown"));
    assert!(!svg.contains("NaN"));
    // more than three subfolders: only the highlighted one in colour
    let many: Vec<MorphPoint> = (0..5).map(|i| p(0.1 * i as f64, i as f64, &format!("g{i}"))).collect();
    let svg = morphospace_svg(&many, Some("g3"));
    assert_eq!(svg.matches("fill=\"#2a78d6\"").count(), 2); // the point and its legend key
    assert!(svg.contains("other subfolders (4)"));
}
