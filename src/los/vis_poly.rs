use bevy::math::Vec2;

const BOARD_W: f32 = 60.0;
const BOARD_H: f32 = 44.0;
const EPSILON: f32 = 1e-6;

/// Compute the exact visibility polygon from `source` against a set of obstacle edges.
///
/// Uses the standard O(n log n) angular sweep:
/// 1. Collect all obstacle vertices and generate angular events (angle - ε, angle, angle + ε).
/// 2. Sort events by angle.
/// 3. For each event direction, cast a ray from `source` and record the nearest intersection.
/// 4. The resulting ordered boundary points form the visibility polygon.
///
/// The polygon is implicitly clipped to the board bounds because we add the four corners
/// as additional "always visible" vertices when nothing else blocks the ray in that direction.
pub fn visibility_polygon(source: Vec2, obstacle_edges: &[[Vec2; 2]]) -> Vec<Vec2> {
    // Board boundary edges (treated as obstacles for the sweep).
    let board_edges = board_boundary_edges();
    let all_edges: Vec<[Vec2; 2]> = obstacle_edges
        .iter()
        .copied()
        .chain(board_edges.iter().copied())
        .collect();

    // Collect angle events from all obstacle vertices (not board corners — they'll be
    // added indirectly).
    let mut angles: Vec<f32> = Vec::with_capacity(obstacle_edges.len() * 6);
    for edge in obstacle_edges {
        for &v in edge.iter() {
            let angle = (v - source).to_angle();
            angles.push(angle - EPSILON);
            angles.push(angle);
            angles.push(angle + EPSILON);
        }
    }
    // Also add board corner angles so the full boundary is traced.
    for &corner in &board_corners() {
        let angle = (corner - source).to_angle();
        angles.push(angle - EPSILON);
        angles.push(angle);
        angles.push(angle + EPSILON);
    }

    // Sort and deduplicate (within float tolerance).
    angles.sort_by(|a, b| a.partial_cmp(b).unwrap());
    angles.dedup_by(|a, b| (*a - *b).abs() < EPSILON * 0.1);

    let mut polygon_verts: Vec<Vec2> = Vec::with_capacity(angles.len());

    for &angle in &angles {
        let dir = Vec2::from_angle(angle);
        if let Some(hit) = nearest_ray_hit(source, dir, &all_edges) {
            polygon_verts.push(hit);
        }
    }

    // Remove near-duplicate consecutive vertices.
    deduplicate_verts(&mut polygon_verts);

    polygon_verts
}

/// Cast a ray from `origin` in `dir` against all edges; return the nearest hit.
fn nearest_ray_hit(origin: Vec2, dir: Vec2, edges: &[[Vec2; 2]]) -> Option<Vec2> {
    let mut best_t = f32::MAX;

    for &[a, b] in edges {
        if let Some(t) = ray_segment_t(origin, dir, a, b) {
            if t < best_t {
                best_t = t;
            }
        }
    }

    if best_t < f32::MAX {
        Some(origin + dir * best_t)
    } else {
        None
    }
}

/// Returns the ray parameter `t >= 0` where `origin + t*dir` intersects segment [a, b].
/// Returns None if parallel, behind the ray, or outside the segment.
fn ray_segment_t(origin: Vec2, dir: Vec2, a: Vec2, b: Vec2) -> Option<f32> {
    let v1 = origin - a;
    let v2 = b - a;
    let v3 = Vec2::new(-dir.y, dir.x);

    let denom = v2.dot(v3);
    if denom.abs() < EPSILON {
        return None;
    }

    let t1 = v2.perp_dot(v1) / denom;
    let t2 = v1.dot(v3) / denom;

    if t1 >= -EPSILON && t2 >= -EPSILON && t2 <= 1.0 + EPSILON {
        Some(t1.max(0.0))
    } else {
        None
    }
}

fn board_corners() -> [Vec2; 4] {
    [
        Vec2::new(0.0, 0.0),
        Vec2::new(BOARD_W, 0.0),
        Vec2::new(BOARD_W, BOARD_H),
        Vec2::new(0.0, BOARD_H),
    ]
}

fn board_boundary_edges() -> [[Vec2; 2]; 4] {
    let [c0, c1, c2, c3] = board_corners();
    [[c0, c1], [c1, c2], [c2, c3], [c3, c0]]
}

fn deduplicate_verts(verts: &mut Vec<Vec2>) {
    const MERGE_DIST: f32 = 1e-4;
    verts.dedup_by(|a, b| (*a - *b).length_squared() < MERGE_DIST * MERGE_DIST);
    // Also check wrap-around.
    if verts.len() >= 2 {
        let first = *verts.first().unwrap();
        let last = *verts.last().unwrap();
        if (first - last).length_squared() < MERGE_DIST * MERGE_DIST {
            verts.pop();
        }
    }
}

/// Convert a `Vec<Vec2>` polygon to a `geo::Polygon<f64>`.
pub fn verts_to_geo_polygon(verts: Vec<Vec2>) -> geo::Polygon<f64> {
    use geo::{Coord, LineString, Polygon};
    let coords: Vec<Coord<f64>> = verts
        .iter()
        .map(|v| Coord {
            x: v.x as f64,
            y: v.y as f64,
        })
        .collect();
    let line_string = LineString(coords);
    Polygon::new(line_string, vec![])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn open_field_covers_board() {
        // No obstacles → visibility polygon should be the four board corners.
        let source = Vec2::new(30.0, 22.0);
        let poly = visibility_polygon(source, &[]);
        assert!(poly.len() >= 4, "Should have at least 4 vertices");

        // All vertices should be on the board boundary.
        for v in &poly {
            let on_boundary = v.x.abs() < 0.01
                || (v.x - BOARD_W).abs() < 0.01
                || v.y.abs() < 0.01
                || (v.y - BOARD_H).abs() < 0.01;
            assert!(on_boundary, "Vertex {v:?} not on board boundary");
        }
    }

    #[test]
    fn wall_blocks_ray() {
        // A vertical wall at x=10, from y=0 to y=44, blocking the left half.
        let wall = [Vec2::new(10.0, 0.0), Vec2::new(10.0, BOARD_H)];
        let source = Vec2::new(30.0, 22.0);
        let poly = visibility_polygon(source, &[wall]);

        // No vertex should be west of x=10 (to the left of the wall).
        for v in &poly {
            assert!(
                v.x >= 10.0 - 0.01,
                "Vertex {v:?} is west of the blocking wall"
            );
        }
    }
}
