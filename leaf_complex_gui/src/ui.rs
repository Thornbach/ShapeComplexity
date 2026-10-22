// leaf_complex_gui/src/ui.rs
// Contains all UI rendering functions

use eframe::egui;
use egui_plot::{HLine, Line, Plot, PlotPoints, Points};
use std::sync::{Arc, Mutex};

use shapecomplexity::batch::VERSION;
use shapecomplexity::v2::TAU;
use shapecomplexity::MaskSource;

use crate::analysis::mask_texture;
use crate::state::{AnalysisStatus, AppState};

const OUTLINE_COLOR: egui::Color32 = egui::Color32::from_rgb(30, 120, 255);
const BLADE_LINE_COLOR: egui::Color32 = egui::Color32::from_rgb(220, 0, 170);
const RED_COLOR: egui::Color32 = egui::Color32::from_rgb(255, 0, 0);

fn source_label(s: Option<MaskSource>) -> &'static str {
    match s {
        None => "Choose mask source…",
        Some(MaskSource::Alpha) => "Alpha: opaque = leaf",
        Some(MaskSource::Dark) => "Dark: dark = leaf",
    }
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
                    ui.selectable_value(&mut source, Some(MaskSource::Alpha), "Alpha: opaque pixels of a cut-out are leaf");
                    ui.selectable_value(&mut source, Some(MaskSource::Dark), "Dark: dark pixels of a binary image are leaf");
                })
                .response
                .on_hover_text("Which pixels are leaf. There is no default; results are cleared when this changes.");
        });
        if let Some(s) = source {
            state.lock().unwrap().set_mask_source(s);
        }

        ui.separator();

        let (has_current, selected_count) = {
            let g = state.lock().unwrap();
            (g.current_image().is_some(), g.selected_count())
        };
        let ready = source.is_some() && !busy;
        ui.add_enabled_ui(has_current && ready, |ui| {
            if ui.button("▶ Analyze").clicked() {
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

        let (mut show_outline, mut show_bl, zoom) = {
            let g = state.lock().unwrap();
            (g.show_outline, g.show_blade_line, g.zoom_level)
        };
        if ui.checkbox(&mut show_outline, "Outline").changed() {
            state.lock().unwrap().show_outline = show_outline;
        }
        if ui.checkbox(&mut show_bl, "Blade line").changed() {
            state.lock().unwrap().show_blade_line = show_bl;
        }
        if ui.button("Reset View").clicked() {
            state.lock().unwrap().reset_view();
        }
        ui.label(format!("Zoom: {:.0}%", zoom * 100.0));
    });

    ui.separator();

    let response = ui.allocate_rect(ui.available_rect_before_wrap(), egui::Sense::click_and_drag());
    let rect = response.rect;
    let painter = ui.painter_at(rect);

    let mut g = state.lock().unwrap();
    let current_path = g.current_image().map(|i| i.path.clone());

    // texture for the image on screen only
    if let (Some(path), Some(result)) = (current_path.as_ref(), g.current_result()) {
        if g.mask_texture.as_ref().is_none_or(|(p, _)| p != path) {
            let tex = mask_texture(&result.mask, format!("{}_mask", path.display()), ctx);
            g.mask_texture = Some((path.clone(), tex));
        }
    }

    match (g.current_result(), g.mask_texture.as_ref()) {
        (Some(result), Some((_, tex))) => {
            let tex_size = tex.size_vec2();
            // fit the picture to the panel, then apply the zoom
            let fit = (rect.width() / tex_size.x).min(rect.height() / tex_size.y);
            let scale = fit * g.zoom_level;
            let img_rect = egui::Rect::from_center_size(rect.center() + g.pan_offset, tex_size * scale);
            painter.image(
                tex.id(),
                img_rect,
                egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
                egui::Color32::WHITE,
            );
            let to_screen = |p: &[f32; 2]| img_rect.min + egui::vec2(p[0] * scale, p[1] * scale);
            let width = (1.5 * g.zoom_level).clamp(1.0, 3.0);
            if g.show_outline {
                let pts: Vec<egui::Pos2> = result.outline.iter().map(to_screen).collect();
                painter.add(egui::Shape::closed_line(pts, egui::Stroke::new(width, OUTLINE_COLOR)));
            }
            if g.show_blade_line {
                let pts: Vec<egui::Pos2> = result.blade_line.iter().map(to_screen).collect();
                painter.add(egui::Shape::closed_line(pts, egui::Stroke::new(width, BLADE_LINE_COLOR)));
            }
            if let Some(i) = g.selected_point {
                if let (Some(c), Some(b)) = (result.outline.get(i), result.blade_line.get(i)) {
                    let (c, b) = (to_screen(c), to_screen(b));
                    painter.line_segment([b, c], egui::Stroke::new(2.0, RED_COLOR));
                    painter.circle_filled(c, 4.0, RED_COLOR);
                    painter.circle_stroke(b, 4.0, egui::Stroke::new(2.0, RED_COLOR));
                }
            }
            painter.text(
                rect.left_bottom() + egui::vec2(6.0, -6.0),
                egui::Align2::LEFT_BOTTOM,
                "Normalised mask (√A = 1000 px). Grey: leaf (L′); orange: removed petiole; light grey: filled holes.",
                egui::FontId::proportional(12.0),
                ui.visuals().weak_text_color(),
            );
        }
        _ => {
            let text = match current_path.as_ref() {
                None => "Open a workspace folder to begin".to_string(),
                Some(p) => match g.errors.get(p) {
                    Some(e) => format!("Analysis failed:\n{e}"),
                    None if g.mask_source.is_none() => "Choose the mask source, then click 'Analyze'".to_string(),
                    None => "Click 'Analyze' to start the analysis".to_string(),
                },
            };
            painter.text(
                rect.center(),
                egui::Align2::CENTER_CENTER,
                text,
                egui::FontId::proportional(16.0),
                ui.visuals().text_color(),
            );
        }
    }

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
        row("EA (edge amount)", num(m.ea, 4), "Extra length of the margin profile h(s) per unit blade-line length");
        row("MC (macro-shape complexity)", num(m.mc, 3), "Sum of the positive values of ln(e(k)/τ) (plot below)");
        row("Elongation", num(m.elongation, 3), "Major / minor axis ratio of the blade");
        row("Petiole length", num(m.petiole_len, 3), "Fraction of √A; 0 = no petiole found");
        row("√A native", format!("{} px", num(m.sqrt_a_native, 1)), "Square root of the leaf area before normalisation");
        row("EA flag", m.ea_flag.as_str().to_string(), "From √A native: < 250 px not resolved, < 500 px below recommended");
        row("MC flag", m.mc_flag.as_str().to_string(), "From √A native: < 250 px not resolved");
        row("Mask source", g.mask_source.map_or("-", |s| s.as_str()).to_string(), "Which pixels were read as leaf");
        row("Version", VERSION.to_string(), "ShapeComplexity version");
    });
}

pub fn render_margin_plot(ui: &mut egui::Ui, state: &Arc<Mutex<AppState>>) {
    ui.heading("📊 Margin profile h(s)");
    let (data, selected) = {
        let g = state.lock().unwrap();
        match g.current_result() {
            Some(r) => (r.margin_profile.clone(), g.selected_point),
            None => {
                ui.label("No analysis data available");
                return;
            }
        }
    };
    ui.label("Distance of the outline from the blade line (px at √A = 1000). EA measures how much this curve wiggles. Click to show a point on the leaf.");
    let line = Line::new(data.iter().enumerate().map(|(i, h)| [i as f64, *h]).collect::<PlotPoints>())
        .color(BLADE_LINE_COLOR)
        .width(1.5)
        .name("h(s)");
    let response = Plot::new("margin_plot")
        .height(200.0)
        .x_axis_label("s (outline point, ≈ px)")
        .y_axis_label("h (px)")
        .allow_drag(false)
        .show(ui, |p| {
            p.line(line);
            if let Some(&h) = selected.and_then(|i| data.get(i)) {
                p.points(Points::new(vec![[selected.unwrap() as f64, h]]).color(RED_COLOR).radius(5.0));
            }
        });
    if response.response.clicked() {
        if let Some(pos) = response.response.interact_pointer_pos() {
            let x = response.transform.value_from_position(pos).x.round();
            if x >= 0.0 && (x as usize) < data.len() {
                state.lock().unwrap().selected_point = Some(x as usize);
            }
        }
    }
}

pub fn render_error_plot(ui: &mut egui::Ui, state: &Arc<Mutex<AppState>>) {
    ui.heading("📊 Elliptic Fourier error e(k)");
    let data = {
        let g = state.lock().unwrap();
        match g.current_result() {
            Some(r) => r.error_curve.clone(),
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
        "Error of the k-harmonic outline fit, as ln(e(k)/τ) with τ = {TAU} √A. MC is the sum of the values above 0."
    ));
    let pts: Vec<[f64; 2]> = data.iter().enumerate().map(|(k, e)| [(k + 1) as f64, (e / TAU).ln()]).collect();
    Plot::new("error_plot")
        .height(200.0)
        .x_axis_label("k (harmonics)")
        .y_axis_label("ln(e(k)/τ)")
        .show(ui, |p| {
            p.hline(HLine::new(0.0).color(egui::Color32::GRAY).name("τ"));
            p.line(Line::new(PlotPoints::from(pts.clone())).color(OUTLINE_COLOR).width(1.5));
            p.points(Points::new(pts).color(OUTLINE_COLOR).radius(3.0).name("ln(e(k)/τ)"));
        });
}

pub fn render_analysis_panel(ui: &mut egui::Ui, state: &Arc<Mutex<AppState>>) {
    egui::ScrollArea::vertical().show(ui, |ui| {
        render_results_panel(ui, state);
        ui.add_space(10.0);
        ui.separator();
        render_margin_plot(ui, state);
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
                        if !status_text.is_empty() {
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
