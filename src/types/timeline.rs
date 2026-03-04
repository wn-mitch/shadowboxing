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
    /// unit entity → live arrow entity for the current phase.
    pub live_arrows: HashMap<Entity, Entity>,
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

/// Root entity for a movement arrow drawn from `from` to `to`.
#[derive(Component)]
pub struct MovementArrow {
    pub unit: Entity,
    pub from: Vec2,
    pub to: Vec2,
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
