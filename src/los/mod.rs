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
pub use shapes::{extract_footprint_edges, extract_solid_edges, is_valid_model_placement, point_in_shape};
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

pub const N_CANDIDATES: usize = 24;
pub const N_PERIMETER: usize = 8;

/// Build source points for Mode 2: base-perimeter LOS with movement reach.
/// Each tuple is `(center, rx, ry, movement_inches)`.
///
/// Algorithm per base:
/// 1. Candidates: always `center`; if movement > 0, also 24 points on the
///    expanded ellipse `(rx + movement, ry + movement)`.
/// 2. Filter: skip non-center candidates that fail `is_valid_model_placement`.
/// 3. Per valid candidate: emit 8 equally-spaced points on the base ellipse `(rx, ry)`.
pub fn unit_sources(bases: &[(Vec2, f32, f32, f32)], pieces: &[TerrainPiece]) -> Vec<Vec2> {
    bases
        .iter()
        .flat_map(|&(center, rx, ry, movement)| {
            let mut candidates = vec![center];
            if movement > 0.0 {
                let erx = rx + movement;
                let ery = ry + movement;
                for i in 0..N_CANDIDATES {
                    let angle = i as f32 * std::f32::consts::TAU / N_CANDIDATES as f32;
                    candidates.push(center + Vec2::new(angle.cos() * erx, angle.sin() * ery));
                }
            }

            // Filter non-center candidates by placement validity.
            candidates.retain(|&c| c == center || is_valid_model_placement(c, rx, ry, pieces));

            // Emit N_PERIMETER base perimeter points for each valid candidate.
            candidates.into_iter().flat_map(move |cand| {
                (0..N_PERIMETER).map(move |i| {
                    let angle = i as f32 * std::f32::consts::TAU / N_PERIMETER as f32;
                    cand + Vec2::new(angle.cos() * rx, angle.sin() * ry)
                })
            })
        })
        .collect()
}

/// Metadata for a single valid candidate position, used to drive the staged UI.
pub struct CandidateInfo {
    /// World position of this candidate (where the base would be placed).
    pub center: Vec2,
    /// The attacker unit's current world position (for the movement-reach ring).
    pub unit_center: Vec2,
    pub rx: f32,
    pub ry: f32,
    /// rx + movement (radius of reach ellipse; equals rx when movement == 0).
    pub movement_rx: f32,
    pub movement_ry: f32,
    /// Index of the source unit in the `bases` slice (for deduplicating reach rings).
    pub unit_idx: usize,
}

pub struct UnitCandidateData {
    /// Flat list of perimeter source points; identical to what `unit_sources` returns.
    /// `sources[i * N_PERIMETER .. (i+1) * N_PERIMETER]` are the points for `candidates[i]`.
    pub sources: Vec<Vec2>,
    pub candidates: Vec<CandidateInfo>,
}

/// Like `unit_sources` but also returns per-candidate metadata needed for the staged UI.
pub fn unit_sources_with_candidates(
    bases: &[(Vec2, f32, f32, f32)],
    pieces: &[TerrainPiece],
) -> UnitCandidateData {
    let mut sources = Vec::new();
    let mut candidates = Vec::new();

    for (unit_idx, &(center, rx, ry, movement)) in bases.iter().enumerate() {
        let mut cands = vec![center];
        if movement > 0.0 {
            let erx = rx + movement;
            let ery = ry + movement;
            for i in 0..N_CANDIDATES {
                let angle = i as f32 * std::f32::consts::TAU / N_CANDIDATES as f32;
                cands.push(center + Vec2::new(angle.cos() * erx, angle.sin() * ery));
            }
        }

        cands.retain(|&c| c == center || is_valid_model_placement(c, rx, ry, pieces));

        for cand in cands {
            candidates.push(CandidateInfo {
                center: cand,
                unit_center: center,
                rx,
                ry,
                movement_rx: rx + movement,
                movement_ry: ry + movement,
                unit_idx,
            });
            for i in 0..N_PERIMETER {
                let angle = i as f32 * std::f32::consts::TAU / N_PERIMETER as f32;
                sources.push(cand + Vec2::new(angle.cos() * rx, angle.sin() * ry));
            }
        }
    }

    UnitCandidateData { sources, candidates }
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
        // No movement: 1 candidate (center) → 8 base perimeter sources.
        let bases = vec![(Vec2::new(30.0, 22.0), 1.0, 1.0, 0.0)];
        let srcs = unit_sources(&bases, &[]);
        assert_eq!(srcs.len(), 8);
        // Each point should be ~1.0 from center.
        for pt in &srcs {
            let dist = (*pt - Vec2::new(30.0, 22.0)).length();
            assert!((dist - 1.0).abs() < 1e-5, "Expected dist 1.0, got {dist}");
        }
    }

    #[test]
    fn unit_sources_with_movement() {
        // movement=6.0, no terrain: 1 center + 24 expanded = 25 candidates → 200 sources.
        let bases = vec![(Vec2::new(30.0, 22.0), 1.0, 1.0, 6.0)];
        let srcs = unit_sources(&bases, &[]);
        assert_eq!(srcs.len(), 200); // 25 candidates × 8 perimeter points
    }

    #[test]
    fn unit_sources_two_bases() {
        // 2 bases, no movement: each has 1 candidate → 8 sources each, 16 total.
        let bases = vec![
            (Vec2::new(10.0, 10.0), 1.0, 1.0, 0.0),
            (Vec2::new(50.0, 10.0), 1.0, 1.0, 0.0),
        ];
        let srcs = unit_sources(&bases, &[]);
        assert_eq!(srcs.len(), 16); // 8 per base
    }
}
