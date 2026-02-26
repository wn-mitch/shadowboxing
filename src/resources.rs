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
