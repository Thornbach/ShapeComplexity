// src/feature_extraction.rs - Feature extraction for EC/MC analysis

use image::RgbaImage;

use crate::errors::{LeafComplexError, Result};
use crate::path_algorithms::{
    calculate_straight_path_length, calculate_diego_path,
    calculate_diego_path_length, calculate_diego_path_pink, trace_straight_line,
    check_straight_line_transparency,
};

#[derive(Debug, Clone)]
pub struct MarginalPointFeatures {
    pub point_index: usize,
    pub straight_path_length: f64,
    pub diego_path_length: f64,
    pub diego_path_pink: Option<u32>,
    pub thornfiddle_path: f64,
    pub thornfiddle_path_harmonic: f64,
}

/// Generate features for all marginal points on the contour.
///
/// For both EC and MC, the geodesic path is used:
///   - If the straight line is clear → straight line (fast path)
///   - If the straight line clips a transparent pixel → BFS from reference_point
///     (the true geodesic, NOT the old hybrid straight+midpoint-BFS)
///
/// EC pink-pixel counts are measured along the geodesic path.
/// Because BFS now starts from reference_point, it traverses the leaf interior
/// and crosses the pink margin zone correctly near tooth tips, so tooth tips
/// consistently produce the highest pink counts.
pub fn generate_features(
    reference_point: (u32, u32),
    marginal_points: &[(u32, u32)],
    image: &RgbaImage,
    marked_image: Option<&RgbaImage>,
    marked_color: [u8; 3],
    is_ec: bool,
) -> Result<Vec<MarginalPointFeatures>> {
    if marginal_points.is_empty() {
        return Err(LeafComplexError::NoValidPoints);
    }

    let mut features = Vec::with_capacity(marginal_points.len());

    // EC navigates on marked_image (pink regions are opaque/walkable).
    // MC navigates on the original image (pink regions are transparent/blocked).
    let analysis_image = if is_ec {
        marked_image.unwrap_or(image)
    } else {
        image
    };

    for (idx, &marginal_point) in marginal_points.iter().enumerate() {
        let straight_path_length =
            calculate_straight_path_length(reference_point, marginal_point);

        let straight_line = trace_straight_line(reference_point, marginal_point);
        let crosses_transparency =
            check_straight_line_transparency(&straight_line, analysis_image);

        // calculate_diego_path now runs BFS from reference_point when transparency
        // is detected — giving the true geodesic, not a midpoint-started shortcut.
        let diego_path = if crosses_transparency {
            calculate_diego_path(reference_point, marginal_point, analysis_image)
        } else {
            straight_line.clone()
        };

        let diego_path_length = if crosses_transparency {
            calculate_diego_path_length(&diego_path)
        } else {
            straight_path_length
        };

        let diego_path_pink = if is_ec {
            if let Some(marked) = marked_image {
                Some(calculate_diego_path_pink(&diego_path, marked, marked_color))
            } else {
                None
            }
        } else {
            None
        };

        features.push(MarginalPointFeatures {
            point_index: idx,
            straight_path_length,
            diego_path_length,
            diego_path_pink,
            thornfiddle_path: 0.0,
            thornfiddle_path_harmonic: 0.0,
        });
    }

    Ok(features)
}