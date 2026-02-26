use bevy::prelude::*;
use geo::MultiPolygon;

use crate::types::units::{BaseShape, Player};
use crate::types::visibility::AnalysisMode;

/// Triggers loading a terrain layout by ID, despawning the current one.
#[derive(Event, Debug, Clone)]
pub struct LoadTerrainLayout(pub String);

/// Triggers loading a deployment pattern by ID.
#[derive(Event, Debug, Clone)]
pub struct LoadDeploymentPattern(pub String);

/// Kick off a visibility analysis on the async task pool.
#[derive(Event, Debug, Clone)]
pub struct TriggerAnalysis(pub AnalysisMode);

/// Result event from the background analysis task.
#[derive(Event, Debug, Clone)]
pub struct AnalysisComplete(pub MultiPolygon<f64>);

/// Spawn unit bases on the board.
#[derive(Event, Debug, Clone)]
pub struct SpawnUnit {
    pub unit_name: String,
    pub model_name: String,
    pub base_shape: BaseShape,
    pub count: u32,
    pub color: Color,
    pub movement_inches: Option<f32>,
    pub player: Player,
}

/// Delete a specific unit base entity.
#[derive(Event, Debug, Clone)]
pub struct DeleteUnit(pub Entity);

/// Import an army list.
#[derive(Event, Debug, Clone)]
pub struct TriggerArmyListImport(pub String);
