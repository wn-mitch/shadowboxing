use bevy::prelude::*;
use serde::Deserialize;

use crate::types::terrain::JsonVec2;
use crate::types::units::Player;

#[derive(Debug, Clone, Deserialize, Resource)]
pub struct DeploymentPattern {
    pub id: String,
    pub name: String,
    pub source: String,
    pub description: String,
    pub zones: Vec<DeploymentZone>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct DeploymentZone {
    pub player: ZonePlayer,
    pub name: String,
    pub shape: ZoneShape,
    pub position: JsonVec2,
    pub color: String,
}

impl DeploymentZone {
    pub fn world_offset(&self) -> Vec2 {
        use crate::types::terrain::BOARD_HEIGHT;
        Vec2::new(self.position.x, BOARD_HEIGHT - self.position.y)
    }

    pub fn to_player(&self) -> Player {
        match self.player {
            ZonePlayer::Attacker => Player::Defender,
            ZonePlayer::Defender => Player::Attacker,
        }
    }

    /// Return all polygon vertices in world space.
    pub fn world_vertices(&self) -> Vec<Vec2> {
        let offset = self.world_offset();
        match &self.shape {
            ZoneShape::Rectangle { width, height } => vec![
                offset,
                offset + Vec2::new(*width, 0.0),
                offset + Vec2::new(*width, -*height),
                offset + Vec2::new(0.0, -*height),
            ],
            ZoneShape::Polygon { points } => {
                points.iter().map(|p| offset + Vec2::new(p.x, -p.y)).collect()
            }
        }
    }

    /// Sample points along the polygon perimeter at `spacing` inch intervals.
    pub fn sample_perimeter(&self, spacing: f32) -> Vec<Vec2> {
        let verts = self.world_vertices();
        let n = verts.len();
        let mut pts = Vec::new();
        for i in 0..n {
            let a = verts[i];
            let b = verts[(i + 1) % n];
            let edge_len = (b - a).length();
            let steps = (edge_len / spacing).ceil() as usize;
            for s in 0..steps {
                pts.push(a.lerp(b, s as f32 / steps as f32));
            }
        }
        pts
    }

    /// Sample interior points at `spacing` inch grid.
    pub fn sample_interior(&self, spacing: f32) -> Vec<Vec2> {
        let verts = self.world_vertices();
        let (min_x, min_y, max_x, max_y) = bounding_box(&verts);
        let mut pts = Vec::new();
        let mut y = min_y + spacing / 2.0;
        while y <= max_y {
            let mut x = min_x + spacing / 2.0;
            while x <= max_x {
                let p = Vec2::new(x, y);
                if point_in_polygon(p, &verts) {
                    pts.push(p);
                }
                x += spacing;
            }
            y += spacing;
        }
        pts
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ZonePlayer {
    Attacker,
    Defender,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum ZoneShape {
    Rectangle { width: f32, height: f32 },
    Polygon { points: Vec<JsonVec2> },
}

fn bounding_box(verts: &[Vec2]) -> (f32, f32, f32, f32) {
    let mut min_x = f32::MAX;
    let mut min_y = f32::MAX;
    let mut max_x = f32::MIN;
    let mut max_y = f32::MIN;
    for v in verts {
        min_x = min_x.min(v.x);
        min_y = min_y.min(v.y);
        max_x = max_x.max(v.x);
        max_y = max_y.max(v.y);
    }
    (min_x, min_y, max_x, max_y)
}

/// Ray-casting point-in-polygon test.
/// Public re-export for the units plugin.
pub fn point_in_polygon_pub(p: Vec2, verts: &[Vec2]) -> bool {
    point_in_polygon(p, verts)
}

fn point_in_polygon(p: Vec2, verts: &[Vec2]) -> bool {
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

/// ECS marker component for deployment zone overlay entities.
#[derive(Component, Debug, Clone)]
pub struct DeploymentZoneMarker {
    pub player: Player,
}

/// Parsed JSON wrapper for `include_str!` deserialization.
pub type DeploymentPatternList = Vec<DeploymentPattern>;

#[cfg(test)]
mod tests {
    use super::*;

    fn make_rect_zone(width: f32, height: f32) -> DeploymentZone {
        DeploymentZone {
            player: ZonePlayer::Attacker,
            name: "test".to_string(),
            shape: ZoneShape::Rectangle { width, height },
            position: JsonVec2 { x: 0.0, y: 0.0 },
            color: "#ffffff".to_string(),
        }
    }

    #[test]
    fn sample_perimeter_4x4_square() {
        // 4×4 square at origin (world_offset = (0, BOARD_HEIGHT)).
        // Perimeter = 4 edges × 4 units = 16 units.
        // At spacing 1.0 → ceil(4/1)=4 steps per edge → 16 points total.
        let zone = make_rect_zone(4.0, 4.0);
        let pts = zone.sample_perimeter(1.0);
        assert_eq!(pts.len(), 16, "Expected 16 perimeter points, got {}", pts.len());
    }

    #[test]
    fn sample_perimeter_no_interior_points() {
        let zone = make_rect_zone(4.0, 4.0);
        let verts = zone.world_vertices();
        let pts = zone.sample_perimeter(0.5);
        // All points should lie on or very near the boundary (within floating-point tolerance).
        for pt in &pts {
            let on_edge = verts.windows(2).any(|w| {
                point_on_segment(*pt, w[0], w[1])
            }) || point_on_segment(*pt, *verts.last().unwrap(), verts[0]);
            assert!(on_edge, "Point {:?} is not on any edge", pt);
        }
    }

    fn point_on_segment(p: Vec2, a: Vec2, b: Vec2) -> bool {
        let ab = b - a;
        let ap = p - a;
        let cross = ab.x * ap.y - ab.y * ap.x;
        if cross.abs() > 1e-4 {
            return false;
        }
        let t = if ab.x.abs() > ab.y.abs() { ap.x / ab.x } else { ap.y / ab.y };
        t >= -1e-5 && t <= 1.0 + 1e-5
    }
}
