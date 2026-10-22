// tests/v2_steps.rs - v2 steps against the fixtures of the Python reference (tests/fixtures/README.md)
//
// Each library step must match the reference bit for bit. The tests skip when the fixture folder is missing
// (it is not part of the packaged crate).

use std::fs;
use std::path::{Path, PathBuf};

use shapecomplexity::v2::contour::{find_contours, gaussian_filter1d_wrap, gaussian_filter_2d, outline, resample, Point};
use shapecomplexity::v2::fill::polygon_fill;
use shapecomplexity::v2::resize::resize_bilinear;
use shapecomplexity::v2::{measure_normalised_traced, normalise, Grid, Mask, PAD_CONTOUR, R_STAR, SQRT_A};

fn fixtures() -> Option<PathBuf> {
    let p = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/public");
    if p.exists() {
        Some(p)
    } else {
        eprintln!("fixtures missing, test skipped: {}", p.display());
        None
    }
}

/// CSV with a header: rows of named columns (all numbers, or text kept as NaN).
fn read_table(p: &Path) -> (Vec<String>, Vec<Vec<String>>) {
    let text = fs::read_to_string(p).unwrap_or_else(|e| panic!("{}: {e}", p.display()));
    let mut lines = text.lines();
    let head = lines.next().unwrap().split(',').map(String::from).collect();
    let rows = lines.filter(|l| !l.is_empty()).map(|l| l.split(',').map(String::from).collect()).collect();
    (head, rows)
}

fn column(head: &[String], rows: &[Vec<String>], name: &str) -> Vec<String> {
    let i = head.iter().position(|h| h == name).unwrap_or_else(|| panic!("no column {name}"));
    rows.iter().map(|r| r[i].clone()).collect()
}

fn num(s: &str) -> f64 {
    s.parse().unwrap_or_else(|_| panic!("not a number: {s}"))
}

fn usz(s: &str) -> usize {
    num(s) as usize
}

/// CSV without a header: a numeric grid.
fn read_grid(p: &Path) -> Grid<f64> {
    let text = fs::read_to_string(p).unwrap();
    let rows: Vec<Vec<f64>> = text.lines().filter(|l| !l.is_empty()).map(|l| l.split(',').map(num).collect()).collect();
    let (r, c) = (rows.len(), rows[0].len());
    Grid::from_vec(r, c, rows.into_iter().flatten().collect())
}

fn read_points(p: &Path) -> Vec<Point> {
    let (_, rows) = read_table(p);
    rows.iter().map(|r| [num(&r[0]), num(&r[1])]).collect()
}

fn read_gray(p: &Path) -> Grid<u8> {
    let im = image::open(p).unwrap_or_else(|e| panic!("{}: {e}", p.display())).to_luma8();
    Grid::from_vec(im.height() as usize, im.width() as usize, im.into_raw())
}

fn read_mask(p: &Path) -> Mask {
    let g = read_gray(p);
    Grid::from_vec(g.rows, g.cols, g.data.iter().map(|v| *v > 0).collect())
}

fn bits(a: &[f64]) -> Vec<u64> {
    a.iter().map(|v| v.to_bits()).collect()
}

fn points_equal(a: &[Point], b: &[Point]) -> bool {
    a.len() == b.len() && a.iter().zip(b).all(|(p, q)| p[0].to_bits() == q[0].to_bits() && p[1].to_bits() == q[1].to_bits())
}

fn max_diff(a: &[Point], b: &[Point]) -> f64 {
    a.iter().zip(b).map(|(p, q)| (p[0] - q[0]).abs().max((p[1] - q[1]).abs())).fold(0.0, f64::max)
}

/// Keys of the leaves that have stage CSVs.
fn stage_leaves(fx: &Path) -> Vec<(String, usize, usize)> {
    let (head, rows) = read_table(&fx.join("leaves/values.csv"));
    let (key, st, h, w) = (column(&head, &rows, "key"), column(&head, &rows, "stages"), column(&head, &rows, "norm_h"), column(&head, &rows, "norm_w"));
    (0..rows.len()).filter(|&i| st[i] == "True").map(|i| (key[i].clone(), usz(&h[i]), usz(&w[i]))).collect()
}

#[test]
fn resize_unit_cases() {
    let Some(fx) = fixtures() else { return };
    let d = fx.join("unit/resize");
    let (head, rows) = read_table(&d.join("cases.csv"));
    let (id, ow, oh) = (column(&head, &rows, "id"), column(&head, &rows, "out_w"), column(&head, &rows, "out_h"));
    let mut bad = Vec::new();
    for i in 0..rows.len() {
        let src = read_gray(&d.join(format!("{}_in.png", id[i])));
        let want = read_gray(&d.join(format!("{}_out.png", id[i])));
        if resize_bilinear(&src, usz(&ow[i]), usz(&oh[i])) != want {
            bad.push(id[i].clone());
        }
    }
    assert!(bad.is_empty(), "resize differs on cases {bad:?}");
}

#[test]
fn gaussian_unit_cases() {
    let Some(fx) = fixtures() else { return };
    let d = fx.join("unit/gauss");
    let (head, rows) = read_table(&d.join("cases.csv"));
    let (id, s2, s1) = (column(&head, &rows, "id"), column(&head, &rows, "sigma_2d"), column(&head, &rows, "sigma_1d"));
    let mut bad = Vec::new();
    for i in 0..rows.len() {
        let a = read_grid(&d.join(format!("{}_2d_in.csv", id[i])));
        let want = read_grid(&d.join(format!("{}_2d_out.csv", id[i])));
        let ok2 = bits(&gaussian_filter_2d(&a, num(&s2[i])).data) == bits(&want.data);
        let line = read_points(&d.join(format!("{}_1d.csv", id[i])));
        let x: Vec<f64> = line.iter().map(|p| p[0]).collect();
        let y: Vec<f64> = line.iter().map(|p| p[1]).collect();
        let ok1 = bits(&gaussian_filter1d_wrap(&x, num(&s1[i]))) == bits(&y);
        if !(ok1 && ok2) {
            bad.push(format!("{} (2-D {ok2}, 1-D {ok1})", id[i]));
        }
    }
    assert!(bad.is_empty(), "Gaussian differs on cases {bad:?}");
}

#[test]
fn contour_unit_cases() {
    let Some(fx) = fixtures() else { return };
    let d = fx.join("unit/contour");
    let (head, rows) = read_table(&d.join("cases.csv"));
    let id = column(&head, &rows, "id");
    let mut bad = Vec::new();
    for i in 0..rows.len() {
        let a = read_grid(&d.join(format!("{}_in.csv", id[i])));
        let (_, out) = read_table(&d.join(format!("{}_out.csv", id[i])));
        let mut want: Vec<Vec<Point>> = Vec::new();
        for r in &out {
            let k = usz(&r[0]);
            if want.len() <= k {
                want.resize(k + 1, Vec::new());
            }
            want[k].push([num(&r[1]), num(&r[2])]);
        }
        let got = find_contours(&a, 0.5);
        if got.len() != want.len() || !got.iter().zip(&want).all(|(g, w)| points_equal(g, w)) {
            bad.push(id[i].clone());
        }
    }
    assert!(bad.is_empty(), "find_contours differs on cases {bad:?}");
}

#[test]
fn polygon_unit_cases() {
    let Some(fx) = fixtures() else { return };
    let d = fx.join("unit/polygon");
    let (head, rows) = read_table(&d.join("cases.csv"));
    let (id, w, h) = (column(&head, &rows, "id"), column(&head, &rows, "width"), column(&head, &rows, "height"));
    let mut bad = Vec::new();
    for i in 0..rows.len() {
        let xy = read_points(&d.join(format!("{}_xy.csv", id[i])));
        let xs: Vec<f64> = xy.iter().map(|p| p[0]).collect();
        let ys: Vec<f64> = xy.iter().map(|p| p[1]).collect();
        if polygon_fill(usz(&w[i]), usz(&h[i]), &xs, &ys) != read_mask(&d.join(format!("{}_mask.png", id[i]))) {
            bad.push(id[i].clone());
        }
    }
    assert!(bad.is_empty(), "polygon fill differs on cases {bad:?}");
}

/// Leaf stages: L' → contour → resampled outline → blade line → filled polygon, each from the previous fixture.
#[test]
fn leaf_stages() {
    let Some(fx) = fixtures() else { return };
    let mut bad = Vec::new();
    let leaves = stage_leaves(&fx);
    assert_eq!(leaves.len(), 20);
    for (key, h, w) in &leaves {
        let d = fx.join("leaves").join(key);
        let lp = read_mask(&d.join("lp.png"));
        let contour = read_points(&d.join("contour.csv"));
        if !points_equal(&outline(&lp), &contour) {
            bad.push(format!("{key}: contour"));
        }
        let (c, step) = resample(&contour, 1.0);
        let want_c = read_points(&d.join("outline.csv"));
        if !points_equal(&c, &want_c) {
            bad.push(format!("{key}: outline (max |d| {:.1e})", max_diff(&c, &want_c)));
        }
        let sig = R_STAR * SQRT_A / step;
        let x: Vec<f64> = want_c.iter().map(|p| p[0]).collect();
        let y: Vec<f64> = want_c.iter().map(|p| p[1]).collect();
        let (bx, by) = (gaussian_filter1d_wrap(&x, sig), gaussian_filter1d_wrap(&y, sig));
        let bl: Vec<Point> = bx.iter().zip(&by).map(|(a, b)| [*a, *b]).collect();
        let want_bl = read_points(&d.join("bl.csv"));
        if !points_equal(&bl, &want_bl) {
            bad.push(format!("{key}: blade line (max |d| {:.1e})", max_diff(&bl, &want_bl)));
        }
        let pad = PAD_CONTOUR as f64;
        let xs: Vec<f64> = want_bl.iter().map(|p| p[1] - pad).collect();
        let ys: Vec<f64> = want_bl.iter().map(|p| p[0] - pad).collect();
        if polygon_fill(*w, *h, &xs, &ys) != read_mask(&d.join("polygon.png")) {
            bad.push(format!("{key}: polygon"));
        }
    }
    assert!(bad.is_empty(), "leaf stages differ: {bad:#?}");
}

/// Whole pipeline on every public leaf: A1 (normalised mask, L') and A2 (|d| <= 1e-9, flags) as in D111.
#[test]
fn leaf_values() {
    let Some(fx) = fixtures() else { return };
    let (head, rows) = read_table(&fx.join("leaves/values.csv"));
    let get = |name: &str| column(&head, &rows, name);
    let (key, mode, ea_flag, mc_flag) = (get("key"), get("mode"), get("EA_flag"), get("MC_flag"));
    let vals: Vec<Vec<String>> = ["EA", "MC", "elongation", "petiole_len"].iter().map(|n| get(n)).collect();
    let mut bad = Vec::new();
    let mut worst = 0.0f64;
    for i in 0..rows.len() {
        let d = fx.join("leaves").join(&key[i]);
        let input = read_mask(&d.join("input.png"));
        let (m, native) = if mode[i] == "raw" {
            let (m, native) = normalise(&input).unwrap();
            if m != read_mask(&d.join("norm.png")) {
                bad.push(format!("{}: normalised mask", key[i]));
            }
            (m, Some(native))
        } else {
            (input, None)
        };
        let (r, trace) = measure_normalised_traced(&m, native);
        if trace.lp != read_mask(&d.join("lp.png")) {
            bad.push(format!("{}: L'", key[i]));
        }
        for (got, want) in [r.ea, r.mc, r.elongation, r.petiole_len].iter().zip(&vals) {
            let w: f64 = if want[i].is_empty() { f64::NAN } else { num(&want[i]) };
            let diff = if got.is_nan() && w.is_nan() { 0.0 } else { (got - w).abs() };
            worst = worst.max(if diff.is_nan() { f64::INFINITY } else { diff });
            if !(diff <= 1e-9) {
                bad.push(format!("{}: value {got} vs {w}", key[i]));
            }
        }
        if r.ea_flag.as_str() != ea_flag[i] || r.mc_flag.as_str() != mc_flag[i] {
            bad.push(format!("{}: flags", key[i]));
        }
    }
    eprintln!("leaf_values: {} masks, max |difference| {worst:.1e}", rows.len());
    assert!(bad.is_empty(), "leaf values differ: {bad:#?}");
}
