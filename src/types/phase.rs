use bevy::prelude::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Resource)]
pub enum GamePhase {
    #[default]
    Command,
    Movement,
    Shooting,
    Charge,
    Fight,
}

impl GamePhase {
    pub fn label(self) -> &'static str {
        match self {
            Self::Command => "Command",
            Self::Movement => "Movement",
            Self::Shooting => "Shooting",
            Self::Charge => "Charge",
            Self::Fight => "Fight",
        }
    }

    /// Returns the next phase, or `None` when the turn ends (after Fight).
    pub fn next(self) -> Option<Self> {
        match self {
            Self::Command => Some(Self::Movement),
            Self::Movement => Some(Self::Shooting),
            Self::Shooting => Some(Self::Charge),
            Self::Charge => Some(Self::Fight),
            Self::Fight => None,
        }
    }

    /// True when dragging units is allowed.
    pub fn drag_allowed(self) -> bool {
        matches!(self, Self::Movement | Self::Charge)
    }
}
