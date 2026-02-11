// GUI Main Entry Point for LeafComplexR
// Cross-platform desktop application for interactive leaf analysis

use eframe::egui;

mod app;
mod state;
mod ui;
mod analysis;
mod config_editor;

use app::LeafComplexApp;

fn main() -> Result<(), eframe::Error> {
    env_logger::init(); // Initialize logging
    
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1400.0, 900.0])
            .with_min_inner_size([1200.0, 700.0])
            .with_icon(load_icon()),
        ..Default::default()
    };

    eframe::run_native(
        "ShapeComplexity",
        options,
        Box::new(|cc| {
            // Set custom fonts and style
            setup_custom_fonts(&cc.egui_ctx);
            setup_custom_style(&cc.egui_ctx);
            
            Ok(Box::new(LeafComplexApp::new(cc)))
        }),
    )
}

fn setup_custom_fonts(ctx: &egui::Context) {
    let fonts = egui::FontDefinitions::default();
    
    // Add custom fonts if desired
    // fonts.font_data.insert(...);
    
    ctx.set_fonts(fonts);
}

fn setup_custom_style(ctx: &egui::Context) {
    let mut style = (*ctx.style()).clone();
    
    // Customize spacing
    style.spacing.item_spacing = egui::vec2(8.0, 6.0);
    style.spacing.window_margin = egui::Margin::same(10.0);
    
    // Customize rounding
    style.visuals.window_rounding = egui::Rounding::same(8.0);
    style.visuals.widgets.noninteractive.rounding = egui::Rounding::same(4.0);
    
    ctx.set_style(style);
}

fn load_icon() -> egui::IconData {
    // Load application icon
    // For now, return a default/placeholder
    egui::IconData {
        rgba: vec![0; 32 * 32 * 4],
        width: 32,
        height: 32,
    }
}
