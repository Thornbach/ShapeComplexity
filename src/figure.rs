// src/figure.rs - pictures and curve tables of measurements, shared by the CLI and the GUI (D114 D, D118, D119)
//
// Hand-written SVG, no drawing dependency: one leaf in the layout of Fig. 1 of the paper, and the EA-MC morphospace
// of many leaves. Colours of `analysis/figures/style.py`. Drawing only: nothing here changes a value.

use std::fmt::Write;

use crate::batch::VERSION;
use crate::v2::contour::{find_contours, gaussian_filter_2d, Point};
use crate::v2::metrics::efd_reconstruct;
use crate::v2::{Grid, Mask, Measurement, Trace, PAD_CONTOUR, SIGMA_PRE, SQRT_A, TAU};

// colours of the paper figures (analysis/figures/style.py)
const INK: &str = "#0b0b0b";
const INK2: &str = "#52514e";
const MUTED: &str = "#898781";
const GRID: &str = "#e1e0d9";
const AXIS: &str = "#c3c2b7";
const LEAF: &str = "#9a9890";
const BLUE: &str = "#2a78d6";
const ORANGE: &str = "#eb6834";
const AQUA: &str = "#1baf7a";
/// Margin inside the blade line (Fig. 1 b).
const PEACH: &str = "#f7c3a8";
/// Filled holes (as the GUI mask picture).
const FILLED: &str = "#cdcbc3";
/// Fourier fits with K_SHOW harmonics.
const RAMP3: [&str; 3] = ["#86b6ef", "#2a78d6", "#104281"];
const K_SHOW: [usize; 3] = [1, 3, 8];
/// Shortest k axis in (e).
const K_AX: usize = 30;
/// Morphospace colours for at most three subfolders (dataviz slots 1-3, light surface: the only ones that stay
/// distinct for every pair in a scatter); all other points grey (LEAF).
pub const GROUP_COLOURS: [&str; 3] = [BLUE, ORANGE, AQUA];

const FONT: &str = "Liberation Sans, Arial, Helvetica, sans-serif";

// --- curve tables -----------------------------------------------------------------------------------------------

/// Arc length along a closed line at each point, px: n + 1 values, the last one the whole length.
pub fn arc_length(line: &[Point]) -> Vec<f64> {
    let n = line.len();
    let mut s = Vec::with_capacity(n + 1);
    s.push(0.0);
    for i in 0..n {
        let (a, b) = (line[i], line[(i + 1) % n]);
        s.push(s[i] + (b[0] - a[0]).hypot(b[1] - a[1]));
    }
    s
}

/// Margin profile as a table `s,h`, one row per blade-line point, both in units of sqrt(A): s = arc length along the
/// blade line from its first point, h = signed distance of the outline from the blade line (outside > 0).
pub fn margin_profile_csv(t: &Trace) -> String {
    let s = arc_length(&t.blade_line);
    let mut out = String::from("s,h\n");
    for (i, h) in t.margin_profile.iter().enumerate() {
        let _ = writeln!(out, "{},{}", s[i] / SQRT_A, h / SQRT_A);
    }
    out
}

/// Error curve as a table `k,e`: e = e(k) / sqrt(A), the error of the k-harmonic fit. Header only if MC was not
/// computed; the table ends where the stop rule ended the fit.
pub fn error_curve_csv(t: &Trace) -> String {
    let mut out = String::from("k,e\n");
    for (i, e) in t.error_curve.iter().enumerate() {
        let _ = writeln!(out, "{},{e}", i + 1);
    }
    out
}

// --- SVG helpers ------------------------------------------------------------------------------------------------

type Xy = [f64; 2];

/// XML-escaped text.
fn esc(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

/// A number for the SVG: one decimal (0.1 unit = 0.025 mm).
fn n(v: f64) -> String {
    let s = format!("{v:.1}");
    if s == "-0.0" { "0.0".into() } else { s }
}

/// Path data of a polyline; closed with `Z` if `close`.
fn path_d(pts: &[Xy], close: bool) -> String {
    let mut d = String::with_capacity(pts.len() * 12);
    for (i, p) in pts.iter().enumerate() {
        let _ = write!(d, "{}{} {}", if i == 0 { "M" } else { "L" }, n(p[0]), n(p[1]));
    }
    if close && !pts.is_empty() {
        d.push('Z');
    }
    d
}

/// Drop points closer than `min` units to the last kept point (drawing only).
fn thin(pts: &[Xy], min: f64) -> Vec<Xy> {
    let mut out: Vec<Xy> = Vec::with_capacity(pts.len());
    for p in pts {
        if out.last().is_none_or(|q| (p[0] - q[0]).hypot(p[1] - q[1]) >= min) {
            out.push(*p);
        }
    }
    out
}

/// Keep, per column of width `bin` units, the first, lowest, highest and last point (a curve over x; drawing only).
fn envelope(pts: &[Xy], bin: f64) -> Vec<Xy> {
    let mut out = Vec::new();
    let mut i = 0;
    while i < pts.len() {
        let b = (pts[i][0] / bin).floor();
        let (start, mut lo, mut hi) = (i, i, i);
        while i < pts.len() && (pts[i][0] / bin).floor() == b {
            if pts[i][1] < pts[lo][1] {
                lo = i;
            }
            if pts[i][1] > pts[hi][1] {
                hi = i;
            }
            i += 1;
        }
        let mut idx = [start, lo, hi, i - 1];
        idx.sort_unstable();
        let mut last = usize::MAX;
        for k in idx {
            if k != last {
                out.push(pts[k]);
                last = k;
            }
        }
    }
    out
}

/// Closed polygon clipped to the rectangle lo..hi (Sutherland-Hodgman); exact for fills inside the rectangle.
fn clip_rect(poly: &[Xy], lo: Xy, hi: Xy) -> Vec<Xy> {
    let mut out = poly.to_vec();
    for axis in 0..2 {
        for (bound, upper) in [(lo[axis], false), (hi[axis], true)] {
            let inside = |p: &Xy| if upper { p[axis] <= bound } else { p[axis] >= bound };
            let cross = |a: &Xy, b: &Xy| {
                let t = (bound - a[axis]) / (b[axis] - a[axis]);
                [a[0] + t * (b[0] - a[0]), a[1] + t * (b[1] - a[1])]
            };
            let input = std::mem::take(&mut out);
            let Some(mut prev) = input.last().copied() else { return out };
            for p in input {
                match (inside(&prev), inside(&p)) {
                    (true, true) => out.push(p),
                    (true, false) => out.push(cross(&prev, &p)),
                    (false, true) => {
                        out.push(cross(&prev, &p));
                        out.push(p);
                    }
                    (false, false) => {}
                }
                prev = p;
            }
        }
    }
    out
}

/// Nice tick values in lo..hi (about `count` steps; step 1, 2 or 5 x 10^m, the one nearest to the range / `count`,
/// smaller if that leaves fewer than three ticks) and the decimals their labels need.
fn ticks(lo: f64, hi: f64, count: usize) -> (Vec<f64>, usize) {
    let raw = (hi - lo) / count as f64;
    if raw.is_nan() || raw <= 0.0 || raw.is_infinite() {
        return (vec![lo], 2);
    }
    let mag = 10f64.powf(raw.log10().floor());
    let steps: Vec<f64> = [0.5, 1.0, 2.0, 5.0, 10.0].iter().map(|m| m * mag).collect();
    let mut i = (1..steps.len()).min_by(|a, b| (steps[*a] / raw).ln().abs().total_cmp(&(steps[*b] / raw).ln().abs())).unwrap();
    loop {
        let step = steps[i];
        let (a, b) = ((lo / step - 1e-9).ceil() as i64, (hi / step + 1e-9).floor() as i64);
        if b - a >= 2 || i == 0 {
            let decimals = (-step.log10().floor()).max(0.0) as usize;
            let v = (a..=b).map(|k| k as f64 * step).map(|v| if v.abs() < step * 1e-9 { 0.0 } else { v }).collect();
            return (v, decimals);
        }
        i -= 1;
    }
}

/// SVG document in units of 0.25 mm.
struct Svg {
    buf: String,
}

impl Svg {
    fn new(w: f64, h: f64) -> Self {
        let mut buf = String::new();
        let _ = writeln!(
            buf,
            "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{}mm\" height=\"{}mm\" viewBox=\"0 0 {w} {h}\" \
             font-family=\"{FONT}\">",
            w / 4.0,
            h / 4.0
        );
        let _ = writeln!(buf, "<rect width=\"{w}\" height=\"{h}\" fill=\"white\"/>");
        Svg { buf }
    }

    fn raw(&mut self, s: &str) {
        self.buf.push_str(s);
        self.buf.push('\n');
    }

    fn path(&mut self, pts: &[Xy], close: bool, attrs: &str) {
        if pts.len() > 1 {
            let _ = writeln!(self.buf, "<path d=\"{}\" {attrs}/>", path_d(pts, close));
        }
    }

    fn line(&mut self, a: Xy, b: Xy, attrs: &str) {
        let _ = writeln!(
            self.buf,
            "<line x1=\"{}\" y1=\"{}\" x2=\"{}\" y2=\"{}\" {attrs}/>",
            n(a[0]),
            n(a[1]),
            n(b[0]),
            n(b[1])
        );
    }

    fn rect(&mut self, x: f64, y: f64, w: f64, h: f64, attrs: &str) {
        let _ = writeln!(
            self.buf,
            "<rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" {attrs}/>",
            n(x),
            n(y),
            n(w),
            n(h)
        );
    }

    fn circle(&mut self, c: Xy, r: f64, attrs: &str) {
        let _ = writeln!(self.buf, "<circle cx=\"{}\" cy=\"{}\" r=\"{r}\" {attrs}/>", n(c[0]), n(c[1]));
    }

    /// Text at (x, y) (baseline); `anchor` start, middle or end.
    fn text(&mut self, x: f64, y: f64, size: f64, anchor: &str, attrs: &str, s: &str) {
        let _ = writeln!(
            self.buf,
            "<text x=\"{}\" y=\"{}\" font-size=\"{size}\" text-anchor=\"{anchor}\" {attrs}>{}</text>",
            n(x),
            n(y),
            esc(s)
        );
    }

    fn finish(mut self) -> String {
        self.buf.push_str("</svg>\n");
        self.buf
    }
}

/// Rough text width in units (sans serif, about 0.5 em per character); for laying out legends.
fn text_width(s: &str, size: f64) -> f64 {
    s.chars().count() as f64 * 0.5 * size
}

/// A plot area: data ranges -> SVG coordinates; y may be logarithmic.
struct Axes {
    x0: f64,
    y0: f64,
    w: f64,
    h: f64,
    xr: [f64; 2],
    yr: [f64; 2],
    log_y: bool,
}

impl Axes {
    fn x(&self, v: f64) -> f64 {
        self.x0 + (v - self.xr[0]) / (self.xr[1] - self.xr[0]) * self.w
    }

    fn y(&self, v: f64) -> f64 {
        let f = |v: f64| if self.log_y { v.log10() } else { v };
        self.y0 + self.h - (f(v) - f(self.yr[0])) / (f(self.yr[1]) - f(self.yr[0])) * self.h
    }

    fn p(&self, x: f64, y: f64) -> Xy {
        [self.x(x), self.y(y)]
    }

    /// Left and bottom axis lines, ticks, tick labels and axis labels (as `style.setup`).
    fn frame(&self, svg: &mut Svg, xt: &(Vec<f64>, usize), yt: &(Vec<f64>, usize), xlabel: &str, ylabel: &str) {
        let (x1, y1) = (self.x0 + self.w, self.y0 + self.h);
        let ax = format!("stroke=\"{AXIS}\" stroke-width=\"0.85\"");
        svg.line([self.x0, y1], [x1, y1], &ax);
        svg.line([self.x0, self.y0], [self.x0, y1], &ax);
        let lab = format!("fill=\"{INK2}\"");
        for v in &xt.0 {
            let x = self.x(*v);
            svg.line([x, y1], [x, y1 + 3.5], &ax);
            svg.text(x, y1 + 12.0, 9.0, "middle", &lab, &format!("{v:.*}", xt.1));
        }
        let mut widest: f64 = 0.0;
        for v in &yt.0 {
            let y = self.y(*v);
            let s = if self.log_y { format!("{v}") } else { format!("{v:.*}", yt.1) };
            widest = widest.max(text_width(&s, 9.0));
            svg.line([self.x0 - 3.5, y], [self.x0, y], &ax);
            svg.text(self.x0 - 5.5, y + 3.2, 9.0, "end", &lab, &s);
        }
        svg.text(self.x0 + self.w / 2.0, y1 + 25.0, 10.0, "middle", &lab, xlabel);
        let (lx, ly) = (self.x0 - 10.0 - widest, self.y0 + self.h / 2.0);
        svg.text(lx, ly, 10.0, "middle", &format!("{lab} transform=\"rotate(-90 {} {})\"", n(lx), n(ly)), ylabel);
    }
}

/// Text with a white halo, so that it stays legible over data.
const HALO: &str = "paint-order=\"stroke\" stroke=\"white\" stroke-width=\"3\" stroke-linejoin=\"round\"";

/// 0.5 contours of every region of a mask, smoothed as `contour::outline`, in the frame of the outline (padded).
fn region_outlines(m: &Mask) -> Vec<Vec<Point>> {
    let p = PAD_CONTOUR;
    let mut a: Grid<f64> = Grid::new(m.rows + 2 * p, m.cols + 2 * p);
    for r in 0..m.rows {
        for c in 0..m.cols {
            if m.get(r, c) {
                a.set(r + p, c + p, 1.0);
            }
        }
    }
    find_contours(&gaussian_filter_2d(&a, SIGMA_PRE), 0.5)
}

/// Mask of the pixels in `a` and not in `b`; None if there are none.
fn difference(a: &Mask, b: &Mask) -> Option<Mask> {
    let data: Vec<bool> = a.data.iter().zip(&b.data).map(|(x, y)| *x && !*y).collect();
    data.iter().any(|v| *v).then(|| Grid::from_vec(a.rows, a.cols, data))
}

fn num_or_na(v: f64, decimals: usize) -> String {
    if v.is_nan() { "NA".into() } else { format!("{v:.decimals$}") }
}

// --- one leaf (Fig. 1 layout) -----------------------------------------------------------------------------------

/// Zoom window of (b): side 10 x the 95th percentile of |h| (100..400 px of the normalised mask), centred on the
/// stretch of that arc length where h(s) wiggles most (largest sum of sqrt(ds^2 + dh^2) - ds, the extra length of the
/// profile). Returns the centre point's index, the centre (x, y) in the picture and the side.
fn zoom_window(t: &Trace, s: &[f64], pic: &dyn Fn(&Point) -> Xy) -> (usize, Xy, f64) {
    let h = &t.margin_profile;
    let n = h.len();
    if n < 2 {
        return (0, t.outline.first().map_or([0.0, 0.0], pic), 100.0);
    }
    let mut abs: Vec<f64> = h.iter().map(|v| v.abs()).collect();
    abs.sort_by(f64::total_cmp);
    let side = (10.0 * abs[(0.95 * (n - 1) as f64) as usize]).clamp(100.0, 400.0);
    // extra length of segment i -> i + 1, and its arc length, over two turns of the closed line
    let ds = |i: usize| s[i % n + 1] - s[i % n];
    let extra = |i: usize| ds(i).hypot(h[(i + 1) % n] - h[i % n]) - ds(i);
    let (mut best, mut best_at) = (f64::NEG_INFINITY, (0, 0));
    let (mut b, mut sum, mut len) = (0, 0.0, 0.0);
    for a in 0..n {
        while b < a + n && len + ds(b) <= side {
            sum += extra(b);
            len += ds(b);
            b += 1;
        }
        if sum > best {
            (best, best_at) = (sum, (a, b));
        }
        if b > a {
            sum -= extra(a);
            len -= ds(a);
        } else {
            b = a + 1;
        }
    }
    let j = ((best_at.0 + best_at.1) / 2) % n;
    (j, pic(&t.outline[j]), side)
}

/// One leaf in the layout of Fig. 1 of the paper: (a) mask, removed petiole, blade line and zoom window; (b) margin
/// band in the zoom window; (c) margin profile h(s) -> EA; (d) whitened blade line and its Fourier fits with 1, 3
/// and 8 harmonics; (e) fit error e(k) against tau, shaded area = MC. `title`: the leaf's name (file, leaf number).
/// 190 x 72 mm (double column).
pub fn leaf_svg(title: &str, m: &Measurement, t: &Trace) -> String {
    let (w, h) = (760.0, 288.0);
    let mut svg = Svg::new(w, h);
    let pad = PAD_CONTOUR as f64;
    // outline frame (row, col; padded) -> picture of the normalised mask (x, y; pixel edges)
    let pic = |p: &Point| -> Xy { [p[1] - pad + 0.5, p[0] - pad + 0.5] };
    let (cols, rows) = (t.norm.cols as f64, t.norm.rows as f64);
    let outline: Vec<Xy> = t.outline.iter().map(pic).collect();
    let blade: Vec<Xy> = t.blade_line.iter().map(pic).collect();

    // header: name and values
    svg.text(10.0, 17.0, 11.0, "start", &format!("font-weight=\"bold\" fill=\"{INK}\""), title);
    let values = format!(
        "EA = {} ({}) · MC = {} ({}) · elongation {} · petiole {} √A · √A native {} px · ShapeComplexity {VERSION}",
        num_or_na(m.ea, 3),
        m.ea_flag.as_str(),
        num_or_na(m.mc, 2),
        m.mc_flag.as_str(),
        num_or_na(m.elongation, 2),
        num_or_na(m.petiole_len, 2),
        num_or_na(m.sqrt_a_native, 0)
    );
    svg.text(10.0, 31.0, 9.0, "start", &format!("fill=\"{INK2}\""), &values);
    let top = 64.0; // top of the panels
    let heads = [
        (10.0, "a", "Mask"),
        (152.0, "b", "Zoom: margin band"),
        (292.0, "c", "Margin profile → EA"),
        (506.0, "d", "Fourier fits"),
        (614.0, "e", "Fit error → MC"),
    ];
    for (x, letter, name) in heads {
        svg.text(x, top - 10.0, 11.0, "start", &format!("font-weight=\"bold\" fill=\"{INK}\""), &format!("({letter})"));
        svg.text(x + 17.0, top - 10.0, 10.0, "start", &format!("fill=\"{INK}\""), name);
    }

    // (a) mask, removed petiole, filled holes, blade line, zoom window
    let (bw, bh) = (130.0, 150.0);
    let sa = (bw / cols).min(bh / rows);
    let (ox, oy) = (10.0 + (bw - cols * sa) / 2.0, top + (bh - rows * sa) / 2.0);
    let a_map = |p: &Xy| -> Xy { [ox + p[0] * sa, oy + p[1] * sa] };
    let a_pts = |pts: &[Xy]| thin(&pts.iter().map(a_map).collect::<Vec<_>>(), 0.3);
    svg.path(&a_pts(&outline), true, &format!("fill=\"{LEAF}\""));
    let region_path = |mask: Option<Mask>| -> String {
        mask.map(|d| {
            region_outlines(&d)
                .iter()
                .map(|c| path_d(&a_pts(&c.iter().map(pic).collect::<Vec<_>>()), true))
                .collect::<String>()
        })
        .unwrap_or_default()
    };
    let holes = region_path(difference(&t.lp, &t.norm));
    if !holes.is_empty() {
        svg.raw(&format!("<path d=\"{holes}\" fill=\"{FILLED}\" fill-rule=\"evenodd\"/>"));
    }
    let petiole = region_path(difference(&t.norm, &t.lp));
    if !petiole.is_empty() {
        svg.raw(&format!("<path d=\"{petiole}\" fill=\"{INK}\" fill-rule=\"evenodd\"/>"));
    }
    svg.path(&a_pts(&blade), true, &format!("fill=\"none\" stroke=\"{BLUE}\" stroke-width=\"0.85\""));
    let s = arc_length(&t.blade_line);
    let (j, zc, side) = zoom_window(t, &s, &pic);
    let r = side / 2.0;
    let (z0, z1) = (a_map(&[zc[0] - r, zc[1] - r]), a_map(&[zc[0] + r, zc[1] + r]));
    svg.rect(z0[0], z0[1], z1[0] - z0[0], z1[1] - z0[1], &format!("fill=\"none\" stroke=\"{INK}\" stroke-width=\"1\""));

    // (b) margin band: leaf outside the blade line orange, background inside it peach, blade grey
    let (bx, by, bs) = (152.0, top + 10.0, 130.0);
    let sb = bs / side;
    let b_map = |p: &Xy| -> Xy { [bx + (p[0] - zc[0] + r) * sb, by + (p[1] - zc[1] + r) * sb] };
    let (lo, hi) = ([bx - 5.0, by - 5.0], [bx + bs + 5.0, by + bs + 5.0]);
    let b_pts = |pts: &[Xy]| thin(&clip_rect(&pts.iter().map(b_map).collect::<Vec<_>>(), lo, hi), 0.3);
    let (bc, bb) = (b_pts(&outline), b_pts(&blade));
    svg.raw(&format!(
        "<defs><clipPath id=\"zoom\"><rect x=\"{}\" y=\"{}\" width=\"{bs}\" height=\"{bs}\"/></clipPath>\
         <clipPath id=\"blade\"><path d=\"{}\"/></clipPath></defs>",
        n(bx),
        n(by),
        path_d(&bb, true)
    ));
    svg.raw("<g clip-path=\"url(#zoom)\">");
    svg.rect(bx, by, bs, bs, "fill=\"white\"");
    svg.path(&bb, true, &format!("fill=\"{PEACH}\""));
    svg.path(&bc, true, &format!("fill=\"{ORANGE}\""));
    svg.path(&bc, true, &format!("fill=\"{GRID}\" clip-path=\"url(#blade)\""));
    svg.path(&bc, true, &format!("fill=\"none\" stroke=\"{INK}\" stroke-width=\"0.7\""));
    svg.path(&bb, true, &format!("fill=\"none\" stroke=\"{BLUE}\" stroke-width=\"2\""));
    svg.raw("</g>");
    svg.rect(bx, by, bs, bs, &format!("fill=\"none\" stroke=\"{INK}\" stroke-width=\"1\""));
    let conn = format!("stroke=\"{MUTED}\" stroke-width=\"0.7\"");
    svg.line([z1[0], z0[1]], [bx, by], &conn);
    svg.line([z1[0], z1[1]], [bx, by + bs], &conn);
    // scale bar: 0.05 sqrt(A), or 0.02 sqrt(A) in small windows
    let bar = if side >= 200.0 { 0.05 } else { 0.02 };
    let (x1, yb) = (bx + bs - 8.0, by + bs - 9.0);
    let x0 = x1 - bar * SQRT_A * sb;
    svg.line([x0, yb], [x1, yb], &format!("stroke=\"{INK}\" stroke-width=\"1.7\""));
    svg.text((x0 + x1) / 2.0, yb - 3.0, 8.0, "middle", &format!("fill=\"{INK}\" {HALO}"), &format!("{bar} √A"));

    // (c) margin profile h(s), filled like the band; grey: the stretch inside the zoom window
    let nh = t.margin_profile.len();
    let s_end = s.last().copied().unwrap_or(1.0) / SQRT_A;
    let hmax = t.margin_profile.iter().fold(0.0f64, |a, v| a.max(v.abs())) / SQRT_A;
    let lim = if hmax > 0.0 { 1.05 * hmax } else { 0.01 };
    let ac = Axes { x0: 334.0, y0: top, w: 152.0, h: 150.0, xr: [0.0, s_end.max(1e-9)], yr: [-lim, lim], log_y: false };
    if nh > 0 {
        let inside = |i: usize| {
            let p = outline[i % nh];
            (p[0] - zc[0]).abs() <= r && (p[1] - zc[1]).abs() <= r
        };
        let (mut a, mut b) = (j as i64, j as i64);
        while a > j as i64 - nh as i64 + 1 && inside(((a - 1).rem_euclid(nh as i64)) as usize) {
            a -= 1;
        }
        while b < a + nh as i64 - 1 && inside(((b + 1) as usize) % nh) {
            b += 1;
        }
        let sv = |i: i64| s[i.rem_euclid(nh as i64) as usize] / SQRT_A;
        let spans = if a < 0 || b >= nh as i64 {
            vec![(sv(a), s_end), (0.0, sv(b))]
        } else {
            vec![(sv(a), sv(b))]
        };
        for (u, v) in &spans {
            svg.rect(ac.x(*u), ac.y0, ac.x(*v) - ac.x(*u), ac.h, &format!("fill=\"{GRID}\""));
        }
        let (u, v) = spans[0];
        svg.text((ac.x(u) + ac.x(v)) / 2.0, ac.y0 + ac.h - 3.0, 8.0, "middle", &format!("fill=\"{INK2}\""), "(b)");
        let mut pts: Vec<Xy> = (0..nh).map(|i| ac.p(s[i] / SQRT_A, t.margin_profile[i] / SQRT_A)).collect();
        pts.push(ac.p(s_end, t.margin_profile[0] / SQRT_A));
        let pts = envelope(&pts, 0.25);
        let y0 = ac.y(0.0);
        let band = |above: bool| -> Vec<Xy> {
            let mut v = vec![[pts[0][0], y0]];
            v.extend(pts.iter().map(|p| [p[0], if above { p[1].min(y0) } else { p[1].max(y0) }]));
            v.push([pts[pts.len() - 1][0], y0]);
            v
        };
        svg.path(&band(true), true, &format!("fill=\"{ORANGE}\""));
        svg.path(&band(false), true, &format!("fill=\"{PEACH}\""));
        svg.path(&pts, false, &format!("fill=\"none\" stroke=\"{INK}\" stroke-width=\"0.35\""));
    }
    svg.line(ac.p(0.0, 0.0), ac.p(s_end, 0.0), &format!("stroke=\"{BLUE}\" stroke-width=\"1.3\""));
    ac.frame(
        &mut svg,
        &ticks(0.0, s_end, 4),
        &ticks(-lim, lim, 4),
        "Arc length along the blade line, s (√A)",
        "Margin profile, h (√A)",
    );
    let ea_text = if m.ea.is_nan() { "EA = NA".to_string() } else { format!("EA = {:.3}", m.ea) };
    svg.text(ac.x0 + ac.w - 2.0, ac.y0 + 10.0, 10.0, "end", &format!("font-weight=\"bold\" fill=\"{INK}\" {HALO}"), &ea_text);

    // (d) whitened blade line and its Fourier fits
    let fits: Vec<Vec<Point>> = K_SHOW.iter().map(|k| efd_reconstruct(&t.white, *k)).collect();
    let all = t.white.iter().chain(fits.iter().flatten());
    let (mut r0, mut r1, mut c0, mut c1) = (f64::INFINITY, f64::NEG_INFINITY, f64::INFINITY, f64::NEG_INFINITY);
    for p in all {
        (r0, r1, c0, c1) = (r0.min(p[0]), r1.max(p[0]), c0.min(p[1]), c1.max(p[1]));
    }
    if r1 > r0 && c1 > c0 {
        let (dw, dh) = (96.0, 150.0);
        let sd = (dw / (c1 - c0)).min(dh / (r1 - r0));
        let (dx, dy) = (506.0 + (dw - (c1 - c0) * sd) / 2.0, top + (dh - (r1 - r0) * sd) / 2.0);
        let d_pts = |pts: &[Point]| thin(&pts.iter().map(|p| [dx + (p[1] - c0) * sd, dy + (p[0] - r0) * sd]).collect::<Vec<_>>(), 0.3);
        let white = d_pts(&t.white);
        svg.path(&white, true, &format!("fill=\"{GRID}\""));
        svg.path(&white, true, &format!("fill=\"none\" stroke=\"{INK}\" stroke-width=\"0.85\""));
        for (fit, colour) in fits.iter().zip(RAMP3) {
            svg.path(&d_pts(fit), true, &format!("fill=\"none\" stroke=\"{colour}\" stroke-width=\"1.4\""));
        }
    }

    // (e) error curve against tau; shaded area = MC
    let e = &t.error_curve;
    if e.is_empty() {
        svg.text(660.0, top + 75.0, 9.0, "middle", &format!("fill=\"{INK2}\""), "MC not computed");
        svg.text(660.0, top + 87.0, 9.0, "middle", &format!("fill=\"{INK2}\""), "(leaf too small in the image)");
    } else {
        let emin = e.iter().fold(TAU, |a, v| a.min(*v));
        let emax = e.iter().fold(TAU, |a, v| a.max(*v));
        let (d0, d1) = ((emin * 0.8).log10().floor(), (emax * 1.25).log10().ceil());
        let kmax = e.len().max(K_AX) as f64;
        let ae = Axes { x0: 662.0, y0: top, w: 84.0, h: 150.0, xr: [0.5, kmax + 0.5], yr: [10f64.powf(d0), 10f64.powf(d1)], log_y: true };
        let ty = ae.y(TAU);
        for (i, v) in e.iter().enumerate().filter(|(_, v)| **v > TAU) {
            let k = (i + 1) as f64;
            let y = ae.y(*v);
            svg.rect(ae.x(k - 0.5), y, ae.x(k + 0.5) - ae.x(k - 0.5), ty - y, &format!("fill=\"{BLUE}\" fill-opacity=\"0.3\""));
        }
        let mut step: Vec<Xy> = Vec::with_capacity(2 * e.len());
        for (i, v) in e.iter().enumerate() {
            let k = (i + 1) as f64;
            step.push(ae.p(k - 0.5, *v));
            step.push(ae.p(k + 0.5, *v));
        }
        svg.path(&step, false, &format!("fill=\"none\" stroke=\"{INK}\" stroke-width=\"1.1\""));
        svg.line([ae.x0, ty], [ae.x0 + ae.w, ty], &format!("stroke=\"{INK}\" stroke-width=\"1\" stroke-dasharray=\"4 2.7\""));
        svg.text(ae.x0 + ae.w + 3.0, ty + 3.5, 10.0, "start", &format!("fill=\"{INK}\""), "τ");
        for (k, colour) in K_SHOW.iter().zip(RAMP3) {
            if let Some(v) = e.get(k - 1) {
                svg.circle(ae.p(*k as f64, *v), 3.0, &format!("fill=\"{colour}\" stroke=\"white\" stroke-width=\"0.6\""));
            }
        }
        let decades: Vec<f64> = (d0 as i32..=d1 as i32).map(|p| 10f64.powi(p)).collect();
        ae.frame(&mut svg, &ticks(0.0, kmax, 4), &(decades, 0), "Harmonics, k", "Error, e(k) (√A)");
        let mc_text = if m.mc.is_nan() { "MC = NA".to_string() } else { format!("MC = {:.2}", m.mc) };
        svg.text(ae.x0 + ae.w - 2.0, ae.y0 + 10.0, 10.0, "end", &format!("font-weight=\"bold\" fill=\"{INK}\" {HALO}"), &mc_text);
    }

    // legend, two rows
    enum Key {
        Patch(&'static str, &'static str),
        Line(&'static str, f64, &'static str),
    }
    let mut row1 = Vec::new();
    if !petiole.is_empty() {
        row1.push(("petiole, removed".to_string(), Key::Patch(INK, "")));
    }
    if !holes.is_empty() {
        row1.push(("holes, filled".to_string(), Key::Patch(FILLED, "")));
    }
    row1.extend([
        ("outline".to_string(), Key::Line(INK, 0.85, "")),
        ("blade line".to_string(), Key::Line(BLUE, 2.0, "")),
        ("margin outside the blade line".to_string(), Key::Patch(ORANGE, "")),
        ("margin inside the blade line".to_string(), Key::Patch(PEACH, "")),
    ]);
    let mut row2 = vec![("whitened blade line".to_string(), Key::Patch(GRID, INK))];
    for (k, colour) in K_SHOW.iter().zip(RAMP3) {
        row2.push((format!("{k} harmonic{}", if *k > 1 { "s" } else { "" }), Key::Line(colour, 1.4, "")));
    }
    row2.push(("tolerance τ = r*/2".to_string(), Key::Line(INK, 1.0, "4 2.7")));
    row2.push(("MC: error above τ".to_string(), Key::Patch(BLUE, "0.3")));
    for (y, items) in [(h - 26.0, row1), (h - 10.0, row2)] {
        let mut x = 10.0;
        for (label, key) in items {
            match key {
                Key::Patch(fill, extra) => {
                    let attrs = match extra {
                        "" => format!("fill=\"{fill}\""),
                        "0.3" => format!("fill=\"{fill}\" fill-opacity=\"0.3\""),
                        stroke => format!("fill=\"{fill}\" stroke=\"{stroke}\" stroke-width=\"0.85\""),
                    };
                    svg.rect(x, y - 7.5, 14.0, 8.0, &attrs);
                }
                Key::Line(colour, width, dash) => {
                    let dash = if dash.is_empty() { String::new() } else { format!(" stroke-dasharray=\"{dash}\"") };
                    svg.line([x, y - 3.5], [x + 14.0, y - 3.5], &format!("stroke=\"{colour}\" stroke-width=\"{width}\"{dash}"));
                }
            }
            svg.text(x + 18.0, y, 9.0, "start", &format!("fill=\"{INK}\""), &label);
            x += 18.0 + text_width(&label, 9.0) + 14.0;
        }
    }
    svg.finish()
}

// --- morphospace ------------------------------------------------------------------------------------------------

/// One leaf in the morphospace.
#[derive(Clone, Debug)]
pub struct MorphPoint {
    pub ea: f64,
    pub mc: f64,
    /// Subfolder of the leaf's file (see [`group_of`]).
    pub group: String,
    /// Flag other than ok or a mask-check warning: drawn hollow.
    pub flagged: bool,
}

/// Subfolder of a file label ("" for files at the top level).
pub fn group_of(label: &str) -> &str {
    label.rsplit_once('/').map_or("", |(dir, _)| dir)
}

/// Colour slot (index into the group colours) of group `g` among the sorted, distinct `groups` (D119): at most three
/// groups: one slot each, by name order; more: slot 0 for the highlighted group. None = grey.
pub fn group_slot(groups: &[String], highlight: Option<&str>, g: &str) -> Option<usize> {
    if groups.len() <= 3 {
        groups.iter().position(|x| x == g)
    } else {
        (highlight == Some(g)).then_some(0)
    }
}

/// Name of a group in a legend.
pub fn group_name(g: &str) -> &str {
    if g.is_empty() { "(top folder)" } else { g }
}

/// EA-MC scatter of many leaves, coloured by subfolder (D119); points without EA or MC are counted, not drawn.
/// 140 x 115 mm.
pub fn morphospace_svg(points: &[MorphPoint], highlight: Option<&str>) -> String {
    let (w, h) = (560.0, 460.0);
    let mut svg = Svg::new(w, h);
    let shown: Vec<&MorphPoint> = points.iter().filter(|p| p.ea.is_finite() && p.mc.is_finite()).collect();
    let missing = points.len() - shown.len();
    let mut groups: Vec<String> = shown.iter().map(|p| p.group.clone()).collect();
    groups.sort();
    groups.dedup();
    let colour = |g: &str| group_slot(&groups, highlight, g).map_or(LEAF, |i| GROUP_COLOURS[i]);

    svg.text(14.0, 20.0, 12.0, "start", &format!("font-weight=\"bold\" fill=\"{INK}\""), "Morphospace: EA and MC");
    let mut note = format!("{} leaves", shown.len());
    if missing > 0 {
        let _ = write!(note, "; {missing} not shown (EA or MC not resolved)");
    }
    let _ = write!(note, " · hollow: flag other than ok or a mask warning · ShapeComplexity {VERSION}");
    svg.text(14.0, 35.0, 9.0, "start", &format!("fill=\"{INK2}\""), &note);
    // legend
    let mut keys: Vec<(String, &str)> = Vec::new();
    if groups.len() <= 3 {
        keys.extend(groups.iter().map(|g| (group_name(g).to_string(), colour(g))));
    } else {
        match highlight.filter(|hl| groups.iter().any(|g| g == hl)) {
            Some(hl) => {
                keys.push((group_name(hl).to_string(), GROUP_COLOURS[0]));
                keys.push((format!("other subfolders ({})", groups.len() - 1), LEAF));
            }
            None => keys.push((format!("{} subfolders", groups.len()), LEAF)),
        }
    }
    let mut x = 14.0;
    for (label, c) in &keys {
        svg.circle([x + 4.0, 50.5], 3.5, &format!("fill=\"{c}\""));
        svg.text(x + 12.0, 54.0, 9.5, "start", &format!("fill=\"{INK}\""), label);
        x += 12.0 + text_width(label, 9.5) + 16.0;
    }

    let range = |v: Vec<f64>| -> [f64; 2] {
        let (lo, hi) = v.iter().fold((f64::INFINITY, f64::NEG_INFINITY), |(a, b), x| (a.min(*x), b.max(*x)));
        if !lo.is_finite() {
            return [0.0, 1.0];
        }
        let d = if hi > lo { 0.05 * (hi - lo) } else { 0.1 * lo.abs().max(0.1) };
        [lo - d, hi + d]
    };
    let ax = Axes {
        x0: 70.0,
        y0: 72.0,
        w: 472.0,
        h: 330.0,
        xr: range(shown.iter().map(|p| p.ea).collect()),
        yr: range(shown.iter().map(|p| p.mc).collect()),
        log_y: false,
    };
    // grey first, coloured on top
    let mut order = shown.clone();
    order.sort_by_key(|p| colour(&p.group) != LEAF);
    for p in order {
        let c = colour(&p.group);
        let attrs = if p.flagged {
            format!("fill=\"white\" stroke=\"{c}\" stroke-width=\"1.3\"")
        } else {
            format!("fill=\"{c}\" stroke=\"white\" stroke-width=\"0.6\"")
        };
        svg.circle(ax.p(p.ea, p.mc), 3.5, &attrs);
    }
    ax.frame(
        &mut svg,
        &ticks(ax.xr[0], ax.xr[1], 5),
        &ticks(ax.yr[0], ax.yr[1], 5),
        "EA (edge amount)",
        "MC (macro-shape complexity)",
    );
    svg.finish()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nice_ticks() {
        assert_eq!(ticks(0.0, 3.9, 4), (vec![0.0, 1.0, 2.0, 3.0], 0));
        assert_eq!(ticks(0.0, 4.2, 4), (vec![0.0, 1.0, 2.0, 3.0, 4.0], 0));
        assert_eq!(ticks(0.0, 64.5, 4).0, vec![0.0, 20.0, 40.0, 60.0]);
        // never a single tick: +-0.045 gives a step of 0.02, +-0.012 one of 0.005
        assert_eq!(ticks(-0.045, 0.045, 4), (vec![-0.04, -0.02, 0.0, 0.02, 0.04], 2));
        assert_eq!(ticks(-0.012, 0.012, 4).0, vec![-0.01, -0.005, 0.0, 0.005, 0.01]);
        assert_eq!(ticks(-0.012, 0.012, 4).1, 3);
    }

    #[test]
    fn clipping_keeps_the_inside() {
        // a square larger than the window: clipped to the window
        let sq = [[-10.0, -10.0], [10.0, -10.0], [10.0, 10.0], [-10.0, 10.0]];
        let c = clip_rect(&sq, [-1.0, -1.0], [1.0, 1.0]);
        assert_eq!(c.len(), 4);
        assert!(c.iter().all(|p| p[0].abs() == 1.0 && p[1].abs() == 1.0));
        // a square outside the window: nothing
        let far = [[5.0, 5.0], [6.0, 5.0], [6.0, 6.0]];
        assert!(clip_rect(&far, [-1.0, -1.0], [1.0, 1.0]).is_empty());
    }

    #[test]
    fn envelope_keeps_extremes() {
        let pts: Vec<Xy> = (0..100).map(|i| [i as f64 * 0.01, if i == 37 { 5.0 } else { 0.0 }]).collect();
        let e = envelope(&pts, 0.25);
        assert!(e.len() < 20);
        assert!(e.contains(&[0.37, 5.0]));
    }

    #[test]
    fn group_colours() {
        let g = |v: &[&str]| v.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        let three = g(&["", "acer", "quercus"]);
        assert_eq!(group_slot(&three, None, "quercus"), Some(2));
        assert_eq!(group_slot(&three, Some("acer"), "quercus"), Some(2));
        let four = g(&["a", "b", "c", "d"]);
        assert_eq!(group_slot(&four, None, "a"), None);
        assert_eq!(group_slot(&four, Some("c"), "c"), Some(0));
        assert_eq!(group_slot(&four, Some("c"), "d"), None);
        assert_eq!(group_of("oak/x/leaf.png"), "oak/x");
        assert_eq!(group_of("leaf.png"), "");
    }
}
