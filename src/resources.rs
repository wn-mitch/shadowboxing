use bevy::prelude::*;

use crate::types::deployment::DeploymentPattern;
use crate::types::terrain::TerrainLayout;

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
