// src/v2/metrics.rs - EA, whitening and elongation, elliptic Fourier error curve and MC (PORTING.md §9-§11)

use std::f64::consts::PI;

use super::contour::{resample, Point};
use super::fill::polygon_fill;
use super::{np_sum, K_MAX, SQRT_A, STOP, TAU};

/// Edge amount (D30, "B8"): extra length of the margin profile h(s) unrolled along the blade line.
pub fn edge_amount(c: &[Point], bl: &[Point]) -> f64 {
    let n = bl.len();
    let next = |i: usize| (i + 1) % n;
    let prev = |i: usize| (i + n - 1) % n;
    let h: Vec<f64> = (0..n)
        .map(|i| {
            let t = [bl[next(i)][0] - bl[prev(i)][0], bl[next(i)][1] - bl[prev(i)][1]];
            let len = t[0].hypot(t[1]);
            let nrm = [-t[1] / len, t[0] / len];
            (c[i][0] - bl[i][0]) * nrm[0] + (c[i][1] - bl[i][1]) * nrm[1]
        })
        .collect();
    let ds: Vec<f64> = (0..n).map(|i| (bl[next(i)][0] - bl[i][0]).hypot(bl[next(i)][1] - bl[i][1])).collect();
    let extra: Vec<f64> = (0..n).map(|i| ds[i].hypot(h[next(i)] - h[i]) - ds[i]).collect();
    np_sum(&extra) / np_sum(&ds)
}

/// Eigen-decomposition of a symmetric 2 x 2 matrix [[a, b], [b, d]]: eigenvalues ascending and unit vectors.
fn eigh2(a: f64, b: f64, d: f64) -> ([f64; 2], [[f64; 2]; 2]) {
    if b == 0.0 {
        return if a <= d { ([a, d], [[1.0, 0.0], [0.0, 1.0]]) } else { ([d, a], [[0.0, 1.0], [1.0, 0.0]]) };
    }
    let mean = (a + d) / 2.0;
    let rad = (((a - d) / 2.0).powi(2) + b * b).sqrt();
    let w = [mean - rad, mean + rad];
    let vec = |l: f64| {
        // the better-conditioned of (b, l - a) and (l - d, b)
        let (u, v) = if (l - a).abs() > (l - d).abs() { (b, l - a) } else { (l - d, b) };
        let n = u.hypot(v);
        [u / n, v / n]
    };
    (w, [vec(w[0]), vec(w[1])])
}

/// D34/D35/D46: whitened, resampled blade line and the elongation. `bl` is in the L' frame (padding removed).
pub fn whiten(bl: &[Point], rows: usize, cols: usize) -> (Vec<Point>, f64) {
    let xs: Vec<f64> = bl.iter().map(|p| p[1]).collect();
    let ys: Vec<f64> = bl.iter().map(|p| p[0]).collect();
    let poly = polygon_fill(cols, rows, &xs, &ys);
    let px: Vec<(f64, f64)> = (0..rows)
        .flat_map(|r| (0..cols).map(move |c| (r, c)))
        .filter(|&(r, c)| poly.get(r, c))
        .map(|(r, c)| (r as f64, c as f64))
        .collect();
    let n = px.len() as f64;
    // integer sums: exact in any order
    let mu = [px.iter().map(|p| p.0).sum::<f64>() / n, px.iter().map(|p| p.1).sum::<f64>() / n];
    let (mut srr, mut src, mut scc) = (0.0, 0.0, 0.0);
    for (r, c) in &px {
        let (dr, dc) = (r - mu[0], c - mu[1]);
        srr += dr * dr;
        src += dr * dc;
        scc += dc * dc;
    }
    let (srr, src, scc) = (srr / (n - 1.0), src / (n - 1.0), scc / (n - 1.0));
    let (w, v) = eigh2(srr, src, scc);
    // W = v diag(1 / sqrt(w)) v^T * (w0 w1)^(1/4)
    let scale = (w[0] * w[1]).powf(0.25);
    let mut wm = [[0.0; 2]; 2];
    for (i, row) in wm.iter_mut().enumerate() {
        for (j, x) in row.iter_mut().enumerate() {
            *x = (v[0][i] * v[0][j] / w[0].sqrt() + v[1][i] * v[1][j] / w[1].sqrt()) * scale;
        }
    }
    let white: Vec<Point> = bl
        .iter()
        .map(|p| {
            let (a, b) = (p[0] - mu[0], p[1] - mu[1]);
            [a * wm[0][0] + b * wm[0][1], a * wm[1][0] + b * wm[1][1]]
        })
        .collect();
    (resample(&white, 1.0).0, (w[1] / w[0]).sqrt())
}

/// Elliptic Fourier coefficients (Kuhl & Giardina 1982), x = col, y = row, arc-length parameter.
struct Efd {
    xy: Vec<[f64; 2]>,
    t: Vec<f64>,
    period: f64,
    dc: [f64; 2],
    co: Vec<[f64; 4]>,
}

fn efd(c: &[Point], n_harm: usize) -> Efd {
    let m = c.len();
    let all: Vec<[f64; 2]> = c.iter().map(|p| [p[1], p[0]]).collect();
    let mut xy = Vec::new();
    let mut d = Vec::new();
    let mut dt = Vec::new();
    for i in 0..m {
        let (a, b) = (all[i], all[(i + 1) % m]);
        let di = [b[0] - a[0], b[1] - a[1]];
        let len = di[0].hypot(di[1]);
        if len > 0.0 {
            xy.push(a);
            d.push(di);
            dt.push(len);
        }
    }
    let k = xy.len();
    let mut t = vec![0.0; k + 1];
    for i in 0..k {
        t[i + 1] = t[i] + dt[i];
    }
    let period = t[k];
    // mean of the piecewise-linear x(t), y(t); axis-0 sums run in order
    let mut dc = [0.0, 0.0];
    for i in 0..k {
        let nx = xy[(i + 1) % k];
        for j in 0..2 {
            dc[j] += 0.5 * (xy[i][j] + nx[j]) * dt[i];
        }
    }
    let dc = [dc[0] / period, dc[1] / period];
    let mut co = Vec::with_capacity(n_harm);
    for n in 1..=n_harm {
        let phi: Vec<f64> = t.iter().map(|ti| 2.0 * n as f64 * PI * ti / period).collect();
        let (cs, sn): (Vec<f64>, Vec<f64>) = phi.iter().map(|p| (p.cos(), p.sin())).unzip();
        let kk = period / (2.0 * (n * n) as f64 * (PI * PI));
        let term = |j: usize, trig: &[f64]| -> f64 {
            let v: Vec<f64> = (0..k).map(|i| d[i][j] / dt[i] * (trig[i + 1] - trig[i])).collect();
            np_sum(&v)
        };
        co.push([kk * term(0, &cs), kk * term(0, &sn), kk * term(1, &cs), kk * term(1, &sn)]);
    }
    t.pop();
    Efd { xy, t, period, dc, co }
}

/// Exact nearest-neighbour distances on a uniform grid of cells.
struct NearestGrid {
    pts: Vec<[f64; 2]>,
    origin: [f64; 2],
    cell: f64,
    dims: [usize; 2],
    start: Vec<usize>,
    order: Vec<usize>,
}

impl NearestGrid {
    fn new(pts: &[[f64; 2]]) -> Self {
        let lo = pts.iter().fold([f64::INFINITY; 2], |a, p| [a[0].min(p[0]), a[1].min(p[1])]);
        let hi = pts.iter().fold([f64::NEG_INFINITY; 2], |a, p| [a[0].max(p[0]), a[1].max(p[1])]);
        let extent = (hi[0] - lo[0]).max(hi[1] - lo[1]).max(1e-9);
        let cell = (extent / (pts.len() as f64).sqrt()).max(1e-9);
        let dims = [((hi[0] - lo[0]) / cell) as usize + 1, ((hi[1] - lo[1]) / cell) as usize + 1];
        let idx = |p: &[f64; 2]| -> usize {
            let a = (((p[0] - lo[0]) / cell) as usize).min(dims[0] - 1);
            let b = (((p[1] - lo[1]) / cell) as usize).min(dims[1] - 1);
            a * dims[1] + b
        };
        let mut count = vec![0usize; dims[0] * dims[1] + 1];
        for p in pts {
            count[idx(p) + 1] += 1;
        }
        for i in 1..count.len() {
            count[i] += count[i - 1];
        }
        let mut fill = count.clone();
        let mut order = vec![0; pts.len()];
        for (i, p) in pts.iter().enumerate() {
            let k = idx(p);
            order[fill[k]] = i;
            fill[k] += 1;
        }
        NearestGrid { pts: pts.to_vec(), origin: lo, cell, dims, start: count, order }
    }

    /// Distance to the nearest point: sqrt(dx^2 + dy^2) of the minimum, as cKDTree with p = 2.
    fn query(&self, q: &[f64; 2]) -> f64 {
        let ci = ((q[0] - self.origin[0]) / self.cell).floor() as i64;
        let cj = ((q[1] - self.origin[1]) / self.cell).floor() as i64;
        let mut best = f64::INFINITY;
        let (n0, n1) = (self.dims[0] as i64, self.dims[1] as i64);
        let mut ring = 0i64;
        loop {
            for i in ci - ring..=ci + ring {
                for j in cj - ring..=cj + ring {
                    if (i - ci).abs() != ring && (j - cj).abs() != ring {
                        continue; // inner cells were done in earlier rings
                    }
                    if i < 0 || j < 0 || i >= n0 || j >= n1 {
                        continue;
                    }
                    let k = (i * n1 + j) as usize;
                    for &p in &self.order[self.start[k]..self.start[k + 1]] {
                        let (dx, dy) = (q[0] - self.pts[p][0], q[1] - self.pts[p][1]);
                        let d2 = dx * dx + dy * dy;
                        if d2 < best {
                            best = d2;
                        }
                    }
                }
            }
            // every point outside the searched square is at least `ring * cell` away from q
            let reach = ring as f64 * self.cell;
            let outside_all = ci - ring <= 0 && cj - ring <= 0 && ci + ring >= n0 - 1 && cj + ring >= n1 - 1;
            if (best.is_finite() && reach * reach >= best) || outside_all {
                return best.sqrt();
            }
            ring += 1;
        }
    }
}

/// e(k) / sqrt(A), k = 1, 2, ...: L4 mean of the two nearest-distance sets, running minimum.
pub fn error_curve(w: &[Point]) -> Vec<f64> {
    let f = efd(w, K_MAX);
    let tree = NearestGrid::new(&f.xy);
    let phi: Vec<f64> = f.t.iter().map(|ti| 2.0 * PI * ti / f.period).collect();
    let mut rec: Vec<[f64; 2]> = vec![f.dc; f.t.len()];
    let mut e = Vec::new();
    for k in 1..=K_MAX {
        let co = f.co[k - 1];
        for (i, p) in phi.iter().enumerate() {
            let a = k as f64 * p;
            let (cs, sn) = (a.cos(), a.sin());
            rec[i] = [rec[i][0] + (co[0] * cs + co[1] * sn), rec[i][1] + (co[2] * cs + co[3] * sn)];
        }
        let d1: Vec<f64> = rec.iter().map(|q| tree.query(q)).collect();
        let rec_tree = NearestGrid::new(&rec);
        let d2: Vec<f64> = f.xy.iter().map(|q| rec_tree.query(q)).collect();
        let m4 = |d: &[f64]| np_sum(&d.iter().map(|v| v.powf(4.0)).collect::<Vec<_>>()) / d.len() as f64;
        e.push((0.5 * (m4(&d1) + m4(&d2))).powf(0.25));
        let hausdorff = d1.iter().chain(&d2).fold(0.0f64, |a, b| a.max(*b));
        if hausdorff < STOP * SQRT_A {
            break;
        }
    }
    let mut run = f64::INFINITY;
    e.iter()
        .map(|v| {
            run = run.min(v / SQRT_A);
            run
        })
        .collect()
}

/// D40: MC = sum_k max(0, ln(e(k) / tau)).
pub fn macro_complexity(w: &[Point]) -> f64 {
    let terms: Vec<f64> = error_curve(w).iter().map(|e| (e / TAU).ln().max(0.0)).collect();
    np_sum(&terms)
}
