// src/v2/morph.rs - exact distance transform, opening by a disk, petiole removal (PORTING.md §4)

use super::mask::{label, largest_filled};
use super::{Grid, Mask, PET_ELONG, PET_LEN, R_STAR, SQRT_A, TOL};

const INF: i64 = i64::MAX / 4;

/// 1-D squared distance transform of a sampled function (Felzenszwalb & Huttenlocher 2012). Exact for integers.
fn dt_1d(f: &[i64], out: &mut [i64]) {
    let n = f.len();
    let mut v = vec![0usize; n];
    let mut z = vec![0f64; n + 1];
    let mut k = 0usize;
    let mut first = None;
    for (q, fq) in f.iter().enumerate() {
        if *fq < INF {
            first = Some(q);
            break;
        }
    }
    let Some(q0) = first else {
        out.iter_mut().for_each(|o| *o = INF);
        return;
    };
    v[0] = q0;
    z[0] = f64::NEG_INFINITY;
    z[1] = f64::INFINITY;
    let s = |q: usize, p: usize| -> f64 {
        ((f[q] + (q * q) as i64) - (f[p] + (p * p) as i64)) as f64 / (2.0 * (q as f64 - p as f64))
    };
    for q in q0 + 1..n {
        if f[q] >= INF {
            continue;
        }
        let mut sq = s(q, v[k]);
        while sq <= z[k] {
            k -= 1;
            sq = s(q, v[k]);
        }
        k += 1;
        v[k] = q;
        z[k] = sq;
        z[k + 1] = f64::INFINITY;
    }
    k = 0;
    for (q, o) in out.iter_mut().enumerate() {
        while z[k + 1] < q as f64 {
            k += 1;
        }
        let d = q as i64 - v[k] as i64;
        *o = d * d + f[v[k]];
    }
}

/// Squared Euclidean distance of every pixel to the nearest pixel where `target` is true.
/// With no target pixel at all, SciPy measures to a phantom pixel at (-1, 0); this is copied.
pub fn sq_distance_to(target: &Mask) -> Grid<i64> {
    let (rows, cols) = (target.rows, target.cols);
    if !target.data.iter().any(|v| *v) {
        let mut g = Grid::new(rows, cols);
        for r in 0..rows {
            for c in 0..cols {
                g.set(r, c, ((r + 1) * (r + 1) + c * c) as i64);
            }
        }
        return g;
    }
    let mut g: Grid<i64> = Grid::from_vec(rows, cols, target.data.iter().map(|t| if *t { 0 } else { INF }).collect());
    // columns
    let mut col = vec![0i64; rows];
    let mut out = vec![0i64; rows];
    for c in 0..cols {
        for r in 0..rows {
            col[r] = g.get(r, c);
        }
        dt_1d(&col, &mut out);
        for r in 0..rows {
            g.set(r, c, out[r]);
        }
    }
    // rows
    let mut out = vec![0i64; cols];
    for r in 0..rows {
        let row = g.data[r * cols..(r + 1) * cols].to_vec();
        dt_1d(&row, &mut out);
        g.data[r * cols..(r + 1) * cols].copy_from_slice(&out);
    }
    g
}

/// Opening of `m` by a disk of radius `r_px` via distance transforms, with TOL px slack at the outline.
/// Distances are compared as SciPy does: sqrt of the squared integer distance, in f64.
pub fn open_disk(m: &Mask, r_px: f64) -> Mask {
    let background = Grid::from_vec(m.rows, m.cols, m.data.iter().map(|v| !v).collect());
    let d_in = sq_distance_to(&background);
    let core = Grid::from_vec(m.rows, m.cols, d_in.data.iter().map(|d| (*d as f64).sqrt() >= r_px).collect());
    let d_core = sq_distance_to(&core);
    Grid::from_vec(
        m.rows,
        m.cols,
        d_core.data.iter().zip(&m.data).map(|(d, v)| (*d as f64).sqrt() <= r_px + TOL && *v).collect(),
    )
}

/// Major and minor axis lengths of each label (skimage 0.24 `regionprops`: 4 sqrt of the inertia eigenvalues).
pub fn region_axes(lab: &Grid<u32>, n: u32) -> Vec<(f64, f64)> {
    // bounding boxes
    let mut bb = vec![(usize::MAX, 0usize, usize::MAX, 0usize); n as usize + 1];
    for r in 0..lab.rows {
        for c in 0..lab.cols {
            let l = lab.get(r, c) as usize;
            if l > 0 {
                let b = &mut bb[l];
                *b = (b.0.min(r), b.1.max(r), b.2.min(c), b.3.max(c));
            }
        }
    }
    (1..=n as usize)
        .map(|l| {
            let (r0, r1, c0, c1) = bb[l];
            let px: Vec<(f64, f64)> = (r0..=r1)
                .flat_map(|r| (c0..=c1).map(move |c| (r, c)))
                .filter(|&(r, c)| lab.get(r, c) as usize == l)
                .map(|(r, c)| ((r - r0) as f64, (c - c0) as f64))
                .collect();
            let m00 = px.len() as f64;
            let rbar = px.iter().map(|p| p.0).sum::<f64>() / m00;
            let cbar = px.iter().map(|p| p.1).sum::<f64>() / m00;
            let (mut mu20, mut mu02, mut mu11) = (0.0, 0.0, 0.0);
            for (r, c) in &px {
                let (dr, dc) = (r - rbar, c - cbar);
                mu20 += dr * dr;
                mu02 += dc * dc;
                mu11 += dr * dc;
            }
            // inertia tensor [[mu02, -mu11], [-mu11, mu20]] / mu00; eigenvalues clipped at 0, descending
            let (a, b, d) = (mu02 / m00, -mu11 / m00, mu20 / m00);
            let mean = (a + d) / 2.0;
            let rad = (((a - d) / 2.0).powi(2) + b * b).sqrt();
            let (l1, l2) = ((mean + rad).max(0.0), (mean - rad).max(0.0));
            (4.0 * l1.sqrt(), 4.0 * l2.sqrt())
        })
        .collect()
}

/// D29 petiole removal. Returns L' and the petiole length / sqrt(A) (0 = none found).
pub fn remove_petiole(m: &Mask) -> (Mask, f64) {
    let opened = open_disk(m, R_STAR * SQRT_A);
    let pieces = Grid::from_vec(m.rows, m.cols, m.data.iter().zip(&opened.data).map(|(a, o)| *a && !o).collect());
    let (lab, n) = label(&pieces, true);
    if n == 0 {
        return (largest_filled(m), 0.0);
    }
    let axes = region_axes(&lab, n);
    let mut best = 0;
    for (i, ax) in axes.iter().enumerate() {
        if ax.0 > axes[best].0 {
            best = i;
        }
    }
    let (major, minor) = axes[best];
    if major < PET_LEN * SQRT_A || major < PET_ELONG * minor.max(1e-9) {
        return (largest_filled(m), 0.0);
    }
    let best_label = best as u32 + 1;
    let rest = Grid::from_vec(m.rows, m.cols, m.data.iter().zip(&lab.data).map(|(a, l)| *a && *l != best_label).collect());
    (largest_filled(&rest), major / SQRT_A)
}
