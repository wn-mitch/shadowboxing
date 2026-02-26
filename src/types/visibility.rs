use bevy::prelude::*;
use geo::MultiPolygon;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AnalysisMode {
    /// Zone Coverage: union of visibility from all points in opponent's deployment zone.
    #[default]
    ZoneCoverage,
    /// Unit Positions: visibility from each opponent base, optionally expanded by movement.
    UnitPositions,
}

/// The result of one analysis run.
#[derive(Debug, Clone, Default, Resource)]
pub struct VisibilityState {
    /// The computed danger region (visible from opponent's perspective).
    pub danger_region: Option<MultiPolygon<f64>>,
    /// Whether an analysis is currently running in the background.
    pub analyzing: bool,
    pub mode: AnalysisMode,
    /// Area of danger region in square inches.
    pub danger_area_sq_in: f64,
}

/// ECS marker for the mesh entity that renders the danger region.
#[derive(Component, Debug)]
pub struct DangerRegionMesh;
