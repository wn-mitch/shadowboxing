use bevy::prelude::*;
use serde::{Deserialize, Serialize};

/// Shape of a model's physical base, parsed from base_size strings.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum BaseShape {
    Circle { diameter_mm: f32 },
    Oval { width_mm: f32, height_mm: f32 },
    /// Vehicles — rendered as 60×35mm oval
    Hull,
    FlyingBase { large: bool },
    /// "Unique" or unrecognized — default 32mm
    Unknown,
}

impl BaseShape {
    /// Half-width in inches (x semi-axis).
    pub fn radius_x_inches(&self) -> f32 {
        match self {
            BaseShape::Circle { diameter_mm } => diameter_mm / 25.4 / 2.0,
            BaseShape::Oval { width_mm, .. } => width_mm / 25.4 / 2.0,
            BaseShape::Hull => 60.0 / 25.4 / 2.0,
            BaseShape::FlyingBase { large: true } => 60.0 / 25.4 / 2.0,
            BaseShape::FlyingBase { large: false } | BaseShape::Unknown => 32.0 / 25.4 / 2.0,
        }
    }

    /// Half-height in inches (y semi-axis).
    pub fn radius_y_inches(&self) -> f32 {
        match self {
            BaseShape::Circle { diameter_mm } => diameter_mm / 25.4 / 2.0,
            BaseShape::Oval { height_mm, .. } => height_mm / 25.4 / 2.0,
            BaseShape::Hull => 35.0 / 25.4 / 2.0,
            BaseShape::FlyingBase { large: true } => 60.0 / 25.4 / 2.0,
            BaseShape::FlyingBase { large: false } | BaseShape::Unknown => 32.0 / 25.4 / 2.0,
        }
    }

    pub fn is_circular(&self) -> bool {
        match self {
            BaseShape::Circle { .. }
            | BaseShape::FlyingBase { .. }
            | BaseShape::Unknown => true,
            BaseShape::Oval { width_mm, height_mm } => (width_mm - height_mm).abs() < 0.01,
            BaseShape::Hull => false,
        }
    }

    /// Human-readable label for the UI.
    pub fn label(&self) -> String {
        match self {
            BaseShape::Circle { diameter_mm } => format!("{diameter_mm}mm"),
            BaseShape::Oval { width_mm, height_mm } => {
                format!("{width_mm}×{height_mm}mm Oval")
            }
            BaseShape::Hull => "Hull".to_string(),
            BaseShape::FlyingBase { large: true } => "Large Flying Base".to_string(),
            BaseShape::FlyingBase { large: false } => "Small Flying Base".to_string(),
            BaseShape::Unknown => "32mm (Unknown)".to_string(),
        }
    }
}

/// Player side.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Component)]
pub enum Player {
    Attacker,
    Defender,
}

impl Player {
    pub fn label(&self) -> &'static str {
        match self {
            Player::Attacker => "Attacker",
            Player::Defender => "Defender",
        }
    }

    pub fn other(&self) -> Player {
        match self {
            Player::Attacker => Player::Defender,
            Player::Defender => Player::Attacker,
        }
    }
}

/// ECS component on each unit base entity.
#[derive(Component, Debug, Clone)]
pub struct UnitBase {
    pub unit_name: String,
    pub model_name: String,
    pub base_shape: BaseShape,
    pub locked: bool,
    pub movement_inches: Option<f32>,
    pub player: Player,
    pub color: Color,
    /// Last valid world position (used for snap-back on invalid placement).
    pub last_valid_pos: Vec2,
}

/// A spawned army unit ready to be placed on the board.
#[derive(Debug, Clone)]
pub struct ArmyUnit {
    pub unit_name: String,
    pub model_name: String,
    pub count: u32,
    pub base_shape: BaseShape,
    pub movement_inches: Option<f32>,
    pub color: Color,
    pub player: Player,
}
