pub mod occupancy;
pub mod shapes;
pub mod vis_poly;

use bevy::math::Vec2;
use geo::{Area, Simplify};
use rayon::prelude::*;
use std::collections::HashSet;

use crate::types::deployment::DeploymentZone;
use crate::types::terrain::TerrainPiece;

pub use occupancy::get_terrain_occupancy;
pub use shapes::{extract_footprint_edges, extract_solid_edges, point_in_shape};
pub use vis_poly::{verts_to_geo_polygon, visibility_polygon, OneWayEdge};

const BOARD_INTERIOR_MARGIN: f32 = 0.05;

fn clamp_to_board_interior(p: Vec2) -> Vec2 {
    Vec2::new(
        p.x.clamp(BOARD_INTERIOR_MARGIN, 60.0 - BOARD_INTERIOR_MARGIN),
        p.y.clamp(BOARD_INTERIOR_MARGIN, 44.0 - BOARD_INTERIOR_MARGIN),
    )
}

/// Compute the union of visibility polygons from all source points.
/// Runs per-source computation in parallel (rayon), then unions with geo::BooleanOps.
/// Returns `(union, per_source)` where `per_source` is `(clamped_source, polygon_verts)` per source.
pub fn run_analysis(
    sources: Vec<Vec2>,
    pieces: &[TerrainPiece],
) -> (geo::MultiPolygon<f64>, Vec<(Vec2, Vec<Vec2>)>) {
    // Per-source: parallel rayon computation.
    let per_source: Vec<(Vec2, Vec<Vec2>)> = sources
        .par_iter()
        .map(|&src| {
            let src = clamp_to_board_interior(src);
            let occupancy = get_terrain_occupancy(src, pieces);
            let solid = extract_solid_edges(pieces, &occupancy);
            let one_way = extract_footprint_edges(pieces, &occupancy);
            let verts = visibility_polygon(src, &solid, &one_way);
            (src, verts)
        })
        .collect();

    let visibility_polys: Vec<geo::Polygon<f64>> = per_source
        .iter()
        .filter_map(|(_, verts)| {
            verts_to_geo_polygon(verts.clone())
                .map(|p| p.simplify(&1e-4))
                .filter(|p| p.exterior().0.len() >= 4 && p.unsigned_area() > 1e-6)
        })
        .collect();

    let union = geo::MultiPolygon::new(visibility_polys);

    (union, per_source)
}

/// Build source points for Mode 1: sample the opponent's deployment zone perimeter at 0.25" spacing.
pub fn sample_zone_sources(zone: &DeploymentZone) -> Vec<Vec2> {
    zone.sample_perimeter(0.25)
}

/// Build source points for Mode 2: 24 points on the expanded ellipse perimeter for each base.
/// Each tuple is (center, rx, ry, movement_inches); the expanded radii are rx+movement and ry+movement.
pub fn unit_sources(bases: &[(Vec2, f32, f32, f32)]) -> Vec<Vec2> {
    bases
        .iter()
        .flat_map(|&(center, rx, ry, movement)| {
            let erx = rx + movement;
            let ery = ry + movement;
            (0..24).map(move |i| {
                let angle = i as f32 * std::f32::consts::TAU / 24.0;
                center + Vec2::new(angle.cos() * erx, angle.sin() * ery)
            })
        })
        .collect()
}

/// Compute the area covered by a geo MultiPolygon via scanline rasterization.
/// Avoids BooleanOps union: each cell is marked once regardless of polygon overlap.
/// Resolution: 0.5" grid → accuracy ±0.25 sq in per boundary cell.
pub fn multi_polygon_area(mp: &geo::MultiPolygon<f64>) -> f64 {
    const STEP: f64 = 0.5;
    const NX: usize = 120; // ceil(60 / 0.5)
    const NY: usize = 88;  // ceil(44 / 0.5)

    let mut grid = vec![false; NX * NY];

    for poly in &mp.0 {
        let coords: Vec<_> = poly.exterior().0.iter().copied().collect();
        let n = coords.len();
        if n < 3 {
            continue;
        }

        for yi in 0..NY {
            let y = (yi as f64 + 0.5) * STEP;
            let mut xs: Vec<f64> = Vec::new();

            for i in 0..n - 1 {
                let (y0, y1) = (coords[i].y, coords[i + 1].y);
                let (x0, x1) = (coords[i].x, coords[i + 1].x);
                let (lo, hi) = if y0 < y1 { (y0, y1) } else { (y1, y0) };
                if y <= lo || y > hi {
                    continue;
                }
                xs.push(x0 + (y - y0) / (y1 - y0) * (x1 - x0));
            }

            xs.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));

            let mut i = 0;
            while i + 1 < xs.len() {
                let xi_start = ((xs[i] / STEP).ceil() as usize).min(NX);
                let xi_end = ((xs[i + 1] / STEP).floor() as usize).min(NX - 1);
                for xi in xi_start..=xi_end {
                    if xi < NX {
                        grid[yi * NX + xi] = true;
                    }
                }
                i += 2;
            }
        }
    }

    grid.iter().filter(|&&v| v).count() as f64 * STEP * STEP
}

/// Convert a geo MultiPolygon to a flat list of triangle vertex positions for mesh rendering.
/// Returns (positions: Vec<[f32; 3]>, indices: Vec<u32>).
pub fn triangulate_multi_polygon(mp: &geo::MultiPolygon<f64>) -> (Vec<[f32; 3]>, Vec<u32>) {
    use geo::TriangulateEarcut;

    let mut positions: Vec<[f32; 3]> = Vec::new();
    let mut indices: Vec<u32> = Vec::new();

    for polygon in mp.0.iter() {
        let triangles = polygon.earcut_triangles();
        let base_idx = positions.len() as u32;

        // Collect all unique coords from the triangulation.
        for tri in &triangles {
            for coord in [tri.0, tri.1, tri.2] {
                positions.push([coord.x as f32, coord.y as f32, 0.0]);
            }
        }

        let tri_count = triangles.len() as u32;
        for i in 0..tri_count {
            indices.push(base_idx + i * 3);
            indices.push(base_idx + i * 3 + 1);
            indices.push(base_idx + i * 3 + 2);
        }
    }

    (positions, indices)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn run_analysis_no_obstacles_covers_board() {
        let sources = vec![Vec2::new(30.0, 22.0)];
        let (result, _) = run_analysis(sources, &[]);
        use geo::Area;
        let area = result.unsigned_area();
        // Open board is 60 × 44 = 2640 sq in; visibility from center should cover nearly all.
        assert!(area > 2000.0, "Expected large coverage, got {area}");
    }

    #[test]
    fn unit_sources_no_movement() {
        // Base radius 1.0, no movement → 24 points on the unit circle around center.
        let bases = vec![(Vec2::new(5.0, 5.0), 1.0, 1.0, 0.0)];
        let srcs = unit_sources(&bases);
        assert_eq!(srcs.len(), 24);
        // Each point should be ~1.0 from center.
        for pt in &srcs {
            let dist = (*pt - Vec2::new(5.0, 5.0)).length();
            assert!((dist - 1.0).abs() < 1e-5, "Expected dist 1.0, got {dist}");
        }
    }

    #[test]
    fn unit_sources_with_movement() {
        // Base radius 1.0, movement 6.0 → 24 points at radius 7.0.
        let bases = vec![(Vec2::new(5.0, 5.0), 1.0, 1.0, 6.0)];
        let srcs = unit_sources(&bases);
        assert_eq!(srcs.len(), 24);
        for pt in &srcs {
            let dist = (*pt - Vec2::new(5.0, 5.0)).length();
            assert!((dist - 7.0).abs() < 1e-5, "Expected dist 7.0, got {dist}");
        }
    }

    #[test]
    fn unit_sources_two_bases() {
        let bases = vec![
            (Vec2::new(0.0, 0.0), 1.0, 1.0, 0.0),
            (Vec2::new(10.0, 0.0), 1.0, 1.0, 0.0),
        ];
        let srcs = unit_sources(&bases);
        assert_eq!(srcs.len(), 48); // 24 per base
    }
}
