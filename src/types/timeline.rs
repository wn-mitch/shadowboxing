use std::collections::HashMap;

use bevy::prelude::*;

use crate::types::units::Player;

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum FirstPlayer {
    #[default]
    Attacker,
    Defender,
}

pub struct TimelineSnapshot {
    pub label: String,
    /// Which player's half-turn this snapshot represents (`None` for Deployment).
    pub player: Option<Player>,
    /// Per-unit: (world position, advanced flag).
    pub positions: HashMap<Entity, (Vec2, bool)>,
    /// Arrow entities that visualise this transition.
    pub arrow_entities: Vec<Entity>,
}

#[derive(Resource, Default)]
pub struct GameTimeline {
    pub locked: bool,
    pub first_player: FirstPlayer,
    pub snapshots: Vec<TimelineSnapshot>,
    /// `snapshots.len()` means "live view".
    pub current_index: usize,
    /// unit entity → live arrow entities for the current phase (multi-segment).
    pub live_arrows: HashMap<Entity, Vec<Entity>>,
    /// unit entity → cumulative path distance across all live segments.
    pub live_cumulative_distance: HashMap<Entity, f32>,
    /// Tail of live arrows — positions at phase start.
    pub phase_start_positions: HashMap<Entity, Vec2>,
    /// Current live positions (used to restore after leaving historical view).
    pub live_unit_positions: HashMap<Entity, Vec2>,
    /// unit entity → ghost entity (semi-transparent phase-start marker).
    pub ghost_entities: HashMap<Entity, Entity>,
    /// unit entity → [normal_ring, advance_ring] standalone entities.
    pub ring_entities: HashMap<Entity, [Entity; 2]>,
}

impl GameTimeline {
    /// Returns which player's turn is active in the current live phase.
    /// Returns `None` if not yet locked.
    pub fn active_player_in_live_view(&self) -> Option<Player> {
        if !self.locked {
            return None;
        }
        // Number of half-turns completed (excludes the Deployment snapshot).
        let phase_count = self.snapshots.len().saturating_sub(1);
        let (first, second) = match self.first_player {
            FirstPlayer::Attacker => (Player::Attacker, Player::Defender),
            FirstPlayer::Defender => (Player::Defender, Player::Attacker),
        };
        Some(if phase_count % 2 == 0 { first } else { second })
    }
}

/// Type of movement — determines arrow color and behavior.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum MoveType {
    #[default]
    Normal,
    Advance,
    FallBack,
    Reactive,
    PileIn,
    Consolidate,
    Charge,
}

impl MoveType {
    pub fn color(self) -> Color {
        match self {
            Self::Normal => Color::srgba(0.2, 0.9, 0.2, 0.75),       // green
            Self::Advance => Color::srgba(1.0, 0.6, 0.0, 0.75),      // orange
            Self::FallBack => Color::srgba(0.9, 0.2, 0.2, 0.75),     // red
            Self::Reactive => Color::srgba(1.0, 1.0, 0.2, 0.75),     // yellow
            Self::PileIn => Color::srgba(0.6, 0.2, 0.9, 0.75),       // purple
            Self::Consolidate => Color::srgba(0.2, 0.9, 0.9, 0.75),  // cyan
            Self::Charge => Color::srgba(1.0, 0.5, 0.0, 0.75),       // orange
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Normal => "Move",
            Self::Advance => "Advance",
            Self::FallBack => "Fall Back",
            Self::Reactive => "Reactive",
            Self::PileIn => "Pile In",
            Self::Consolidate => "Consolidate",
            Self::Charge => "Charge",
        }
    }

    pub fn is_dashed(self) -> bool {
        matches!(self, Self::Charge)
    }
}

/// Root entity for a movement arrow drawn from `from` to `to`.
#[derive(Component)]
pub struct MovementArrow {
    pub unit: Entity,
    pub from: Vec2,
    pub to: Vec2,
    pub move_type: MoveType,
}

/// Semi-transparent ghost that marks a unit's phase-start position.
#[derive(Component)]
pub struct GhostUnit {
    pub unit: Entity,
}

/// Marker for standalone range-ring entities anchored to a unit's phase-start position.
#[derive(Component)]
pub struct MovementRangeRing;

/// Marker for the "ADV" text badge child on a unit base.
#[derive(Component)]
pub struct AdvanceIndicator;

/// Standalone ring entity showing the 12" charge range for a selected charger.
#[derive(Component)]
pub struct ChargeRangeRing;

/// Standalone ring entity showing the selected weapon's range from the shooter's base edge.
#[derive(Component)]
pub struct ShooterRangeRing;

/// Persistent range ring that follows a unit.
#[derive(Component)]
pub struct PersistentRangeRing {
    pub unit: Entity,
    pub radius: f32,
}

/// Annotation line from shooter to target.
#[derive(Component)]
pub struct ShootAnnotation {
    pub shooter: Entity,
    pub target: Entity,
    pub weapon_name: String,
    pub distance: f32,
}

/// Marker for ephemeral measure tool entities.
#[derive(Component)]
pub struct MeasureMarker;
