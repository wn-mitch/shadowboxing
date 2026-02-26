use bevy::math::Vec2;
use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TerrainLayout {
    pub id: String,
    pub name: String,
    pub source: String,
    pub board_width: f32,
    pub board_height: f32,
    pub pieces: Vec<TerrainPiece>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TerrainPiece {
    pub id: String,
    pub name: String,
    pub shapes: Vec<TerrainShape>,
    pub position: JsonVec2,
    #[serde(default)]
    pub rotation: f32,
    #[serde(default)]
    pub mirror: Mirror,
    pub blocking: bool,
    #[serde(default)]
    pub height: f32,
    #[serde(default)]
    pub category: String,
}

impl TerrainPiece {
    pub fn world_position(&self) -> Vec2 {
        Vec2::new(self.position.x, self.position.y)
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum TerrainShape {
    Rectangle {
        width: f32,
        height: f32,
    },
    Polygon {
        points: Vec<JsonVec2>,
    },
    Circle {
        radius: f32,
    },
    Line {
        start: JsonVec2,
        end: JsonVec2,
        thickness: f32,
    },
}

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum Mirror {
    #[default]
    None,
    Horizontal,
    Vertical,
}

/// JSON vec2 helper — serde can't directly deserialize bevy's Vec2 from {x, y}.
#[derive(Debug, Clone, Copy, Deserialize)]
pub struct JsonVec2 {
    pub x: f32,
    pub y: f32,
}

impl From<JsonVec2> for Vec2 {
    fn from(v: JsonVec2) -> Self {
        Vec2::new(v.x, v.y)
    }
}
