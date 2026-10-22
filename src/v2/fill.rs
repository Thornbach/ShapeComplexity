// src/v2/fill.rs - Pillow 11.0.0 filled polygon on a mode "1" image (PORTING.md §7)
//
// `ImageDraw.polygon(xy, fill=1)`: vertices truncated toward zero, edge list with merged horizontal runs,
// scan-line fill in single precision (`polygon_generic` in Draw.c). The corner rule writes `xx[k]` with k an
// edge index, as Pillow does; it is copied on purpose.

use super::Mask;

#[derive(Clone, Copy)]
struct Edge {
    xmin: i32,
    xmax: i32,
    ymin: i32,
    ymax: i32,
    x0: i32,
    y0: i32,
    dx: f32,
}

fn edge(x0: i32, y0: i32, x1: i32, y1: i32) -> Edge {
    Edge {
        xmin: x0.min(x1),
        xmax: x0.max(x1),
        ymin: y0.min(y1),
        ymax: y0.max(y1),
        x0,
        y0,
        dx: if y0 == y1 { 0.0 } else { (x1 - x0) as f32 / (y1 - y0) as f32 },
    }
}

/// x of an edge on scan line y, in f32 as Pillow computes it.
#[inline]
fn x_at(e: &Edge, y: i32) -> f32 {
    (y - e.y0) as f32 * e.dx + e.x0 as f32
}

// Draw.c ROUND_UP / ROUND_DOWN: `f + 0.5F` is a float addition; `fabs(f)` promotes to double.
fn round_up(f: f32) -> i32 {
    if f >= 0.0 {
        (f + 0.5).floor() as i32
    } else {
        -((f.abs() as f64 + 0.5).floor() as i32)
    }
}

fn round_down(f: f32) -> i32 {
    if f >= 0.0 {
        (f - 0.5).ceil() as i32
    } else {
        -((f.abs() as f64 - 0.5).ceil() as i32)
    }
}

fn hline(img: &mut Mask, x0: i32, y: i32, x1: i32) {
    let (w, h) = (img.cols as i32, img.rows as i32);
    if y < 0 || y >= h || x0 >= w || x1 < 0 {
        return;
    }
    let (x0, x1) = (x0.max(0), x1.min(w - 1));
    for x in x0..=x1 {
        img.set(y as usize, x as usize, true);
    }
}

/// Filled polygon with vertices (x = col, y = row) on a `cols` × `rows` mask.
pub fn polygon_fill(cols: usize, rows: usize, xs: &[f64], ys: &[f64]) -> Mask {
    let mut img = Mask::new(rows, cols);
    let n = xs.len();
    if n == 0 {
        return img;
    }
    // `_draw_polygon`: C `(int)` cast
    let xi: Vec<i32> = xs.iter().map(|v| *v as i32).collect();
    let yi: Vec<i32> = ys.iter().map(|v| *v as i32).collect();

    // `ImagingDrawPolygon`: edge list
    let mut edges: Vec<Edge> = Vec::with_capacity(n);
    for i in 0..n - 1 {
        let (x0, y0, x1, y1) = (xi[i], yi[i], xi[i + 1], yi[i + 1]);
        if y0 == y1 && i != 0 && y0 == yi[i - 1] {
            // a horizontal line that follows another horizontal line
            let last = edges.last_mut().unwrap();
            if x1 > x0 && x0 > xi[i - 1] {
                last.xmax = x1;
                continue;
            } else if x1 < x0 && x0 < xi[i - 1] {
                last.xmin = x1;
                continue;
            }
        }
        edges.push(edge(x0, y0, x1, y1));
    }
    if xi[n - 1] != xi[0] || yi[n - 1] != yi[0] {
        edges.push(edge(xi[n - 1], yi[n - 1], xi[0], yi[0]));
    }

    // `polygon_generic`, hasAlpha = 0
    let (mut ymin, mut ymax) = (rows as i32 - 1, 0);
    let mut table: Vec<Edge> = Vec::new();
    for e in &edges {
        ymin = ymin.min(e.ymin);
        ymax = ymax.max(e.ymax);
        if e.ymin == e.ymax {
            hline(&mut img, e.xmin, e.ymin, e.xmax);
            continue;
        }
        table.push(*e);
    }
    let ymin = ymin.max(0);
    let ymax = ymax.min(rows as i32);
    let mut xx = vec![0.0f32; 2 * table.len()]; // not cleared between scan lines, as in Pillow
    for y in ymin..=ymax {
        let mut j = 0;
        for i in 0..table.len() {
            let cur = table[i];
            if !(y >= cur.ymin && y <= cur.ymax) {
                continue;
            }
            xx[j] = x_at(&cur, y);
            j += 1;
            if y == cur.ymax && y < ymax {
                // needed to draw consistent polygons
                xx[j] = xx[j - 1];
                j += 1;
            } else if cur.dx != 0.0 && xx[j - 1].round() == xx[j - 1] {
                // connect discontiguous corners
                for k in 0..i {
                    let other = table[k];
                    if (cur.dx > 0.0 && other.dx <= 0.0) || (cur.dx < 0.0 && other.dx >= 0.0) {
                        continue;
                    }
                    if ((y == cur.ymin && y == other.ymin) || (y == cur.ymax && y == other.ymax))
                        && xx[j - 1] == x_at(&other, y)
                    {
                        let off = if y == ymax { -1 } else { 1 };
                        let a = x_at(&cur, y + off) as f64;
                        let b = x_at(&other, y + off) as f64;
                        xx[k] = if y == cur.ymax {
                            if cur.dx > 0.0 { (a.max(b) + 1.0) as f32 } else { (a.min(b) - 1.0) as f32 }
                        } else if cur.dx > 0.0 {
                            a.min(b) as f32
                        } else {
                            (a.max(b) + 1.0) as f32
                        };
                        break;
                    }
                }
            }
        }
        xx[..j].sort_by(|a, b| a.partial_cmp(b).unwrap());
        let mut i = 1;
        while i < j {
            hline(&mut img, round_up(xx[i - 1]), y, round_down(xx[i]));
            i += 2;
        }
    }
    img
}
