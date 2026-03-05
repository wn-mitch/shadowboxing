mod army_list;
mod events;
mod los;
mod plugins;
mod resources;
mod types;

use bevy::prelude::*;
use bevy_egui::EguiPlugin;

use army_list::base_lookup::BaseDatabase;
use events::{
    AnalysisComplete, DeleteUnit, LoadDeploymentPattern, LoadTerrainLayout, SpawnUnit,
    TriggerAnalysis, TriggerArmyListImport,
};
use plugins::{
    board::BoardPlugin,
    deployment::DeploymentPlugin,
    terrain::TerrainPlugin,
    timeline::TimelinePlugin,
    ui::UiPlugin,
    units::UnitsPlugin,
    visibility::VisibilityPlugin,
};
use resources::{
    ActiveLayout, ActivePattern, BattleshockToolState, BoardConfig, ChargeToolState,
    DeployReservesToolState, DeploymentPatterns, EnforceMaxMove, EnterReservesToolState,
    KillToolState, MeasureToolState, OverlaySettings, PhaseState, RangeRingToolState,
    RightPanelWidth, ShootToolState, TerrainLayouts,
};
use types::{
    deployment::DeploymentPatternList,
    phase::ActiveTool,
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
        // Tool state.
        .init_state::<ActiveTool>()
        // Resources.
        .init_resource::<BoardConfig>()
        .init_resource::<ActiveLayout>()
        .init_resource::<ActivePattern>()
        .init_resource::<VisibilityState>()
        .init_resource::<OverlaySettings>()
        .init_resource::<RightPanelWidth>()
        .init_resource::<PhaseState>()
        // Per-tool resources.
        .init_resource::<KillToolState>()
        .init_resource::<ShootToolState>()
        .init_resource::<ChargeToolState>()
        .init_resource::<MeasureToolState>()
        .init_resource::<RangeRingToolState>()
        .init_resource::<BattleshockToolState>()
        .init_resource::<EnterReservesToolState>()
        .init_resource::<DeployReservesToolState>()
        .init_resource::<EnforceMaxMove>()
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
            TimelinePlugin,
            UiPlugin,
            bevy::picking::mesh_picking::MeshPickingPlugin,
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
    // Base + weapon database.
    let base_db = BaseDatabase::load(
        include_str!("../assets/Datasheets.json"),
        include_str!("../assets/Datasheets_models.json"),
        include_str!("../assets/Datasheets_wargear.json"),
    );
    commands.insert_resource(base_db);
    // Terrain layouts.
    let layout_gw: TerrainLayout = serde_json::from_str(include_str!(
        "../assets/terrain-layouts/gw/layout-1.json"
    ))
    .expect("Failed to parse layout-1.json");
    let layout_empty: TerrainLayout = serde_json::from_str(include_str!(
        "../assets/terrain-layouts/sandbox/empty.json"
    ))
    .expect("Failed to parse sandbox/empty.json");
    let layout_ruin: TerrainLayout = serde_json::from_str(include_str!(
        "../assets/terrain-layouts/sandbox/single-ruin.json"
    ))
    .expect("Failed to parse sandbox/single-ruin.json");
    let layout_walls: TerrainLayout = serde_json::from_str(include_str!(
        "../assets/terrain-layouts/sandbox/ruin-with-walls.json"
    ))
    .expect("Failed to parse sandbox/ruin-with-walls.json");

    let layout_id = layout_gw.id.clone();
    commands.insert_resource(TerrainLayouts(vec![
        layout_gw,
        layout_empty,
        layout_ruin,
        layout_walls,
    ]));

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
