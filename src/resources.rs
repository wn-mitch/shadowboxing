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

/// Tracks which game phase is active and per-phase selection state.
#[derive(Resource)]
pub struct PhaseState {
    pub phase: GamePhase,
    /// 1-based turn counter; increments when Fight ends.
    pub turn_number: u32,
    /// Which player's turn is currently active (set on lock, toggled at end of Fight).
    pub active_player: Player,

    // Shooting
    pub selected_shooter: Option<Entity>,
    pub selected_weapon_idx: Option<usize>,
    /// Enemy clicked, awaiting kill/cancel confirmation.
    pub pending_target: Option<Entity>,
    /// Standalone range ring showing the selected weapon's reach from the shooter's base edge.
    pub shooter_range_ring: Option<Entity>,

    // Charge
    pub declared_charger: Option<Entity>,
    pub declared_charge_target: Option<Entity>,
    /// `Some(true)` = success declared, `Some(false)` = failure declared.
    pub charge_declared: Option<bool>,
    /// Standalone charge range ring entity spawned when charger is selected.
    pub charge_ring_entity: Option<Entity>,

    // Fight
    pub pending_kill_target: Option<Entity>,

    /// Set by UI "Kill it" / "Confirm Kill" buttons; processed next frame by confirm_kills system.
    pub confirmed_kill: Option<Entity>,
    /// Set by UI "Mark performing action" button.
    pub confirm_action: Option<Entity>,
}

impl Default for PhaseState {
    fn default() -> Self {
        Self {
            phase: GamePhase::default(),
            turn_number: 0,
            active_player: Player::Attacker,
            selected_shooter: None,
            selected_weapon_idx: None,
            pending_target: None,
            shooter_range_ring: None,
            declared_charger: None,
            declared_charge_target: None,
            charge_declared: None,
            charge_ring_entity: None,
            pending_kill_target: None,
            confirmed_kill: None,
            confirm_action: None,
        }
    }
}

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
