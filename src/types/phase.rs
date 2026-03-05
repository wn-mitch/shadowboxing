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

    /// Tools available in this phase.
    pub fn available_tools(self) -> &'static [ActiveTool] {
        match self {
            Self::Command => &[ActiveTool::Select, ActiveTool::Battleshock],
            Self::Movement => &[
                ActiveTool::Select,
                ActiveTool::Move,
                ActiveTool::Advance,
                ActiveTool::FallBack,
                ActiveTool::Reactive,
                ActiveTool::DeployReserves,
                ActiveTool::Kill,
            ],
            Self::Shooting => &[
                ActiveTool::Select,
                ActiveTool::ShootAnnotate,
                ActiveTool::PerformAction,
                ActiveTool::Reactive,
                ActiveTool::Kill,
            ],
            Self::Charge => &[
                ActiveTool::Select,
                ActiveTool::Charge,
                ActiveTool::Reactive,
                ActiveTool::Kill,
            ],
            Self::Fight => &[
                ActiveTool::Select,
                ActiveTool::PileIn,
                ActiveTool::Kill,
                ActiveTool::Consolidate,
                ActiveTool::Reactive,
                ActiveTool::EnterReserves,
            ],
        }
    }

    /// The tool automatically selected when entering this phase.
    pub fn default_tool(self) -> ActiveTool {
        match self {
            Self::Command => ActiveTool::Select,
            Self::Movement => ActiveTool::Move,
            Self::Shooting => ActiveTool::ShootAnnotate,
            Self::Charge => ActiveTool::Charge,
            Self::Fight => ActiveTool::PileIn,
        }
    }
}

#[derive(States, Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum ActiveTool {
    #[default]
    Select,
    // Movement tools
    Move,
    Advance,
    FallBack,
    Reactive,
    PileIn,
    Consolidate,
    Charge,
    // Annotation tools
    ShootAnnotate,
    Measure,
    RangeRing,
    // Action tools
    Kill,
    PerformAction,
    Battleshock,
    DeployReserves,
    EnterReserves,
}

impl ActiveTool {
    pub fn label(self) -> &'static str {
        match self {
            Self::Select => "Select",
            Self::Move => "Move",
            Self::Advance => "Advance",
            Self::FallBack => "Fall Back",
            Self::Reactive => "Reactive",
            Self::PileIn => "Pile In",
            Self::Consolidate => "Consolidate",
            Self::Charge => "Charge",
            Self::ShootAnnotate => "Shoot",
            Self::Measure => "Measure",
            Self::RangeRing => "Range Ring",
            Self::Kill => "Kill",
            Self::PerformAction => "Action",
            Self::Battleshock => "Battleshock",
            Self::DeployReserves => "Deploy Reserves",
            Self::EnterReserves => "Enter Reserves",
        }
    }

    pub fn is_movement_tool(self) -> bool {
        matches!(
            self,
            Self::Move
                | Self::Advance
                | Self::FallBack
                | Self::Reactive
                | Self::PileIn
                | Self::Consolidate
                | Self::Charge
        )
    }
}
