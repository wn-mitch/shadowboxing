mod army_list;
mod events;
mod los;
mod plugins;
mod resources;
mod types;

use bevy::prelude::*;
use bevy_egui::EguiPlugin;

use events::{
    AnalysisComplete, DeleteUnit, LoadDeploymentPattern, LoadTerrainLayout, SpawnUnit,
    TriggerAnalysis, TriggerArmyListImport,
};
use plugins::{
    board::BoardPlugin,
    deployment::DeploymentPlugin,
    terrain::TerrainPlugin,
    ui::UiPlugin,
    units::UnitsPlugin,
    visibility::VisibilityPlugin,
};
use resources::{ActiveLayout, ActivePattern, BoardConfig, DeploymentPatterns, TerrainLayouts};
use types::{
    deployment::DeploymentPatternList,
    terrain::TerrainLayout,
    visibility::VisibilityState,
};

fn main() {
    App::new()
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window {
                title: "Warhammer 40k Deployment Helper".into(),
                resolution: (1400.0, 900.0).into(),
                ..default()
            }),
            ..default()
        }))
        .add_plugins(EguiPlugin)
        // Resources.
        .init_resource::<BoardConfig>()
        .init_resource::<ActiveLayout>()
        .init_resource::<ActivePattern>()
        .init_resource::<VisibilityState>()
        // Events.
        .add_event::<LoadTerrainLayout>()
        .add_event::<LoadDeploymentPattern>()
        .add_event::<TriggerAnalysis>()
        .add_event::<AnalysisComplete>()
        .add_event::<SpawnUnit>()
        .add_event::<DeleteUnit>()
        .add_event::<TriggerArmyListImport>()
        // Game plugins.
        .add_plugins((
            BoardPlugin,
            TerrainPlugin,
            DeploymentPlugin,
            UnitsPlugin,
            VisibilityPlugin,
            UiPlugin,
        ))
        .add_systems(Startup, load_static_data)
        .run();
}

/// Load JSON data at startup using include_str! (embedded at compile time).
fn load_static_data(
    mut commands: Commands,
    mut ev_load_layout: EventWriter<LoadTerrainLayout>,
    mut ev_load_pattern: EventWriter<LoadDeploymentPattern>,
) {
    // Terrain layouts.
    let layout: TerrainLayout = serde_json::from_str(include_str!(
        "../assets/terrain-layouts/gw/layout-1.json"
    ))
    .expect("Failed to parse layout-1.json");

    let layout_id = layout.id.clone();
    commands.insert_resource(TerrainLayouts(vec![layout]));

    // Deployment patterns.
    let patterns: DeploymentPatternList = serde_json::from_str(include_str!(
        "../assets/deployment-patterns.json"
    ))
    .expect("Failed to parse deployment-patterns.json");

    let first_pattern_id = patterns.first().map(|p| p.id.clone());
    commands.insert_resource(DeploymentPatterns(patterns));

    // Set defaults and trigger initial load.
    commands.insert_resource(ActiveLayout(Some(layout_id.clone())));
    ev_load_layout.send(LoadTerrainLayout(layout_id));

    if let Some(pid) = first_pattern_id {
        commands.insert_resource(ActivePattern(Some(pid.clone())));
        ev_load_pattern.send(LoadDeploymentPattern(pid));
    } else {
        commands.insert_resource(ActivePattern(None));
    }
}
