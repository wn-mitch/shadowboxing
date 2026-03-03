use bevy::prelude::*;
use geo::MultiPolygon;

use crate::los::CandidateInfo;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AnalysisMode {
    /// Zone Coverage: union of visibility from all points in opponent's deployment zone.
    ZoneCoverage,
    /// Unit Positions: visibility from each opponent base, optionally expanded by movement.
    #[default]
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

/// ECS marker for debug dots showing LOS source points.
#[derive(Component)]
pub struct SourcePointMarker;

/// Index into the per-source data vec; attached to each SourcePointMarker at spawn time.
#[derive(Component)]
pub struct SourceIndex(pub usize);

/// Ray endpoints from visibility_polygon; attached to a source dot after analysis completes.
#[derive(Component)]
pub struct SourceRayVerts {
    pub source: Vec2,
    pub endpoints: Vec<Vec2>,
}

/// Which source dot (if any) is selected for ray display.
#[derive(Resource, Default)]
pub struct SelectedSourceEntity(pub Option<Entity>);

/// Which unit (if any) is selected for per-unit analysis fade.
#[derive(Resource, Default)]
pub struct SelectedUnitForAnalysis(pub Option<Entity>);

/// ECS marker for green candidate position dots.
#[derive(Component)]
pub struct CandidatePointMarker;

/// Which candidate index a green dot represents.
#[derive(Component)]
pub struct CandidateIndex(pub usize);

/// Which stage of the UnitPositions analysis flow we're in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum UnitAnalysisStage {
    #[default]
    Idle,
    /// Green candidate dots are visible; waiting for the user to select one.
    SelectCandidate,
    /// A candidate was clicked; yellow source dots for that candidate are visible.
    SelectSource,
}

/// Holds cached candidate data for the staged UnitPositions flow.
#[derive(Resource, Default)]
pub struct UnitAnalysisState {
    pub stage: UnitAnalysisStage,
    /// All valid candidate positions for the current analysis run.
    pub candidates: Vec<CandidateInfo>,
}

/// Which candidate index (if any) the user has selected.
#[derive(Resource, Default)]
pub struct SelectedCandidate(pub Option<usize>);
