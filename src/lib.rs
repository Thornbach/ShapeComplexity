// src/lib.rs - ShapeComplexity library: edge amount (EA) and macro-shape complexity (MC) of leaf outlines
//
// `v2` holds the method; `batch` the parts shared by the CLI and the GUI; `gui_api` an egui-free entry point
// for front-ends. One pipeline function, `v2::measure_traced`, serves all of them.

pub mod batch;
pub mod gui_api;
pub mod v2;

pub use gui_api::{analyze, analyze_rgba, MorphOverlay, MorphResult};
pub use v2::{measure, measure_traced, MaskSource, Measurement, Trace};
