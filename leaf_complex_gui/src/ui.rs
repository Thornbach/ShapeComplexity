// leaf_complex_gui/src/ui.rs
// Contains all UI rendering functions. The panels follow Fig. 1 of the paper: (a) mask, petiole, blade line;
// (b) margin band; (c) margin profile h(s) -> EA; (d) Fourier fits of the whitened blade line; (e) e(k) -> MC.

use eframe::egui;
use egui_plot::{Bar, BarChart, HLine, Line, LineStyle, Plot, PlotPoints, Points, Polygon, VLine};
use std::sync::{Arc, Mutex};

use shapecomplexity::batch::VERSION;
use shapecomplexity::figure::{arc_length, group_name, group_slot};
use shapecomplexity::v2::contour::Point;
use shapecomplexity::v2::metrics::efd_reconstruct;
use shapecomplexity::v2::{K_MAX, SQRT_A, TAU};
use shapecomplexity::MaskSource;

use crate::analysis::{input_texture, mask_texture};
use crate::state::{xy, AnalysisStatus, AppState, CentralView, MorphEntry, ViewMode};

// colours of Fig. 1 (analysis/figures/style.py)
const INK: egui::Color32 = egui::Color32::from_rgb(11, 11, 11);
const BLUE: egui::Color32 = egui::Color32::from_rgb(42, 120, 214);
const ORANGE: egui::Color32 = egui::Color32::from_rgb(235, 104, 52);
const PEACH: egui::Color32 = egui::Color32::from_rgb(247, 195, 168);
const GRID: egui::Color32 = egui::Color32::from_rgb(225, 224, 217);
/// Outline over the input image (black would vanish on a dark leaf).
const OVERLAY_OUTLINE: egui::Color32 = egui::Color32::from_rgb(220, 0, 170);
const RED_COLOR: egui::Color32 = egui::Color32::from_rgb(255, 0, 0);
const WARN_COLOR: egui::Color32 = egui::Color32::from_rgb(230, 140, 0);

fn source_label(s: MaskSource) -> &'static str {
    match s {
        MaskSource::Auto => "Mask: auto",
        MaskSource::Alpha => "Mask: alpha (opaque = leaf)",
        MaskSource::Dark => "Mask: dark (dark = leaf)",
    }
}

/// The main action button: large and in the accent colour, so that it is easy to find.
fn analyze_button(text: &str) -> egui::Button<'static> {
    egui::Button::new(egui::RichText::new(text).size(16.0).strong().color(egui::Color32::WHITE))
        .fill(BLUE)
        .min_size(egui::vec2(0.0, 30.0))
}

pub fn render_image_view(
    ui: &mut egui::Ui,
    state: &Arc<Mutex<AppState>>,
    ctx: &egui::Context,
    analyze_clicked: &mut bool,
    batch_clicked: &mut bool,
) {
    let filename = {
        let g = state.lock().unwrap();
        g.current_image().map(|img| img.label.clone()).unwrap_or_else(|| "No image selected".to_string())
    };

    ui.heading(format!("🖼️ {}", filename));
    ui.separator();

    ui.horizontal_wrapped(|ui| {
        let (busy, mut source) = {
            let g = state.lock().unwrap();
            (g.analysis_in_progress || g.batch_processing, g.mask_source)
        };
        ui.add_enabled_ui(!busy, |ui| {
            egui::ComboBox::from_id_salt("mask_source")
                .selected_text(source_label(source))
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut source, MaskSource::Auto, "Auto: alpha if the image has transparent pixels, else dark");
                    ui.selectable_value(&mut source, MaskSource::Alpha, "Alpha: opaque pixels of a cut-out are leaf");
                    ui.selectable_value(&mut source, MaskSource::Dark, "Dark: dark pixels of a binary image are leaf");
                })
                .response
                .on_hover_text("Which pixels are leaf. Auto decides per image; results are cleared when this changes.");
        });
        state.lock().unwrap().set_mask_source(source);

        ui.separator();

        let (has_current, selected_count) = {
            let g = state.lock().unwrap();
            (g.current_image().is_some(), g.selected_count())
        };
        let ready = !busy;
        ui.add_enabled_ui(has_current && ready, |ui| {
            if ui.add(analyze_button("▶ Analyze")).on_hover_text("Analyse the image on screen").clicked() {
                *analyze_clicked = true;
            }
        });
        let batch_label = if selected_count > 0 {
            format!("⏩ Analyze Selected ({})", selected_count)
        } else {
            "⏩ Analyze All".to_string()
        };
        ui.add_enabled_ui(ready, |ui| {
            if ui.button(batch_label).clicked() {
                *batch_clicked = true;
            }
        });

        ui.separator();

        let mut g = state.lock().unwrap();
        let mut all = g.all_leaves;
        ui.add_enabled_ui(!busy, |ui| {
            ui.checkbox(&mut all, "All leaves").on_hover_text(
                "Measure every region with √A ≥ 250 px as a leaf (one row each). Off: the largest region only. \
                 Results are cleared when this changes.",
            );
        });
        g.set_all_leaves(all);
        ui.label("px/mm:");
        let field = ui.add(egui::TextEdit::singleline(&mut g.px_per_mm_text).desired_width(50.0)).on_hover_text(
            "Image scale in pixels per mm (optional). Adds the leaf area (whole leaf, petiole included) to the \
             results and the column area_mm2 to the export.",
        );
        if field.changed() {
            g.px_per_mm = g.px_per_mm_text.trim().parse::<f64>().ok().filter(|v| *v > 0.0 && v.is_finite());
        }
        if !g.px_per_mm_text.trim().is_empty() && g.px_per_mm.is_none() {
            ui.colored_label(WARN_COLOR, "not a positive number");
        }
    });

    ui.horizontal_wrapped(|ui| {
        let mut g = state.lock().unwrap();
        let mode = g.view_mode;
        ui.selectable_value(&mut g.view_mode, ViewMode::Mask, "Normalised mask")
            .on_hover_text("The mask as measured: largest region, scaled to √A = 1000 px");
        ui.selectable_value(&mut g.view_mode, ViewMode::Input, "Input image")
            .on_hover_text("The input image with the normalised mask placed over it (mask check)");
        if g.view_mode != mode {
            g.reset_view();
        }
        ui.separator();
        ui.checkbox(&mut g.show_outline, "Outline");
        ui.checkbox(&mut g.show_blade_line, "Blade line");
        ui.checkbox(&mut g.show_band, "Margin band")
            .on_hover_text("Orange: leaf outside the blade line; peach: background inside it (Fig. 1 b)");
        if ui.button("Reset View").clicked() {
            g.reset_view();
        }
        ui.label(format!("Zoom: {:.0}%", g.zoom_level * 100.0));
        let n = g.current_leaves().len();
        if n > 1 {
            ui.separator();
            ui.label("Leaf:");
            let mut pick = g.current_leaf;
            for i in 0..n {
                ui.selectable_value(&mut pick, i, format!("{}", i + 1));
            }
            g.select_leaf(pick);
        }
    });

    ui.separator();

    let response = ui.allocate_rect(ui.available_rect_before_wrap(), egui::Sense::click_and_drag());
    let rect = response.rect;
    let painter = ui.painter_at(rect);

    let mut g = state.lock().unwrap();
    let current_path = g.current_image().map(|i| i.path.clone());

    // textures for the image on screen only
    if let (Some(path), Some(result)) = (current_path.as_ref(), g.current_result()) {
        let (band, leaf) = (g.show_band, g.current_leaf);
        if g.mask_texture.as_ref().is_none_or(|(p, l, b, _)| p != path || *l != leaf || *b != band) {
            let tex = mask_texture(result, band, format!("{}_mask_{leaf}", path.display()), ctx);
            g.mask_texture = Some((path.clone(), leaf, band, tex));
        }
        if g.view_mode == ViewMode::Input && g.input_texture.as_ref().is_none_or(|(p, _)| p != path) {
            g.input_texture = Some((path.clone(), input_texture(path, ctx)));
        }
    }

    let mut visible_runs = Vec::new();
    match (g.current_result(), g.mask_texture.as_ref()) {
        (Some(result), Some((.., tex))) => {
            let mask_size = tex.size_vec2();
            let input = match (&g.input_texture, &result.placement) {
                (Some((p, Ok((itex, size)))), Some(pl)) if g.view_mode == ViewMode::Input && Some(p) == current_path.as_ref() => {
                    Some((itex, egui::vec2(size[0], size[1]), pl))
                }
                _ => None,
            };
            // content coordinates: mask picture (mask view) or input image (input view), in pixel-edge units
            let to_content = |x: f32, y: f32| -> egui::Pos2 {
                match input {
                    Some((_, _, pl)) => {
                        let (a, b) = pl.to_input(x as f64, y as f64);
                        egui::pos2(a as f32, b as f32)
                    }
                    None => egui::pos2(x, y),
                }
            };
            let content_size = input.map_or(mask_size, |(_, size, _)| size);
            // fit the picture to the panel, then apply the zoom
            let fit = (rect.width() / content_size.x).min(rect.height() / content_size.y);
            let scale = fit * g.zoom_level;
            let img_rect = egui::Rect::from_center_size(rect.center() + g.pan_offset, content_size * scale);
            let to_screen = |c: egui::Pos2| img_rect.min + c.to_vec2() * scale;
            let uv = egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0));
            painter.rect_filled(img_rect, 0.0, egui::Color32::WHITE);
            let mask_rect = egui::Rect::from_min_max(
                to_screen(to_content(0.0, 0.0)),
                to_screen(to_content(mask_size.x, mask_size.y)),
            );
            match input {
                Some((itex, _, _)) => {
                    painter.image(itex.id(), img_rect, uv, egui::Color32::WHITE);
                    painter.image(tex.id(), mask_rect, uv, egui::Color32::from_white_alpha(140));
                }
                None => {
                    painter.image(tex.id(), mask_rect, uv, egui::Color32::WHITE);
                }
            }
            let pt = |p: &Point| {
                let [x, y] = xy(p);
                to_screen(to_content(x, y))
            };
            let width = (1.5 * g.zoom_level).clamp(1.0, 3.0);
            if g.show_outline {
                let colour = if input.is_some() { OVERLAY_OUTLINE } else { INK };
                let pts: Vec<egui::Pos2> = result.outline.iter().map(pt).collect();
                painter.add(egui::Shape::closed_line(pts, egui::Stroke::new(0.6 * width, colour)));
            }
            if g.show_blade_line {
                let pts: Vec<egui::Pos2> = result.blade_line.iter().map(pt).collect();
                painter.add(egui::Shape::closed_line(pts, egui::Stroke::new(width, BLUE)));
            }
            if let Some(i) = g.selected_point {
                if let (Some(c), Some(b)) = (result.outline.get(i), result.blade_line.get(i)) {
                    let (c, b) = (pt(c), pt(b));
                    painter.line_segment([b, c], egui::Stroke::new(2.0, RED_COLOR));
                    painter.circle_filled(c, 4.0, RED_COLOR);
                    painter.circle_stroke(b, 4.0, egui::Stroke::new(2.0, RED_COLOR));
                }
            }
            // leaf numbers at the region centroids (all leaves measured; input view)
            if input.is_some() {
                for (i, leaf) in g.current_leaves().iter().enumerate().filter(|(_, l)| !l.centroid[0].is_nan()) {
                    let c = to_screen(egui::pos2(leaf.centroid[0] as f32, leaf.centroid[1] as f32));
                    let fill = if i == g.current_leaf { BLUE } else { INK };
                    painter.circle(c, 11.0, fill, egui::Stroke::new(1.5, egui::Color32::WHITE));
                    painter.text(c, egui::Align2::CENTER_CENTER, leaf.leaf.to_string(), egui::FontId::proportional(13.0), egui::Color32::WHITE);
                }
            }
            // outline points on screen, for the grey stretch in the h(s) plot; gaps of a few points are bridged
            let shown: Vec<bool> = result.outline.iter().map(|p| rect.contains(pt(p))).collect();
            if shown.iter().any(|v| !v) {
                for (i, v) in shown.iter().enumerate() {
                    if !v {
                        continue;
                    }
                    match visible_runs.last_mut() {
                        Some((_, last)) if i - *last <= 5 => *last = i,
                        _ => visible_runs.push((i, i)),
                    }
                }
            }
            let legend = match (input.is_some(), g.view_mode, g.show_band) {
                (true, _, _) => "Input image; normalised mask placed over it (half transparent), outline magenta.".to_string(),
                (false, ViewMode::Input, _) => match &g.input_texture {
                    Some((_, Err(e))) => format!("Input image could not be shown: {e}"),
                    _ => "Input image not available.".to_string(),
                },
                (false, _, true) => "Normalised mask. Orange: leaf outside the blade line; peach: background inside it; black: removed petiole.".to_string(),
                (false, _, false) => "Normalised mask (√A = 1000 px). Grey: leaf; black: removed petiole; light grey: filled holes.".to_string(),
            };
            let galley = painter.layout_no_wrap(legend, egui::FontId::proportional(12.0), ui.visuals().text_color());
            let pos = rect.left_bottom() + egui::vec2(8.0, -8.0 - galley.size().y);
            painter.rect_filled(egui::Rect::from_min_size(pos, galley.size()).expand(4.0), 3.0, ui.visuals().extreme_bg_color);
            painter.galley(pos, galley, ui.visuals().text_color());
        }
        _ => {
            let text = match current_path.as_ref() {
                None => "Open a workspace folder to begin".to_string(),
                Some(p) => match g.errors.get(p) {
                    Some(e) => format!("Analysis failed:\n{e}"),
                    None => String::new(),
                },
            };
            if text.is_empty() {
                // not analysed yet: a large button in the middle of the view
                let busy = g.analysis_in_progress || g.batch_processing;
                let button = egui::Rect::from_center_size(rect.center(), egui::vec2(260.0, 48.0));
                let clicked = ui
                    .put(button, analyze_button("▶ Analyze this image").min_size(button.size()))
                    .on_hover_text("Measure EA and MC of this image (or use ⏩ Analyze All in the toolbar)")
                    .clicked();
                if clicked && !busy {
                    *analyze_clicked = true;
                }
            } else {
                painter.text(
                    rect.center(),
                    egui::Align2::CENTER_CENTER,
                    text,
                    egui::FontId::proportional(16.0),
                    ui.visuals().text_color(),
                );
            }
        }
    }
    g.visible_runs = visible_runs;

    if response.dragged() {
        g.pan_offset += response.drag_delta();
    }
    if response.hovered() {
        let scroll = ui.input(|i| i.raw_scroll_delta.y);
        if scroll != 0.0 {
            g.zoom_level = (g.zoom_level * (1.0 + scroll * 0.001)).clamp(0.2, 20.0);
        }
    }
}

pub fn render_results_panel(ui: &mut egui::Ui, state: &Arc<Mutex<AppState>>) {
    ui.heading("📋 Results");
    let g = state.lock().unwrap();
    let Some(r) = g.current_result() else {
        ui.label("No analysis data available");
        return;
    };
    let m = &r.measurement;
    let num = |v: f64, digits: usize| if v.is_nan() { "NA".to_string() } else { format!("{v:.digits$}") };
    egui::Grid::new("results_grid").num_columns(2).spacing([20.0, 4.0]).striped(true).show(ui, |ui| {
        let mut row = |name: &str, value: String, hover: &str| {
            ui.label(name).on_hover_text(hover);
            ui.label(value);
            ui.end_row();
        };
        if g.all_leaves {
            row(
                "Leaf",
                format!("{} of {} (centre x {:.0}, y {:.0} px)", r.leaf, g.current_leaves().len(), r.centroid[0], r.centroid[1]),
                "Leaf regions: √A ≥ 250 px, numbered in raster order (D116)",
            );
        }
        row("EA (edge amount)", num(m.ea, 4), "Extra length of the margin profile h(s) per unit blade-line length");
        row("MC (macro-shape complexity)", num(m.mc, 3), "Sum of the positive values of ln(e(k)/τ) (shaded in the plot below)");
        row("Elongation", num(m.elongation, 3), "Major / minor axis ratio of the blade");
        row("Petiole length", num(m.petiole_len, 3), "Fraction of √A; 0 = no petiole found");
        row("√A native", format!("{} px", num(m.sqrt_a_native, 1)), "Square root of the leaf area before normalisation");
        if let Some(k) = g.px_per_mm {
            row("Leaf area", format!("{:.2} mm²", r.px as f64 / (k * k)), "Pixels of the measured region / (px per mm)²; petiole included");
        }
        row("EA flag", m.ea_flag.as_str().to_string(), "From √A native: < 250 px not resolved, < 500 px below recommended");
        row("MC flag", m.mc_flag.as_str().to_string(), "From √A native: < 250 px not resolved");
        row(
            "Dropped area",
            format!("{:.2} %", 100.0 * r.check.dropped_share),
            "Share of the leaf pixels outside the largest region (not measured). Warning above 1 %.",
        );
        if let Some(gs) = r.check.grey_share {
            row(
                "Grey pixels",
                format!("{:.1} %", 100.0 * gs),
                "Dark input: grey pixels (32–223) among the pixels that are not white. Warning above 50 %.",
            );
        }
        let how = if g.mask_source == MaskSource::Auto { " (auto)" } else { "" };
        row("Mask source", format!("{}{how}", r.source.as_str()), "Which pixels were read as leaf");
        row("Version", VERSION.to_string(), "ShapeComplexity version");
    });
    for c in r.concerns() {
        ui.colored_label(WARN_COLOR, format!("⚠ {c}"));
    }
}

pub fn render_margin_plot(ui: &mut egui::Ui, state: &Arc<Mutex<AppState>>) {
    ui.heading("📊 (c) Margin profile h(s) and EA");
    let (h, s, ea, selected, runs) = {
        let g = state.lock().unwrap();
        match g.current_result() {
            Some(r) => (
                r.margin_profile.iter().map(|v| v / SQRT_A).collect::<Vec<f64>>(),
                arc_length(&r.blade_line).iter().map(|v| v / SQRT_A).collect::<Vec<f64>>(),
                r.measurement.ea,
                g.selected_point,
                g.visible_runs.clone(),
            ),
            None => {
                ui.label("No analysis data available");
                return;
            }
        }
    };
    ui.label("Distance of the outline from the blade line. Orange: outline outside the blade line; peach: inside. EA measures how much this curve wiggles. Grey: the stretch on screen in the image view. Click to show a point on the leaf.");
    let lim = h.iter().fold(0.0f64, |a, v| a.max(v.abs())) * 1.05;
    let bars: Vec<Bar> = h
        .iter()
        .enumerate()
        .map(|(i, v)| {
            let ds = s[i + 1] - s[i];
            Bar::new(s[i] + ds / 2.0, *v).width(ds).fill(if *v > 0.0 { ORANGE } else { PEACH }).stroke(egui::Stroke::NONE)
        })
        .collect();
    let ink = ui.visuals().text_color();
    let line = Line::new(h.iter().enumerate().map(|(i, v)| [s[i], *v]).collect::<PlotPoints>()).color(ink).width(0.5);
    let response = Plot::new("margin_plot")
        .allow_scroll(false)
        .allow_zoom(false)
        .height(200.0)
        .x_axis_label("s, arc length along the blade line (√A)")
        .y_axis_label("h (√A)")
        .allow_drag(false)
        .show(ui, |p| {
            for (a, b) in &runs {
                let (x0, x1) = (s[*a], s[*b + 1]);
                p.polygon(
                    Polygon::new(vec![[x0, -lim], [x1, -lim], [x1, lim], [x0, lim]])
                        .fill_color(GRID.linear_multiply(0.25))
                        .stroke(egui::Stroke::NONE),
                );
            }
            p.bar_chart(BarChart::new(bars).allow_hover(false));
            p.line(line);
            p.hline(HLine::new(0.0).color(BLUE).width(1.2).name("blade line"));
            if let Some(&v) = selected.and_then(|i| h.get(i)) {
                p.points(Points::new(vec![[s[selected.unwrap()], v]]).color(RED_COLOR).radius(5.0));
            }
        });
    ui.label(egui::RichText::new(format!("EA = {}", if ea.is_nan() { "NA".into() } else { format!("{ea:.3}") })).strong());
    if response.response.clicked() {
        if let Some(pos) = response.response.interact_pointer_pos() {
            let x = response.transform.value_from_position(pos).x;
            let i = s.partition_point(|v| *v <= x);
            if i >= 1 && i <= h.len() {
                state.lock().unwrap().selected_point = Some(i - 1);
            }
        }
    }
}

pub fn render_fit_plot(ui: &mut egui::Ui, state: &Arc<Mutex<AppState>>) {
    ui.heading("📊 (d) Fourier fits of the whitened blade line");
    let mut g = state.lock().unwrap();
    let Some(path) = g.current_image().map(|i| i.path.clone()) else {
        ui.label("No analysis data available");
        return;
    };
    let Some(r) = g.current_result() else {
        ui.label("No analysis data available");
        return;
    };
    let white = r.white.clone();
    let e = r.error_curve.get(g.efd_k - 1).copied();
    let n_e = r.error_curve.len();
    ui.label("The blade line, stretched to remove elongation (whitened), and its elliptic Fourier fit with k harmonics. MC counts the harmonics a fit needs to come within τ of the line.");
    let (mut k, leaf) = (g.efd_k, g.current_leaf);
    ui.add(egui::Slider::new(&mut k, 1..=K_MAX).text("harmonics k"));
    g.efd_k = k;
    if g.efd_fit.as_ref().is_none_or(|(p, l, kk, _)| *p != path || *l != leaf || *kk != k) {
        g.efd_fit = Some((path, leaf, k, efd_reconstruct(&white, k)));
    }
    let fit = g.efd_fit.as_ref().map(|(.., f)| f.clone()).unwrap_or_default();
    drop(g);
    // (row, col) -> (x, y) with y up
    let closed = |pts: &[[f64; 2]]| -> PlotPoints {
        pts.iter().chain(pts.first()).map(|p| [p[1] / SQRT_A, -p[0] / SQRT_A]).collect()
    };
    let ink = ui.visuals().text_color();
    Plot::new("fit_plot")
        .allow_scroll(false)
        .allow_zoom(false)
        .height(260.0)
        .data_aspect(1.0)
        .show_axes(false)
        .show_grid(false)
        .allow_drag(false)
        .show(ui, |p| {
            p.line(Line::new(closed(&white)).color(ink).width(0.8).name("whitened blade line"));
            p.line(Line::new(closed(&fit)).color(BLUE).width(1.5).name(format!("{k} harmonics")));
        });
    ui.label(match e {
        Some(e) => format!("k = {k}: e(k) = {e:.4} √A, ln(e(k)/τ) = {:.2}", (e / TAU).ln()),
        None if n_e == 0 => format!("k = {k}: e(k) not computed (MC not resolved)"),
        None => format!("k = {k}: e(k) not computed (the fit was within the stop rule after {n_e} harmonics)"),
    });
}

pub fn render_error_plot(ui: &mut egui::Ui, state: &Arc<Mutex<AppState>>) {
    ui.heading("📊 (e) Fit error e(k) and MC");
    let (data, mc, k_sel) = {
        let g = state.lock().unwrap();
        match g.current_result() {
            Some(r) => (r.error_curve.clone(), r.measurement.mc, g.efd_k),
            None => {
                ui.label("No analysis data available");
                return;
            }
        }
    };
    if data.is_empty() {
        ui.label("MC was not computed (leaf too small in the image).");
        return;
    }
    ui.label(format!(
        "Error of the k-harmonic fit, as ln(e(k)/τ) with τ = {TAU} √A. MC is the shaded area: the sum of the values above 0. Blue line: k of the fit above."
    ));
    let pts: Vec<[f64; 2]> = data.iter().enumerate().map(|(k, e)| [(k + 1) as f64, (e / TAU).ln()]).collect();
    let bars: Vec<Bar> = pts
        .iter()
        .filter(|p| p[1] > 0.0)
        .map(|p| Bar::new(p[0], p[1]).width(1.0).fill(BLUE.linear_multiply(0.3)).stroke(egui::Stroke::NONE))
        .collect();
    let ink = ui.visuals().text_color();
    Plot::new("error_plot")
        .allow_scroll(false)
        .allow_zoom(false)
        .height(200.0)
        .x_axis_label("k (harmonics)")
        .y_axis_label("ln(e(k)/τ)")
        .allow_drag(false)
        .show(ui, |p| {
            p.bar_chart(BarChart::new(bars).allow_hover(false).name("MC (area)"));
            p.hline(HLine::new(0.0).color(ink).style(LineStyle::dashed_dense()).name("τ"));
            p.line(Line::new(PlotPoints::from(pts.clone())).color(ink).width(1.0));
            p.points(Points::new(pts.clone()).color(ink).radius(2.5).name("ln(e(k)/τ)"));
            p.vline(VLine::new(k_sel as f64).color(BLUE).width(1.2));
            if let Some(q) = pts.get(k_sel - 1) {
                p.points(Points::new(vec![*q]).color(BLUE).radius(5.0));
            }
        });
    ui.label(egui::RichText::new(format!("MC = {}", if mc.is_nan() { "NA".into() } else { format!("{mc:.2}") })).strong());
}

/// Categorical colours for at most three subfolders (dataviz palette, dark-surface steps, slots 1-3: the only ones
/// that stay distinct for every pair in a scatter) and the grey for all other points.
const GROUP_COLORS: [egui::Color32; 3] = [
    egui::Color32::from_rgb(57, 135, 229),
    egui::Color32::from_rgb(217, 89, 38),
    egui::Color32::from_rgb(25, 158, 112),
];
const OTHER_COLOR: egui::Color32 = egui::Color32::from_rgb(154, 152, 144);

pub fn render_morphospace(ui: &mut egui::Ui, state: &Arc<Mutex<AppState>>) {
    ui.heading("📊 Morphospace: EA and MC of all analysed leaves");
    let (points, missing, current, mut highlight) = {
        let g = state.lock().unwrap();
        let all = g.morph_entries();
        let n = all.len();
        let pts: Vec<MorphEntry> = all.into_iter().filter(|q| q.point.ea.is_finite() && q.point.mc.is_finite()).collect();
        let missing = n - pts.len();
        (pts, missing, g.current_image_index.map(|i| (i, g.current_leaf)), g.highlight.clone())
    };
    if points.is_empty() {
        ui.label("No analysed leaves with both EA and MC yet. Analyse some images first.");
        return;
    }
    let mut groups: Vec<String> = points.iter().map(|p| p.point.group.clone()).collect();
    groups.sort();
    groups.dedup();
    let name = |g: &str| group_name(g).to_string();
    ui.horizontal_wrapped(|ui| {
        if groups.len() <= 3 {
            for (g, c) in groups.iter().zip(GROUP_COLORS) {
                ui.colored_label(c, "⏺");
                ui.label(name(g));
            }
        } else {
            ui.label("Highlight:");
            egui::ComboBox::from_id_salt("highlight")
                .selected_text(highlight.as_deref().map_or("none".to_string(), name))
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut highlight, None, "none");
                    for g in &groups {
                        ui.selectable_value(&mut highlight, Some(g.clone()), name(g));
                    }
                });
            ui.label(format!("({} subfolders; grey: the others)", groups.len()));
        }
        ui.separator();
        ui.label("Hollow: flag other than ok or a mask warning. Ring: leaf on screen. Click a point to open the leaf.");
    });
    state.lock().unwrap().highlight = highlight.clone();
    // colour follows the subfolder: by name order if there are at most three, else one highlighted subfolder (D119)
    let colour = |g: &str| group_slot(&groups, highlight.as_deref(), g).map_or(OTHER_COLOR, |i| GROUP_COLORS[i]);
    if missing > 0 {
        ui.label(format!("{missing} leaves not shown (EA or MC not resolved)."));
    }
    let response = Plot::new("morphospace")
        .height(ui.available_height().max(300.0) - 10.0)
        .x_axis_label("EA (edge amount)")
        .y_axis_label("MC (macro-shape complexity)")
        .show_x(false)
        .show_y(false)
        .show(ui, |p| {
            // grey first, coloured on top; flagged leaves hollow
            let mut order: Vec<&MorphEntry> = points.iter().collect();
            order.sort_by_key(|q| colour(&q.point.group) != OTHER_COLOR);
            for q in order {
                p.points(
                    Points::new(vec![[q.point.ea, q.point.mc]])
                        .color(colour(&q.point.group))
                        .radius(4.0)
                        .filled(!q.point.flagged)
                        .allow_hover(false),
                );
            }
            if let Some(q) = points.iter().find(|q| Some((q.image, q.leaf)) == current) {
                p.points(Points::new(vec![[q.point.ea, q.point.mc]]).color(egui::Color32::WHITE).radius(8.0).filled(false));
            }
        });
    // nearest point under the pointer: tooltip and click
    let r = &response.response;
    let near = r.hover_pos().and_then(|pos| {
        points
            .iter()
            .map(|q| {
                let at = egui_plot::PlotPoint::new(q.point.ea, q.point.mc);
                (q, response.transform.position_from_point(&at).distance(pos))
            })
            .filter(|(_, d)| *d < 10.0)
            .min_by(|a, b| a.1.total_cmp(&b.1))
            .map(|(q, _)| q)
    });
    if let Some(q) = near {
        r.clone().on_hover_text_at_pointer(format!("{}\nEA {:.4}, MC {:.2}", q.label, q.point.ea, q.point.mc));
        if r.clicked() {
            let mut g = state.lock().unwrap();
            g.open_leaf(q.image, q.leaf);
            g.central_view = CentralView::Leaf;
        }
    }
}

pub fn render_analysis_panel(ui: &mut egui::Ui, state: &Arc<Mutex<AppState>>) {
    {
        let mut g = state.lock().unwrap();
        ui.horizontal(|ui| {
            ui.selectable_value(&mut g.central_view, CentralView::Leaf, "Leaf");
            ui.selectable_value(&mut g.central_view, CentralView::Morphospace, "Morphospace");
        });
    }
    ui.separator();
    if state.lock().unwrap().central_view == CentralView::Morphospace {
        render_morphospace(ui, state);
        return;
    }
    egui::ScrollArea::vertical().show(ui, |ui| {
        render_results_panel(ui, state);
        ui.add_space(10.0);
        ui.separator();
        render_margin_plot(ui, state);
        ui.add_space(10.0);
        ui.separator();
        render_fit_plot(ui, state);
        ui.add_space(10.0);
        ui.separator();
        render_error_plot(ui, state);
    });
}

pub fn render_thumbnail_strip(ui: &mut egui::Ui, state: &Arc<Mutex<AppState>>) {
    let mut selected_idx: Option<usize> = None;
    let mut selection_changes: Vec<(usize, bool)> = Vec::new();

    ui.horizontal(|ui| {
        let state_guard = state.lock().unwrap();
        ui.heading("📂 Workspace Images");
        ui.label(format!("({} images, {} selected)", state_guard.images.len(), state_guard.selected_count()));
        ui.separator();

        let select_all_clicked = ui.button("✓ Select All").clicked();
        let deselect_all_clicked = ui.button("✗ Deselect All").clicked();
        drop(state_guard);

        if select_all_clicked {
            state.lock().unwrap().select_all();
        }
        if deselect_all_clicked {
            state.lock().unwrap().deselect_all();
        }
    });

    ui.separator();

    egui::ScrollArea::horizontal().auto_shrink([false, true]).show(ui, |ui| {
        let state_guard = state.lock().unwrap();

        ui.horizontal(|ui| {
            let current_idx = state_guard.current_image_index;

            for (idx, img_info) in state_guard.images.iter().enumerate() {
                let is_current = current_idx == Some(idx);

                let frame = if is_current {
                    egui::Frame::default()
                        .fill(ui.visuals().selection.bg_fill)
                        .stroke(ui.visuals().selection.stroke)
                        .rounding(4.0)
                        .inner_margin(4.0)
                } else {
                    egui::Frame::default()
                        .fill(ui.visuals().window_fill())
                        .stroke(ui.visuals().widgets.noninteractive.bg_stroke)
                        .rounding(4.0)
                        .inner_margin(4.0)
                };

                frame.show(ui, |ui| {
                    ui.set_width(130.0);
                    ui.set_height(130.0);

                    ui.vertical_centered(|ui| {
                        let mut selected = img_info.selected;
                        if ui.checkbox(&mut selected, "").changed() {
                            selection_changes.push((idx, selected));
                        }

                        let (rect, response) = ui.allocate_exact_size(egui::vec2(120.0, 90.0), egui::Sense::click());

                        if let Some(thumbnail) = &img_info.thumbnail {
                            let size = thumbnail.size_vec2();
                            let thumb_rect = egui::Rect::from_center_size(rect.center(), size);
                            ui.painter().image(
                                thumbnail.id(),
                                thumb_rect,
                                egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
                                egui::Color32::WHITE,
                            );
                        } else {
                            ui.painter().rect_filled(rect, 0.0, egui::Color32::DARK_GRAY);
                            ui.painter().text(
                                rect.center(),
                                egui::Align2::CENTER_CENTER,
                                "Loading...",
                                egui::FontId::proportional(10.0),
                                egui::Color32::LIGHT_GRAY,
                            );
                        }

                        if response.clicked() {
                            selected_idx = Some(idx);
                        }

                        ui.add_space(2.0);
                        let name = img_info.label.rsplit('/').next().unwrap_or(&img_info.label);
                        let filename_label = if name.chars().count() > 14 {
                            format!("{}...", name.chars().take(11).collect::<String>())
                        } else {
                            name.to_string()
                        };
                        ui.label(egui::RichText::new(filename_label).small()).on_hover_text(&img_info.label);

                        let status_text = match img_info.status {
                            AnalysisStatus::NotStarted => "",
                            AnalysisStatus::Running => "⏳",
                            AnalysisStatus::Completed => "✓",
                            AnalysisStatus::Failed => "✗",
                        };
                        let concerns: Vec<String> = state_guard
                            .analysis_results
                            .get(&img_info.path)
                            .map(|v| {
                                let many = v.len() > 1;
                                v.iter()
                                    .flat_map(|r| {
                                        r.concerns().into_iter().map(move |c| if many { format!("leaf {}: {c}", r.leaf) } else { c })
                                    })
                                    .collect()
                            })
                            .unwrap_or_default();
                        if !concerns.is_empty() {
                            ui.colored_label(WARN_COLOR, format!("{status_text} ⚠")).on_hover_text(concerns.join("\n"));
                        } else if !status_text.is_empty() {
                            ui.label(status_text);
                        }
                    });
                });
            }
        });

        drop(state_guard);
    });

    if !selection_changes.is_empty() {
        let mut state_guard = state.lock().unwrap();
        for (idx, selected) in selection_changes {
            if let Some(img) = state_guard.images.get_mut(idx) {
                img.selected = selected;
            }
        }
    }

    if let Some(idx) = selected_idx {
        state.lock().unwrap().select_image(idx);
    }
}
