// src/v2/contour.rs - Gaussian smoothing, 0.5 contour and arc-length resampling (PORTING.md §5, §6, §8)
//
// Copies SciPy 1.13.1 `gaussian_filter` / `gaussian_filter1d` (symmetric correlation, kernel normalised with
// NumPy's pairwise sum), scikit-image 0.24.0 `find_contours` (marching squares and segment assembly) and
// NumPy 1.26.4 `interp` / `linspace`.

use std::collections::{HashMap, VecDeque};

use super::{np_sum, round_half_even, Grid, Mask, PAD_CONTOUR, SIGMA_PRE};

/// A point (row, col).
pub type Point = [f64; 2];

/// Normalised Gaussian kernel of radius trunc(4 sigma + 0.5), as SciPy `_gaussian_kernel1d`.
pub fn gaussian_kernel(sigma: f64) -> Vec<f64> {
    let radius = (4.0 * sigma + 0.5) as i64;
    let a = -0.5 / (sigma * sigma);
    let phi: Vec<f64> = (-radius..=radius).map(|x| (a * (x * x) as f64).exp()).collect();
    let total = np_sum(&phi);
    phi.iter().map(|v| v / total).collect()
}

#[derive(Clone, Copy)]
enum Extend {
    /// d c b a | a b c d
    Reflect,
    /// periodic
    Wrap,
}

fn extend_index(i: i64, n: i64, mode: Extend) -> usize {
    match mode {
        Extend::Reflect => {
            let m = i.rem_euclid(2 * n);
            (if m >= n { 2 * n - 1 - m } else { m }) as usize
        }
        Extend::Wrap => i.rem_euclid(n) as usize,
    }
}

/// Symmetric correlation of one line with the kernel (`NI_Correlate1D`, symmetric branch: outer taps first).
fn correlate_line(line: &[f64], kernel: &[f64], mode: Extend, out: &mut [f64]) {
    let n = line.len() as i64;
    let r = (kernel.len() / 2) as i64;
    let ext: Vec<f64> = (-r..n + r).map(|i| line[extend_index(i, n, mode)]).collect();
    for i in 0..n {
        let c = (i + r) as usize;
        let mut acc = ext[c] * kernel[r as usize];
        for j in (1..=r as usize).rev() {
            acc += (ext[c - j] + ext[c + j]) * kernel[r as usize - j];
        }
        out[i as usize] = acc;
    }
}

/// `ndimage.gaussian_filter(a, sigma)` (mode reflect): rows axis first, then columns.
pub fn gaussian_filter_2d(a: &Grid<f64>, sigma: f64) -> Grid<f64> {
    let kernel = gaussian_kernel(sigma);
    // axis 0: along each column
    let t = a.transpose();
    let mut tmp = Grid::new(t.rows, t.cols);
    for r in 0..t.rows {
        let (src, dst) = (&t.data[r * t.cols..(r + 1) * t.cols], &mut tmp.data[r * t.cols..(r + 1) * t.cols]);
        correlate_line(src, &kernel, Extend::Reflect, dst);
    }
    let b = tmp.transpose();
    // axis 1: along each row
    let mut out = Grid::new(b.rows, b.cols);
    for r in 0..b.rows {
        let (src, dst) = (&b.data[r * b.cols..(r + 1) * b.cols], &mut out.data[r * b.cols..(r + 1) * b.cols]);
        correlate_line(src, &kernel, Extend::Reflect, dst);
    }
    out
}

/// `ndimage.gaussian_filter1d(x, sigma, mode="wrap")`.
pub fn gaussian_filter1d_wrap(x: &[f64], sigma: f64) -> Vec<f64> {
    let mut out = vec![0.0; x.len()];
    correlate_line(x, &gaussian_kernel(sigma), Extend::Wrap, &mut out);
    out
}

fn frac(from: f64, to: f64, level: f64) -> f64 {
    if to == from {
        0.0
    } else {
        (level - from) / (to - from)
    }
}

/// Bit pattern of a point, for exact matching (+0.0 for -0.0, as Python's float equality).
fn key(p: Point) -> (u64, u64) {
    ((p[0] + 0.0).to_bits(), (p[1] + 0.0).to_bits())
}

/// `skimage.measure.find_contours(a, level)` with the defaults (fully_connected and positive_orientation "low").
pub fn find_contours(a: &Grid<f64>, level: f64) -> Vec<Vec<Point>> {
    // marching squares, squares in raster order
    let mut segments: Vec<(Point, Point)> = Vec::new();
    for r0 in 0..a.rows.saturating_sub(1) {
        for c0 in 0..a.cols.saturating_sub(1) {
            let (ul, ur) = (a.get(r0, c0), a.get(r0, c0 + 1));
            let (ll, lr) = (a.get(r0 + 1, c0), a.get(r0 + 1, c0 + 1));
            let case = (ul > level) as u8 | ((ur > level) as u8) << 1 | ((ll > level) as u8) << 2 | ((lr > level) as u8) << 3;
            if case == 0 || case == 15 {
                continue;
            }
            let (rf, cf) = (r0 as f64, c0 as f64);
            let top = [rf, cf + frac(ul, ur, level)];
            let bottom = [rf + 1.0, cf + frac(ll, lr, level)];
            let left = [rf + frac(ul, ll, level), cf];
            let right = [rf + frac(ur, lr, level), cf + 1.0];
            match case {
                1 => segments.push((top, left)),
                2 => segments.push((right, top)),
                3 => segments.push((right, left)),
                4 => segments.push((left, bottom)),
                5 => segments.push((top, bottom)),
                6 => {
                    segments.push((right, top));
                    segments.push((left, bottom));
                }
                7 => segments.push((right, bottom)),
                8 => segments.push((bottom, right)),
                9 => {
                    segments.push((top, left));
                    segments.push((bottom, right));
                }
                10 => segments.push((bottom, top)),
                11 => segments.push((bottom, left)),
                12 => segments.push((left, right)),
                13 => segments.push((top, right)),
                14 => segments.push((left, top)),
                _ => unreachable!(),
            }
        }
    }
    assemble(&segments)
}

/// `_assemble_contours`: joins segments into contours; contours are returned in creation order.
fn assemble(segments: &[(Point, Point)]) -> Vec<Vec<Point>> {
    let mut contours: Vec<Option<VecDeque<Point>>> = Vec::new();
    let mut starts: HashMap<(u64, u64), usize> = HashMap::new();
    let mut ends: HashMap<(u64, u64), usize> = HashMap::new();
    for &(from, to) in segments {
        if key(from) == key(to) {
            continue;
        }
        let tail = starts.remove(&key(to));
        let head = ends.remove(&key(from));
        match (tail, head) {
            (Some(t), Some(h)) if t == h => contours[h].as_mut().unwrap().push_back(to),
            (Some(t), Some(h)) => {
                if t > h {
                    let tl = contours[t].take().unwrap();
                    let hd = contours[h].as_mut().unwrap();
                    hd.extend(tl);
                    starts.insert(key(*hd.front().unwrap()), h);
                    ends.insert(key(*hd.back().unwrap()), h);
                } else {
                    let hd = contours[h].take().unwrap();
                    let tl = contours[t].as_mut().unwrap();
                    for p in hd.iter().rev() {
                        tl.push_front(*p);
                    }
                    starts.remove(&key(*hd.front().unwrap()));
                    starts.insert(key(*tl.front().unwrap()), t);
                    ends.insert(key(*tl.back().unwrap()), t);
                }
            }
            (None, None) => {
                let id = contours.len();
                contours.push(Some(VecDeque::from(vec![from, to])));
                starts.insert(key(from), id);
                ends.insert(key(to), id);
            }
            (Some(t), None) => {
                contours[t].as_mut().unwrap().push_front(from);
                starts.insert(key(from), t);
            }
            (None, Some(h)) => {
                contours[h].as_mut().unwrap().push_back(to);
                ends.insert(key(to), h);
            }
        }
    }
    contours.into_iter().flatten().map(|d| d.into_iter().collect()).collect()
}

/// `np.allclose(a, b)` for two points.
fn allclose(a: Point, b: Point) -> bool {
    (0..2).all(|i| (a[i] - b[i]).abs() <= 1e-8 + 1e-5 * b[i].abs())
}

/// Outline of L': first longest 0.5 contour of the Gaussian-smoothed mask, padded by PAD_CONTOUR px,
/// repeated end point dropped. Coordinates in the padded frame.
pub fn outline(lp: &Mask) -> Vec<Point> {
    let (p, rows, cols) = (PAD_CONTOUR, lp.rows + 2 * PAD_CONTOUR, lp.cols + 2 * PAD_CONTOUR);
    let mut a = Grid::new(rows, cols);
    for r in 0..lp.rows {
        for c in 0..lp.cols {
            if lp.get(r, c) {
                a.set(r + p, c + p, 1.0);
            }
        }
    }
    let contours = find_contours(&gaussian_filter_2d(&a, SIGMA_PRE), 0.5);
    let mut best: Option<Vec<Point>> = None;
    for c in contours {
        if best.as_ref().is_none_or(|b| c.len() > b.len()) {
            best = Some(c);
        }
    }
    let mut c = best.unwrap_or_default();
    if c.len() > 1 && allclose(c[0], c[c.len() - 1]) {
        c.pop();
    }
    c
}

/// Closed polygon resampled by arc length at about `step` px. Returns the points and the exact step.
pub fn resample(c: &[Point], step: f64) -> (Vec<Point>, f64) {
    let m = c.len();
    let closed = |i: usize| c[i % m];
    let mut seg = vec![0.0; m + 1];
    for i in 0..m {
        let (a, b) = (closed(i), closed(i + 1));
        seg[i + 1] = seg[i] + (b[0] - a[0]).hypot(b[1] - a[1]);
    }
    let total = seg[m];
    let n = round_half_even(total / step).max(16) as usize;
    let ds = total / n as f64;
    let mut out = Vec::with_capacity(n);
    let mut j = 0;
    for i in 0..n {
        let s = i as f64 * ds + 0.0;
        while j < m && seg[j + 1] <= s {
            j += 1; // last index with seg[j] <= s
        }
        let p = if j == m || seg[j] == s {
            closed(j)
        } else {
            let (a, b) = (closed(j), closed(j + 1));
            let d = seg[j + 1] - seg[j];
            [(b[0] - a[0]) / d * (s - seg[j]) + a[0], (b[1] - a[1]) / d * (s - seg[j]) + a[1]]
        };
        out.push(p);
    }
    (out, ds)
}
