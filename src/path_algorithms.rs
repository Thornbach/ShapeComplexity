// src/path_algorithms.rs - Path analysis algorithms for geodesic calculations

use image::RgbaImage;
use bresenham::Bresenham;
use std::collections::{VecDeque, HashMap};

/// Trace a straight line path between two points using Bresenham's algorithm
pub fn trace_straight_line(
    start: (u32, u32),
    end: (u32, u32),
) -> Vec<(u32, u32)> {
    let (start_x, start_y) = (start.0 as isize, start.1 as isize);
    let (end_x, end_y) = (end.0 as isize, end.1 as isize);
    let line = Bresenham::new((start_x, start_y), (end_x, end_y));
    line.map(|(x, y)| (x as u32, y as u32)).collect()
}

/// Check if a straight line path crosses any transparent pixels (excluding endpoints)
pub fn check_straight_line_transparency(
    line_points: &[(u32, u32)],
    image: &RgbaImage,
) -> bool {
    let (width, height) = image.dimensions();

    if line_points.len() <= 2 {
        return false;
    }

    for i in 1..(line_points.len() - 1) {
        let (x, y) = line_points[i];
        if x < width && y < height {
            if image.get_pixel(x, y)[3] == 0 {
                return true;
            }
        }
    }
    false
}

/// Euclidean distance between two points
pub fn calculate_straight_path_length(point1: (u32, u32), point2: (u32, u32)) -> f64 {
    let dx = point1.0 as f64 - point2.0 as f64;
    let dy = point1.1 as f64 - point2.1 as f64;
    (dx * dx + dy * dy).sqrt()
}

/// Calculate the geodesic (Diego) path from reference_point to margin_point,
/// staying within non-transparent pixels.
///
/// If the straight line is clear, it is returned directly.
/// If the straight line clips a transparent pixel, BFS runs from
/// **reference_point** (not from the midpoint where the line exited the leaf).
///
/// # Why full BFS from reference_point matters
///
/// The old implementation started BFS from the last opaque pixel on the
/// straight line before it hit the gap.  This produced a hybrid path:
///
///   [straight segment to sinus edge]  +  [BFS shortcut through leaf interior]
///
/// The BFS shortcut avoids the leaf margin entirely, which means:
/// - For EC: pink-pixel counts are wrong (the path cuts through the interior,
///   missing the margin material near tooth tips).
/// - For MC: path length is not the true geodesic from COM.
///
/// Starting BFS from reference_point gives the TRUE shortest geodesic inside
/// the leaf.  For EC this means the path naturally traverses the leaf interior
/// and then the pink margin zone near the target tooth, producing correct
/// (and maximum) pink-pixel counts at tooth tips.  For MC it gives the true
/// geodesic distance through sinuses.
pub fn calculate_diego_path(
    reference_point: (u32, u32),
    margin_point: (u32, u32),
    image: &RgbaImage,
) -> Vec<(u32, u32)> {
    // Fast path: straight line is clear, no BFS needed
    let straight_line = trace_straight_line(reference_point, margin_point);
    if !check_straight_line_transparency(&straight_line, image) {
        return straight_line;
    }

    // Full BFS from reference_point to margin_point through opaque pixels.
    // This gives the true geodesic, not a hybrid straight+BFS path.
    let (width, height) = image.dimensions();
    let mut queue: VecDeque<(u32, u32)> = VecDeque::new();
    let mut visited: HashMap<(u32, u32), (u32, u32)> = HashMap::new();

    // Cardinal directions first so BFS prefers axis-aligned steps over diagonals
    // when path length is equal — matches the old behaviour for clear paths.
    let directions: [(i32, i32); 8] = [
        (0, 1), (1, 0), (0, -1), (-1, 0),   // Cardinal
        (1, 1), (1, -1), (-1, 1), (-1, -1), // Diagonal
    ];

    // Seed: reference_point must itself be non-transparent
    if image.get_pixel(reference_point.0, reference_point.1)[3] == 0 {
        // Reference point is transparent — fall back to straight line
        return straight_line;
    }

    queue.push_back(reference_point);
    visited.insert(reference_point, reference_point);

    let max_iterations = (width * height) as usize * 2;
    let mut iterations = 0;
    let mut found = false;

    'bfs: while let Some(current) = queue.pop_front() {
        iterations += 1;
        if iterations > max_iterations {
            println!("Warning: BFS exceeded max iterations ({}) — returning straight line", max_iterations);
            return straight_line;
        }

        for &(dx, dy) in &directions {
            let nx = current.0 as i32 + dx;
            let ny = current.1 as i32 + dy;

            if nx < 0 || ny < 0 || nx >= width as i32 || ny >= height as i32 {
                continue;
            }
            let next = (nx as u32, ny as u32);

            if visited.contains_key(&next) {
                continue;
            }
            if image.get_pixel(next.0, next.1)[3] == 0 {
                continue; // skip transparent
            }

            visited.insert(next, current);

            if next == margin_point {
                found = true;
                break 'bfs;
            }

            queue.push_back(next);
        }
    }

    if !found {
        println!("BFS could not reach target — returning straight line");
        return straight_line;
    }

    // Reconstruct path from margin_point back to reference_point, then reverse
    let mut path = Vec::new();
    let mut cur = margin_point;
    loop {
        path.push(cur);
        let prev = visited[&cur];
        if prev == cur {
            break; // reached reference_point (points to itself)
        }
        cur = prev;
    }
    path.reverse();
    path
}

/// Total length of a path in pixels (Euclidean step-by-step)
pub fn calculate_diego_path_length(path: &[(u32, u32)]) -> f64 {
    if path.len() < 2 {
        return 0.0;
    }
    let mut length = 0.0;
    for i in 1..path.len() {
        let dx = path[i].0 as f64 - path[i - 1].0 as f64;
        let dy = path[i].1 as f64 - path[i - 1].1 as f64;
        length += (dx * dx + dy * dy).sqrt();
    }
    length
}

/// Count pink (marked) pixels along a path.
/// Used for EC analysis to measure how much eroded margin material the path crosses.
pub fn calculate_diego_path_pink(
    path: &[(u32, u32)],
    marked_image: &RgbaImage,
    pink_color: [u8; 3],
) -> u32 {
    let mut count = 0;
    for &(x, y) in path {
        let p = marked_image.get_pixel(x, y);
        if p[0] == pink_color[0] && p[1] == pink_color[1] && p[2] == pink_color[2] {
            count += 1;
        }
    }
    count
}