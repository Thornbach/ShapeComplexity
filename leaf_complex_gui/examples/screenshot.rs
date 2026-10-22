// examples/screenshot.rs - renders the GUI on one analysed leaf and saves a screenshot. A development check of the
// panels (no clicks needed, e.g. under Xvfb); not part of the application.
//
// Usage: cargo run --example screenshot -- <image or folder> <alpha|dark> <out.png> [band] [input] [all] [leaf=<n>] [zoom=<f>]
//        [pan=<x>,<y>] [k=<n>] [mm=<px per mm>] [morpho]
// A folder: every image in it is analysed; the first is shown.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use eframe::egui;
use leaf_complex_gui::analysis::AnalysisEngine;
use leaf_complex_gui::state::{AnalysisStatus, AppState, CentralView, ImageInfo, ViewMode};
use leaf_complex_gui::LeafComplexApp;
use shapecomplexity::MaskSource;

struct Shot {
    app: LeafComplexApp,
    out: PathBuf,
    frames: usize,
}

impl eframe::App for Shot {
    fn update(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        self.app.update(ctx, frame);
        self.frames += 1;
        if self.frames == 10 {
            ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot);
        }
        let shot = ctx.input(|i| {
            i.raw.events.iter().find_map(|e| match e {
                egui::Event::Screenshot { image, .. } => Some(image.clone()),
                _ => None,
            })
        });
        if let Some(img) = shot {
            let bytes: Vec<u8> = img.pixels.iter().flat_map(|c| c.to_array()).collect();
            image::RgbaImage::from_raw(img.size[0] as u32, img.size[1] as u32, bytes)
                .expect("screenshot size")
                .save(&self.out)
                .expect("save screenshot");
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
        ctx.request_repaint();
    }
}

fn main() -> eframe::Result<()> {
    let args: Vec<String> = std::env::args().collect();
    let path = PathBuf::from(&args[1]);
    let source = if args[2] == "dark" { MaskSource::Dark } else { MaskSource::Alpha };
    let out = PathBuf::from(&args[3]);
    let flag = |name: &str| args.iter().any(|a| a == name);
    let value = |name: &str| args.iter().find_map(|a| a.strip_prefix(&format!("{name}="))).map(String::from);

    let mut st = AppState { workspace_dir: path.parent().map(PathBuf::from), ..Default::default() };
    if path.is_dir() {
        st.load_workspace(path.clone()).unwrap();
    } else {
        st.images.push(ImageInfo {
            path: path.clone(),
            label: path.file_name().unwrap().to_string_lossy().into_owned(),
            thumbnail: None,
            status: AnalysisStatus::NotStarted,
            selected: false,
        });
        st.current_image_index = Some(0);
    }
    st.set_mask_source(source);
    st.set_all_leaves(flag("all"));
    let paths: Vec<PathBuf> = st.images.iter().map(|i| i.path.clone()).collect();
    for p in &paths {
        let result = AnalysisEngine::new().analyze_image(p, source, flag("all"));
        st.set_result(p, result);
    }
    let n = st.current_leaves().first().map_or(0, |r| r.outline.len());
    if flag("morpho") {
        st.central_view = CentralView::Morphospace;
    }
    if let Some(l) = value("leaf") {
        st.select_leaf(l.parse::<usize>().unwrap() - 1);
    }
    if let Some(mm) = value("mm") {
        st.px_per_mm_text = mm.clone();
        st.px_per_mm = mm.parse().ok();
    }
    st.show_band = flag("band");
    if flag("input") {
        st.view_mode = ViewMode::Input;
    }
    if let Some(z) = value("zoom") {
        st.zoom_level = z.parse().unwrap();
    }
    if let Some((x, y)) = value("pan").as_deref().and_then(|v| v.split_once(',')) {
        st.pan_offset = egui::vec2(x.parse().unwrap(), y.parse().unwrap());
    }
    if let Some(k) = value("k") {
        st.efd_k = k.parse().unwrap();
    }
    st.selected_point = Some(n / 3);

    let app = LeafComplexApp::with_state(Arc::new(Mutex::new(st)));
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_inner_size([1400.0, 1500.0]),
        ..Default::default()
    };
    eframe::run_native("screenshot", options, Box::new(|_| Ok(Box::new(Shot { app, out, frames: 0 }))))
}
