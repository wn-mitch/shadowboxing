use bevy::prelude::*;

use crate::types::deployment::DeploymentPattern;
use crate::types::phase::GamePhase;
use crate::types::terrain::TerrainLayout;
use crate::types::units::Player;

#[derive(Resource)]
pub struct BoardConfig {
    pub width: f32,
    pub height: f32,
}

impl Default for BoardConfig {
    fn default() -> Self {
        BoardConfig {
            width: 60.0,
            height: 44.0,
        }
    }
}

#[derive(Resource, Default)]
pub struct TerrainLayouts(pub Vec<TerrainLayout>);

#[derive(Resource, Default)]
pub struct ActiveLayout(pub Option<String>);

#[derive(Resource, Default)]
pub struct DeploymentPatterns(pub Vec<DeploymentPattern>);

#[derive(Resource, Default)]
pub struct ActivePattern(pub Option<String>);

#[derive(Resource)]
pub struct PanelWidth(pub f32);

impl Default for PanelWidth {
    fn default() -> Self {
        Self(256.0)
    }
}

#[derive(Resource)]
pub struct RightPanelWidth(pub f32);

impl Default for RightPanelWidth {
    fn default() -> Self {
        Self(220.0)
    }
}

/// Tracks game phase sequencing — no per-tool selection state.
#[derive(Resource)]
pub struct PhaseState {
    pub phase: GamePhase,
    /// 1-based turn counter; increments when Fight ends.
    pub turn_number: u32,
    /// Which player's turn is currently active (set on lock, toggled at end of Fight).
    pub active_player: Player,
}

impl Default for PhaseState {
    fn default() -> Self {
        Self {
            phase: GamePhase::default(),
            turn_number: 0,
            active_player: Player::Attacker,
        }
    }
}

// ── Per-tool state resources ─────────────────────────────────────────────────

#[derive(Resource, Default)]
pub struct KillToolState {
    pub pending_target: Option<Entity>,
}

#[derive(Resource, Default)]
pub struct ShootToolState {
    pub selected_shooter: Option<Entity>,
    pub selected_weapon_idx: Option<usize>,
    pub pending_target: Option<Entity>,
}

#[derive(Resource, Default)]
pub struct ChargeToolState {
    pub declared_charger: Option<Entity>,
    pub charge_targets: Vec<Entity>,
    pub charge_declared: Option<bool>,
}

#[derive(Resource, Default)]
pub struct MeasureToolState {
    pub start_point: Option<Vec2>,
}

#[derive(Resource, Default)]
pub struct RangeRingToolState {
    pub radius_input: String,
    pub selected_unit: Option<Entity>,
}

#[derive(Resource, Default)]
pub struct BattleshockToolState {
    pub pending_target: Option<Entity>,
}

#[derive(Resource, Default)]
pub struct EnterReservesToolState {
    pub pending: Option<Entity>,
}

#[derive(Resource, Default)]
pub struct DeployReservesToolState {
    pub selected_reserve: Option<Entity>,
}

/// When true, movement tools enforce the unit's M (or M+6 for Advance) as a
/// cumulative path-distance cap.
#[derive(Resource, Default)]
pub struct EnforceMaxMove(pub bool);

#[derive(Resource)]
pub struct OverlaySettings {
    pub show_source_points: bool,
    pub show_danger_region: bool,
    pub show_deployment_zones: bool,
    pub show_validity_rings: bool,
    pub show_terrain_debug: bool,
    pub show_collision_boxes: bool,
}

impl Default for OverlaySettings {
    fn default() -> Self {
        Self {
            show_source_points: false,
            show_danger_region: false,
            show_deployment_zones: true,
            show_validity_rings: false,
            show_terrain_debug: false,
            show_collision_boxes: false,
        }
    }
}
