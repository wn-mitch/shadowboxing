use bevy::math::{Mat2, Vec2};
use std::collections::HashSet;

use crate::types::terrain::{Mirror, TerrainPiece, TerrainShape};

/// Transform a world point into a piece's local coordinate frame.
/// Correct order: translate → inverse-mirror → inverse-rotate.
/// (The original JS bug was applying rotation before mirroring.)
pub fn world_to_local(world: Vec2, piece: &TerrainPiece) -> Vec2 {
    let translated = world - piece.world_position();
    let mirrored = apply_mirror(translated, &piece.mirror);
    Mat2::from_angle(-piece.rotation.to_radians()) * mirrored
}

/// Transform a piece-local point into world space.
/// Correct order: rotate → mirror → translate.
pub fn local_to_world(local: Vec2, piece: &TerrainPiece) -> Vec2 {
    let rotated = Mat2::from_angle(piece.rotation.to_radians()) * local;
    let mirrored = apply_mirror(rotated, &piece.mirror);
    mirrored + piece.world_position()
}

fn apply_mirror(v: Vec2, mirror: &Mirror) -> Vec2 {
    match mirror {
        Mirror::Horizontal => Vec2::new(-v.x, v.y),
        Mirror::Vertical => Vec2::new(v.x, -v.y),
        Mirror::None => v,
    }
}

/// Test whether a point in local space is inside a terrain shape.
/// `Line` shapes are never considered "containment" regions (they're wall edges).
pub fn point_in_shape_local(local: Vec2, shape: &TerrainShape) -> bool {
    match shape {
        TerrainShape::Rectangle { width, height } => {
            let half_w = width / 2.0;
            let half_h = height / 2.0;
            local.x >= -half_w && local.x <= half_w && local.y >= -half_h && local.y <= half_h
        }
        TerrainShape::Polygon { points } => {
            let verts: Vec<Vec2> = points.iter().map(|p| Vec2::new(p.x, p.y)).collect();
            point_in_polygon_local(local, &verts)
        }
        TerrainShape::Circle { radius } => local.length_squared() <= radius * radius,
        TerrainShape::Line { .. } => false,
    }
}

/// Convenience: test in world space (transforms point to local first).
pub fn point_in_shape(world: Vec2, shape: &TerrainShape, piece: &TerrainPiece) -> bool {
    let local = world_to_local(world, piece);
    point_in_shape_local(local, shape)
}

fn point_in_polygon_local(p: Vec2, verts: &[Vec2]) -> bool {
    let n = verts.len();
    if n < 3 {
        return false;
    }
    let mut inside = false;
    let mut j = n - 1;
    for i in 0..n {
        let vi = verts[i];
        let vj = verts[j];
        if ((vi.y > p.y) != (vj.y > p.y))
            && (p.x < (vj.x - vi.x) * (p.y - vi.y) / (vj.y - vi.y) + vi.x)
        {
            inside = !inside;
        }
        j = i;
    }
    inside
}

/// Extract all world-space obstacle edge segments from blocking terrain.
/// `exclude_footprints`: piece IDs whose first shape should be skipped (occupant rule).
/// Circles are approximated as 32-gons for the obstacle edge set.
pub fn extract_obstacle_edges(
    pieces: &[TerrainPiece],
    exclude_footprints: &HashSet<&str>,
) -> Vec<[Vec2; 2]> {
    let mut edges = Vec::new();

    for piece in pieces {
        if !piece.blocking {
            continue;
        }
        for (shape_idx, shape) in piece.shapes.iter().enumerate() {
            let is_footprint = shape_idx == 0;
            if is_footprint && exclude_footprints.contains(piece.id.as_str()) {
                continue;
            }
            let local_verts = shape_local_vertices(shape);
            let world_verts: Vec<Vec2> = local_verts
                .iter()
                .map(|&lv| local_to_world(lv, piece))
                .collect();
            // Emit edges from the vertex ring.
            let n = world_verts.len();
            for i in 0..n {
                let a = world_verts[i];
                let b = world_verts[(i + 1) % n];
                edges.push([a, b]);
            }
        }
    }
    edges
}

/// Return local-space vertices for a terrain shape.
/// Rectangle and Polygon produce a closed ring.
/// Line becomes a thin rectangle (4 vertices).
/// Circle becomes a 32-gon.
fn shape_local_vertices(shape: &TerrainShape) -> Vec<Vec2> {
    match shape {
        TerrainShape::Rectangle { width, height } => {
            let hw = width / 2.0;
            let hh = height / 2.0;
            vec![
                Vec2::new(-hw, -hh),
                Vec2::new(hw, -hh),
                Vec2::new(hw, hh),
                Vec2::new(-hw, hh),
            ]
        }
        TerrainShape::Polygon { points } => {
            points.iter().map(|p| Vec2::new(p.x, p.y)).collect()
        }
        TerrainShape::Circle { radius } => {
            const SEGMENTS: usize = 32;
            (0..SEGMENTS)
                .map(|i| {
                    let angle = i as f32 * std::f32::consts::TAU / SEGMENTS as f32;
                    Vec2::new(angle.cos() * radius, angle.sin() * radius)
                })
                .collect()
        }
        TerrainShape::Line {
            start,
            end,
            thickness,
        } => {
            let s = Vec2::new(start.x, start.y);
            let e = Vec2::new(end.x, end.y);
            let dir = (e - s).normalize_or_zero();
            let perp = Vec2::new(-dir.y, dir.x) * (thickness / 2.0);
            vec![s + perp, e + perp, e - perp, s - perp]
        }
    }
}

/// World-space obstacle vertices (used by vis_poly for angle events).
pub fn extract_obstacle_vertices(
    pieces: &[TerrainPiece],
    exclude_footprints: &HashSet<&str>,
) -> Vec<Vec2> {
    let mut verts = Vec::new();
    for piece in pieces {
        if !piece.blocking {
            continue;
        }
        for (shape_idx, shape) in piece.shapes.iter().enumerate() {
            let is_footprint = shape_idx == 0;
            if is_footprint && exclude_footprints.contains(piece.id.as_str()) {
                continue;
            }
            let local_verts = shape_local_vertices(shape);
            for lv in local_verts {
                verts.push(local_to_world(lv, piece));
            }
        }
    }
    verts
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::terrain::{JsonVec2, Mirror, TerrainPiece, TerrainShape};

    fn make_piece(rotation: f32, mirror: Mirror, pos: Vec2) -> TerrainPiece {
        TerrainPiece {
            id: "test".to_string(),
            name: "Test".to_string(),
            shapes: vec![TerrainShape::Rectangle {
                width: 4.0,
                height: 2.0,
            }],
            position: JsonVec2 {
                x: pos.x,
                y: pos.y,
            },
            rotation,
            mirror,
            blocking: true,
            height: 1.0,
            category: "test".to_string(),
        }
    }

    #[test]
    fn world_to_local_no_transform() {
        let piece = make_piece(0.0, Mirror::None, Vec2::new(10.0, 5.0));
        // A point 2 inches right and 1 inch up from the piece center
        let world = Vec2::new(12.0, 6.0);
        let local = world_to_local(world, &piece);
        assert!((local.x - 2.0).abs() < 1e-4);
        assert!((local.y - 1.0).abs() < 1e-4);
    }

    #[test]
    fn world_to_local_90_rotation() {
        // After 90° rotation, local +x maps to world +y.
        // So a world point that is +3 in y from center should appear at local x = +3.
        let piece = make_piece(90.0, Mirror::None, Vec2::ZERO);
        let world = Vec2::new(0.0, 3.0);
        let local = world_to_local(world, &piece);
        // local_to_world: rotate(90°) maps local (3, 0) → world (0, 3) ✓
        // world_to_local: inverse-rotate(-90°) maps world (0, 3) → local (3, 0)
        assert!((local.x - 3.0).abs() < 1e-4, "local.x={}", local.x);
        assert!(local.y.abs() < 1e-4, "local.y={}", local.y);
    }

    #[test]
    fn round_trip_rotated_mirrored() {
        let piece = make_piece(45.0, Mirror::Horizontal, Vec2::new(5.0, 5.0));
        let original = Vec2::new(2.0, -1.0);
        let world = local_to_world(original, &piece);
        let back = world_to_local(world, &piece);
        assert!((back.x - original.x).abs() < 1e-4);
        assert!((back.y - original.y).abs() < 1e-4);
    }
}
