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
