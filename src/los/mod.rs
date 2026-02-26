pub mod occupancy;
pub mod shapes;
pub mod vis_poly;

use bevy::math::Vec2;
use geo::BooleanOps;
use rayon::prelude::*;
use std::collections::HashSet;

use crate::types::deployment::DeploymentZone;
use crate::types::terrain::TerrainPiece;

pub use occupancy::get_terrain_occupancy;
pub use shapes::{extract_obstacle_edges, point_in_shape};
pub use vis_poly::{verts_to_geo_polygon, visibility_polygon};

/// Compute the union of visibility polygons from all source points.
/// Runs per-source computation in parallel (rayon), then unions with geo::BooleanOps.
pub fn run_analysis(
    sources: Vec<Vec2>,
    pieces: &[TerrainPiece],
) -> geo::MultiPolygon<f64> {
    // Per-source: parallel rayon computation.
    let visibility_polys: Vec<geo::Polygon<f64>> = sources
        .par_iter()
        .map(|&src| {
            let occupancy = get_terrain_occupancy(src, pieces);
            let edges = extract_obstacle_edges(pieces, &occupancy);
            let verts = visibility_polygon(src, &edges);
            verts_to_geo_polygon(verts)
        })
        .collect();

    // Union all polygons into one MultiPolygon.
    visibility_polys.into_iter().fold(
        geo::MultiPolygon::new(vec![]),
        |acc, poly| {
            let mp = geo::MultiPolygon::new(vec![poly]);
            acc.union(&mp)
        },
    )
}

/// Build source points for Mode 1: sample the opponent's deployment zone at 2" spacing.
pub fn sample_zone_sources(zone: &DeploymentZone) -> Vec<Vec2> {
    zone.sample_interior(2.0)
}

/// Build source points for Mode 2: base positions, optionally expanded by M inches.
/// For each base, if movement_inches > 0, sample a disk (center + 12 perimeter points).
pub fn unit_sources(bases: &[(Vec2, f32)], movement_inches: f32) -> Vec<Vec2> {
    bases
        .iter()
        .flat_map(|&(center, _)| {
            if movement_inches <= 0.0 {
                return vec![center];
            }
            let mut pts = vec![center];
            for i in 0..12 {
                let angle = i as f32 * std::f32::consts::TAU / 12.0;
                pts.push(center + Vec2::from_angle(angle) * movement_inches);
            }
            pts
        })
        .collect()
}

/// Compute the area of a geo MultiPolygon in the same units as its coordinates (square inches).
pub fn multi_polygon_area(mp: &geo::MultiPolygon<f64>) -> f64 {
    use geo::Area;
    mp.unsigned_area()
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
                positions.push([coord.x as f32, coord.y as f32, 3.0]);
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
        let result = run_analysis(sources, &[]);
        use geo::Area;
        let area = result.unsigned_area();
        // Open board is 60 × 44 = 2640 sq in; visibility from center should cover nearly all.
        assert!(area > 2000.0, "Expected large coverage, got {area}");
    }

    #[test]
    fn unit_sources_no_movement() {
        let bases = vec![(Vec2::new(5.0, 5.0), 0.0)];
        let srcs = unit_sources(&bases, 0.0);
        assert_eq!(srcs.len(), 1);
        assert_eq!(srcs[0], Vec2::new(5.0, 5.0));
    }

    #[test]
    fn unit_sources_with_movement() {
        let bases = vec![(Vec2::new(5.0, 5.0), 0.0)];
        let srcs = unit_sources(&bases, 6.0);
        assert_eq!(srcs.len(), 13); // center + 12 perimeter
    }
}
