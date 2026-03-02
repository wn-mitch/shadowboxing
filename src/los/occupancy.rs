use bevy::math::Vec2;
use std::collections::HashSet;

use crate::los::shapes::point_in_shape;
use crate::types::terrain::{TerrainPiece, TerrainShape};

/// Return IDs of blocking pieces whose footprint (shapes[0]) contains `point`.
/// Lines are never footprints — they're walls only.
pub fn get_terrain_occupancy<'a>(
    point: Vec2,
    pieces: &'a [TerrainPiece],
) -> HashSet<&'a str> {
    pieces
        .iter()
        .filter(|p| {
            p.blocking
                && matches!(
                    p.shapes.first(),
                    Some(
                        TerrainShape::Rectangle { .. }
                            | TerrainShape::Polygon { .. }
                            | TerrainShape::Circle { .. }
                    )
                )
        })
        .filter(|p| point_in_shape(point, &p.shapes[0], p))
        .map(|p| p.id.as_str())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::terrain::{JsonVec2, Mirror, TerrainShape};

    fn rect_piece(id: &str, bevy_pos: Vec2, w: f32, h: f32) -> TerrainPiece {
        use crate::types::terrain::BOARD_HEIGHT;
        TerrainPiece {
            id: id.to_string(),
            name: id.to_string(),
            shapes: vec![TerrainShape::Rectangle { width: w, height: h, x: 0.0, y: 0.0 }],
            position: JsonVec2 { x: bevy_pos.x, y: BOARD_HEIGHT - bevy_pos.y },
            rotation: 0.0,
            mirror: Mirror::None,
            blocking: true,
            height: 1.0,
            category: String::new(),
        }
    }

    #[test]
    fn point_inside_footprint() {
        let pieces = [rect_piece("a", Vec2::new(10.0, 10.0), 4.0, 4.0)];
        let occ = get_terrain_occupancy(Vec2::new(10.0, 10.0), &pieces);
        assert!(occ.contains("a"));
    }

    #[test]
    fn point_outside_footprint() {
        let pieces = [rect_piece("a", Vec2::new(10.0, 10.0), 4.0, 4.0)];
        let occ = get_terrain_occupancy(Vec2::new(20.0, 20.0), &pieces);
        assert!(occ.is_empty());
    }

    #[test]
    fn line_shapes_excluded_from_occupancy() {
        use crate::types::terrain::TerrainShape;
        let pieces = [TerrainPiece {
            id: "wall".to_string(),
            name: "Wall".to_string(),
            shapes: vec![TerrainShape::Line {
                start: JsonVec2 { x: 0.0, y: 0.0 },
                end: JsonVec2 { x: 10.0, y: 0.0 },
                thickness: 1.0,
            }],
            position: JsonVec2 { x: 0.0, y: 0.0 },
            rotation: 0.0,
            mirror: Mirror::None,
            blocking: true,
            height: 1.0,
            category: String::new(),
        }];
        let occ = get_terrain_occupancy(Vec2::new(5.0, 0.0), &pieces);
        assert!(occ.is_empty(), "Line shapes should never provide occupancy");
    }
}
