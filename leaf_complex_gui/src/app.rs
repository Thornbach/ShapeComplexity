// Main Application Structure
use eframe::egui;
use std::fs;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::thread;

use shapecomplexity::batch::{leaf_stem, leaf_title, name_clash, write_file, Columns, LeafExport};
use shapecomplexity::figure::{leaf_svg, morphospace_svg};

use crate::analysis::AnalysisEngine;
use crate::state::{AnalysisStatus, AppState};
use crate::ui;

pub struct LeafComplexApp {
    state: Arc<Mutex<AppState>>,
    show_about: bool,
}

impl LeafComplexApp {
    pub fn new(_cc: &eframe::CreationContext<'_>) -> Self {
        Self::with_state(Arc::new(Mutex::new(AppState::default())))
    }

    /// The app on a prepared state (for tools that drive the GUI without clicks, e.g. `examples/screenshot.rs`).
    pub fn with_state(state: Arc<Mutex<AppState>>) -> Self {
        Self { state, show_about: false }
    }

    fn open_workspace(&mut self, ctx: &egui::Context) {
        if let Some(path) = rfd::FileDialog::new().set_title("Select Workspace Folder").pick_folder() {
            let res = self.state.lock().unwrap().load_workspace(path);
            match res {
                Ok(()) => self.generate_all_thumbnails(ctx),
                Err(e) => self.state.lock().unwrap().last_error = Some(e),
            }
        }
    }

    fn render_menu_bar(&mut self, ctx: &egui::Context, ui: &mut egui::Ui) {
        egui::menu::bar(ui, |ui| {
            ui.menu_button("File", |ui| {
                if ui.button("📁 Open Workspace...").clicked() {
                    ui.close_menu();
                    self.open_workspace(ctx);
                }

                ui.separator();

                if ui.button("💾 Export Results...").clicked() {
                    ui.close_menu();
                    self.export_results();
                }
                if ui.button("🖼 Export Figure of This Leaf...").clicked() {
                    ui.close_menu();
                    self.export_figure();
                }
                if ui.button("🖼 Export Figures...").clicked() {
                    ui.close_menu();
                    self.export_leaf_files(true);
                }
                if ui.button("📈 Export Curves...").clicked() {
                    ui.close_menu();
                    self.export_leaf_files(false);
                }
                if ui.button("📊 Export Morphospace...").clicked() {
                    ui.close_menu();
                    self.export_morphospace();
                }

                ui.separator();

                if ui.button("❌ Exit").clicked() {
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                }
            });

            ui.menu_button("View", |ui| {
                let mut state = self.state.lock().unwrap();
                ui.checkbox(&mut state.show_outline, "Show outline");
                ui.checkbox(&mut state.show_blade_line, "Show blade line");
                ui.separator();
                if ui.button("🔍 Reset Zoom").clicked() {
                    state.reset_view();
                    ui.close_menu();
                }
            });

            ui.menu_button("Analysis", |ui| {
                let (ready, selected_count) = {
                    let g = self.state.lock().unwrap();
                    (!g.analysis_in_progress && !g.batch_processing, g.selected_count())
                };
                ui.add_enabled_ui(ready, |ui| {
                    if ui.button("▶ Analyze Current Image").clicked() {
                        self.analyze_current_image(ctx);
                        ui.close_menu();
                    }
                    let batch_label = if selected_count > 0 {
                        format!("⏩ Analyze Selected ({})", selected_count)
                    } else {
                        "⏩ Analyze All Images".to_string()
                    };
                    if ui.button(batch_label).clicked() {
                        self.analyze_all_images(ctx);
                        ui.close_menu();
                    }
                });
            });

            ui.menu_button("Help", |ui| {
                if ui.button("ℹ️ About").clicked() {
                    self.show_about = true;
                    ui.close_menu();
                }
            });
        });
    }

    fn generate_all_thumbnails(&self, ctx: &egui::Context) {
        let state = Arc::clone(&self.state);
        let engine = AnalysisEngine::new();
        let ctx_clone = ctx.clone();

        thread::spawn(move || {
            let paths: Vec<_> = state.lock().unwrap().images.iter().map(|img| img.path.clone()).collect();
            for path in paths {
                if let Some(thumbnail) = engine.generate_thumbnail(&path, &ctx_clone) {
                    let mut g = state.lock().unwrap();
                    if let Some(img_info) = g.images.iter_mut().find(|img| img.path == path) {
                        img_info.thumbnail = Some(thumbnail);
                    }
                    drop(g);
                    ctx_clone.request_repaint();
                }
            }
        });
    }

    fn analyze_current_image(&mut self, ctx: &egui::Context) {
        let state = Arc::clone(&self.state);
        let ctx = ctx.clone();

        let (image_path, source, all) = {
            let mut g = state.lock().unwrap();
            let Some(path) = g.current_image().map(|i| i.path.clone()) else {
                return;
            };
            let source = g.mask_source;
            let all = g.all_leaves;
            g.analysis_in_progress = true;
            if let Some(img) = g.images.iter_mut().find(|i| i.path == path) {
                img.status = AnalysisStatus::Running;
            }
            (path, source, all)
        };

        thread::spawn(move || {
            let result = AnalysisEngine::new().analyze_image(&image_path, source, all);
            let mut g = state.lock().unwrap();
            g.analysis_in_progress = false;
            if let Err(e) = &result {
                g.last_error = Some(format!("Analysis failed: {e}"));
            }
            g.set_result(&image_path, result);
            ctx.request_repaint();
        });
    }

    /// Work-stealing batch processing — threads pick up new work when finished.
    fn analyze_all_images(&mut self, ctx: &egui::Context) {
        let state = Arc::clone(&self.state);
        let ctx = ctx.clone();

        let (image_paths, source, all): (Vec<PathBuf>, _, _) = {
            let g = state.lock().unwrap();
            let source = g.mask_source;
            let selected = g.get_selected_images();
            let paths = if selected.is_empty() { g.images.iter().map(|img| img.path.clone()).collect() } else { selected };
            (paths, source, g.all_leaves)
        };

        if image_paths.is_empty() {
            return;
        }

        {
            let mut g = state.lock().unwrap();
            g.batch_processing = true;
            g.current_batch_index = 0;
            g.total_batch_count = image_paths.len();
            for img in g.images.iter_mut() {
                if image_paths.contains(&img.path) {
                    img.status = AnalysisStatus::Running;
                }
            }
        }

        thread::spawn(move || {
            use std::sync::mpsc;

            let num_threads = std::cmp::min(num_cpus::get(), 8);
            let work_queue = Arc::new(Mutex::new(image_paths.clone()));
            let (tx, rx) = mpsc::channel();
            let mut handles = vec![];

            for _ in 0..num_threads {
                let queue = Arc::clone(&work_queue);
                let tx = tx.clone();
                handles.push(thread::spawn(move || {
                    let engine = AnalysisEngine::new();
                    loop {
                        let Some(path) = queue.lock().unwrap().pop() else { break };
                        let result = engine.analyze_image(&path, source, all);
                        if tx.send((path, result)).is_err() {
                            break;
                        }
                    }
                }));
            }
            drop(tx);

            let mut completed = 0;
            for (path, result) in rx {
                completed += 1;
                let mut g = state.lock().unwrap();
                g.current_batch_index = completed;
                if let Err(e) = &result {
                    eprintln!("{e}");
                }
                g.set_result(&path, result);
                drop(g);
                ctx.request_repaint();
            }

            for (i, handle) in handles.into_iter().enumerate() {
                if let Err(e) = handle.join() {
                    eprintln!("Thread {} panicked: {:?}", i, e);
                }
            }

            let mut g = state.lock().unwrap();
            g.batch_processing = false;
            let failed = g.errors.len();
            if failed > 0 {
                g.last_message = Some(format!("Batch done: {completed} images, {failed} failed (marked ✗)."));
            }
        });
    }

    /// One CSV for the selected (or all) analysed images, same columns as the command line (D116 columns when all
    /// leaves were measured or a scale is given).
    fn export_results(&mut self) {
        let (rows, cols): (Vec<String>, Columns) = {
            let g = self.state.lock().unwrap();
            let cols = Columns { leaves: g.all_leaves, px_per_mm: g.px_per_mm };
            let rows = g
                .export_set()
                .into_iter()
                .flat_map(|(img, v)| v.iter().map(|r| cols.row_of(&img.label, r.leaf, r.centroid, r.px, r.source, &r.measurement)))
                .collect();
            (rows, cols)
        };

        if rows.is_empty() {
            self.state.lock().unwrap().last_error = Some("No analysed images to export.".to_string());
            return;
        }

        let Some(path) = rfd::FileDialog::new()
            .set_title("Save Results")
            .set_file_name("shapecomplexity_results.csv")
            .add_filter("CSV", &["csv"])
            .save_file()
        else {
            return;
        };

        let text = std::iter::once(cols.header()).chain(rows.iter().cloned()).collect::<Vec<_>>().join("\n") + "\n";
        let mut g = self.state.lock().unwrap();
        match fs::write(&path, text) {
            Ok(()) => g.last_message = Some(format!("Exported {} rows to {}", rows.len(), path.display())),
            Err(e) => g.last_error = Some(format!("Failed to write {}: {e}", path.display())),
        }
    }

    /// Fig. 1-style SVG of the leaf on screen (the file `--figures` writes for it).
    fn export_figure(&mut self) {
        let job = {
            let g = self.state.lock().unwrap();
            match (g.current_image(), g.current_result()) {
                (Some(img), Some(r)) => {
                    let n = g.all_leaves.then_some(r.leaf);
                    let name = img.label.rsplit('/').next().unwrap_or(&img.label);
                    Some((leaf_stem(name, n), leaf_title(&img.label, n), r.measurement.clone(), r.trace()))
                }
                _ => None,
            }
        };
        let Some((stem, title, m, trace)) = job else {
            self.state.lock().unwrap().last_error = Some("No analysed leaf on screen.".to_string());
            return;
        };
        let Some(path) = rfd::FileDialog::new()
            .set_title("Save Figure")
            .set_file_name(format!("{stem}.svg"))
            .add_filter("SVG", &["svg"])
            .save_file()
        else {
            return;
        };
        let res = write_file(&path, &leaf_svg(&title, &m, &trace));
        let mut g = self.state.lock().unwrap();
        match res {
            Ok(()) => g.last_message = Some(format!("Saved {}", path.display())),
            Err(e) => g.last_error = Some(format!("Failed to write {e}")),
        }
    }

    /// Figures (`figures`) or curve tables of the selected (or all) analysed leaves into a folder, keeping the
    /// workspace's subfolders: the files `--figures` / `--curves` write. Runs in the background.
    fn export_leaf_files(&mut self, figures: bool) {
        // (image path, leaf index, file stem, title)
        let jobs: Vec<(PathBuf, usize, String, String)> = {
            let g = self.state.lock().unwrap();
            let set = g.export_set();
            if let Some(e) = name_clash(set.iter().map(|(img, _)| img.label.as_str())) {
                drop(set);
                drop(g);
                self.state.lock().unwrap().last_error = Some(e);
                return;
            }
            set.iter()
                .flat_map(|(img, leaves)| {
                    leaves.iter().enumerate().map(|(i, r)| {
                        let n = g.all_leaves.then_some(r.leaf);
                        (img.path.clone(), i, leaf_stem(&img.label, n), leaf_title(&img.label, n))
                    })
                })
                .collect()
        };
        if jobs.is_empty() {
            self.state.lock().unwrap().last_error = Some("No analysed images to export.".to_string());
            return;
        }
        let what = if figures { "Figures" } else { "Curves" };
        let Some(dir) = rfd::FileDialog::new().set_title(format!("Folder for the {}", what.to_lowercase())).pick_folder()
        else {
            return;
        };
        let export = if figures {
            LeafExport { figures: Some(dir.clone()), curves: None }
        } else {
            LeafExport { figures: None, curves: Some(dir.clone()) }
        };
        let state = Arc::clone(&self.state);
        state.lock().unwrap().last_message = Some(format!("Exporting {} leaves...", jobs.len()));
        thread::spawn(move || {
            let mut failed = Vec::new();
            for (path, i, stem, title) in &jobs {
                let leaf = {
                    let g = state.lock().unwrap();
                    g.analysis_results.get(path).and_then(|v| v.get(*i)).map(|r| (r.measurement.clone(), r.trace()))
                };
                let Some((m, trace)) = leaf else { continue };
                if let Err(e) = export.write(stem, title, &m, &trace) {
                    failed.push(e);
                }
            }
            let mut g = state.lock().unwrap();
            match failed.first() {
                None => g.last_message = Some(format!("{what} of {} leaves saved in {}", jobs.len(), dir.display())),
                Some(e) => g.last_error = Some(format!("{} of {} files failed: {e}", failed.len(), jobs.len())),
            }
        });
    }

    /// EA-MC morphospace of all analysed leaves as SVG, coloured as in the tab (the file `--morphospace` writes).
    fn export_morphospace(&mut self) {
        let (points, highlight) = {
            let g = self.state.lock().unwrap();
            (g.morph_entries().into_iter().map(|e| e.point).collect::<Vec<_>>(), g.highlight.clone())
        };
        if points.is_empty() {
            self.state.lock().unwrap().last_error = Some("No analysed leaves yet.".to_string());
            return;
        }
        let Some(path) = rfd::FileDialog::new()
            .set_title("Save Morphospace")
            .set_file_name("morphospace.svg")
            .add_filter("SVG", &["svg"])
            .save_file()
        else {
            return;
        };
        let res = write_file(&path, &morphospace_svg(&points, highlight.as_deref()));
        let mut g = self.state.lock().unwrap();
        match res {
            Ok(()) => g.last_message = Some(format!("Saved {}", path.display())),
            Err(e) => g.last_error = Some(format!("Failed to write {e}")),
        }
    }
}

impl eframe::App for LeafComplexApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        let mut analyze_clicked = false;
        let mut batch_clicked = false;

        egui::TopBottomPanel::top("menu_bar").show(ctx, |ui| {
            self.render_menu_bar(ctx, ui);
        });

        egui::TopBottomPanel::bottom("thumbnails").min_height(150.0).max_height(150.0).show(ctx, |ui| {
            ui::render_thumbnail_strip(ui, &self.state);
        });

        egui::SidePanel::left("image_view")
            .default_width(700.0)
            .min_width(400.0)
            .max_width(1000.0)
            .resizable(true)
            .show(ctx, |ui| {
                ui::render_image_view(ui, &self.state, ctx, &mut analyze_clicked, &mut batch_clicked);
            });

        egui::CentralPanel::default().show(ctx, |ui| {
            ui::render_analysis_panel(ui, &self.state);
        });

        if analyze_clicked {
            self.analyze_current_image(ctx);
        }
        if batch_clicked {
            self.analyze_all_images(ctx);
        }

        // copy first: a guard in the `if let` line would stay locked for the whole block (Rust 2021), and the OK
        // button's lock would then wait for it forever
        let error = self.state.lock().unwrap().last_error.clone();
        if let Some(error) = error {
            egui::Window::new("⚠️ Error").collapsible(false).resizable(false).show(ctx, |ui| {
                ui.label(&error);
                if ui.button("OK").clicked() {
                    self.state.lock().unwrap().last_error = None;
                }
            });
        }

        let msg = self.state.lock().unwrap().last_message.clone();
        if let Some(msg) = msg {
            egui::Window::new("Done").collapsible(false).resizable(false).show(ctx, |ui| {
                ui.label(&msg);
                if ui.button("OK").clicked() {
                    self.state.lock().unwrap().last_message = None;
                }
            });
        }

        if self.show_about {
            egui::Window::new("About ShapeComplexity")
                .collapsible(false)
                .resizable(false)
                .open(&mut self.show_about)
                .show(ctx, |ui| {
                    ui.label(format!("ShapeComplexity {}", shapecomplexity::batch::VERSION));
                    ui.label("Edge amount (EA) and macro-shape complexity (MC) of leaf outlines.");
                    ui.hyperlink("https://github.com/Thornbach/ShapeComplexity");
                });
        }

        {
            let state = self.state.lock().unwrap();

            if state.analysis_in_progress {
                egui::Window::new("⏳ Analyzing...")
                    .collapsible(false)
                    .resizable(false)
                    .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                    .show(ctx, |ui| {
                        ui.spinner();
                        ui.label("Please wait...");
                    });
            }

            if state.batch_processing {
                egui::Window::new("⏳ Batch Processing...")
                    .collapsible(false)
                    .resizable(false)
                    .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                    .show(ctx, |ui| {
                        ui.spinner();
                        ui.label(format!("Processing {}/{}...", state.current_batch_index, state.total_batch_count));
                    });
            }
        }

        ctx.request_repaint_after(std::time::Duration::from_millis(100));
    }
}
