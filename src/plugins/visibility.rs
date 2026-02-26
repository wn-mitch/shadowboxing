use bevy::prelude::*;
use bevy::tasks::{AsyncComputeTaskPool, Task};
use futures_lite::future;

use crate::events::{AnalysisComplete, TriggerAnalysis};
use crate::los::{run_analysis, sample_zone_sources, unit_sources};
use crate::resources::{ActiveLayout, ActivePattern, DeploymentPatterns, TerrainLayouts};
use crate::types::units::{Player, UnitBase};
use crate::types::visibility::{AnalysisMode, DangerRegionMesh, VisibilityState};

pub struct VisibilityPlugin;

impl Plugin for VisibilityPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            (trigger_analysis, poll_analysis_task, on_analysis_complete),
        );
    }
}

/// Component holding the in-flight analysis task.
#[derive(Component)]
struct AnalysisTask(Task<geo::MultiPolygon<f64>>);

fn trigger_analysis(
    mut commands: Commands,
    mut events: EventReader<TriggerAnalysis>,
    mut vis_state: ResMut<VisibilityState>,
    layouts: Res<TerrainLayouts>,
    active_layout: Res<ActiveLayout>,
    patterns: Res<DeploymentPatterns>,
    active_pattern: Res<ActivePattern>,
    unit_bases: Query<(&Transform, &UnitBase)>,
) {
    for ev in events.read() {
        if vis_state.analyzing {
            continue; // Don't stack analyses.
        }
        vis_state.analyzing = true;
        vis_state.mode = ev.0;

        let pieces = active_layout
            .0
            .as_ref()
            .and_then(|id| layouts.0.iter().find(|l| &l.id == id))
            .map(|l| l.pieces.clone())
            .unwrap_or_default();

        let sources: Vec<Vec2> = match ev.0 {
            AnalysisMode::ZoneCoverage => {
                // Sample the opponent (attacker) deployment zone.
                active_pattern
                    .0
                    .as_ref()
                    .and_then(|id| patterns.0.iter().find(|p| &p.id == id))
                    .and_then(|pat| pat.zones.iter().find(|z| z.to_player() == Player::Attacker))
                    .map(|z| sample_zone_sources(z))
                    .unwrap_or_default()
            }
            AnalysisMode::UnitPositions => {
                // Use all attacker unit bases.
                unit_bases
                    .iter()
                    .filter(|(_, ub)| ub.player == Player::Attacker)
                    .map(|(t, ub)| (t.translation.truncate(), ub.movement_inches.unwrap_or(0.0)))
                    .flat_map(|(pos, m)| unit_sources(&[(pos, m)], m))
                    .collect()
            }
        };

        let task_pool = AsyncComputeTaskPool::get();
        let task = task_pool.spawn(async move { run_analysis(sources, &pieces) });
        commands.spawn(AnalysisTask(task));
    }
}

fn poll_analysis_task(
    mut commands: Commands,
    mut tasks: Query<(Entity, &mut AnalysisTask)>,
    mut analysis_complete: EventWriter<AnalysisComplete>,
) {
    for (entity, mut task) in tasks.iter_mut() {
        if let Some(result) = future::block_on(future::poll_once(&mut task.0)) {
            analysis_complete.send(AnalysisComplete(result));
            commands.entity(entity).despawn();
        }
    }
}

fn on_analysis_complete(
    mut commands: Commands,
    mut events: EventReader<AnalysisComplete>,
    mut vis_state: ResMut<VisibilityState>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<ColorMaterial>>,
    existing: Query<Entity, With<DangerRegionMesh>>,
) {
    for ev in events.read() {
        vis_state.analyzing = false;
        vis_state.danger_area_sq_in = crate::los::multi_polygon_area(&ev.0);
        vis_state.danger_region = Some(ev.0.clone());

        // Remove old mesh.
        for e in existing.iter() {
            commands.entity(e).despawn_recursive();
        }

        // Build new mesh from triangulated MultiPolygon.
        let (positions, indices) = crate::los::triangulate_multi_polygon(&ev.0);
        if positions.is_empty() {
            continue;
        }

        let mesh = build_mesh_from_triangles(positions, indices);
        let danger_color = Color::srgba(0.85, 0.1, 0.1, 0.4);

        commands.spawn((
            Mesh2d(meshes.add(mesh)),
            MeshMaterial2d(materials.add(ColorMaterial::from_color(danger_color))),
            Transform::from_xyz(0.0, 0.0, 3.0),
            DangerRegionMesh,
        ));
    }
}

fn build_mesh_from_triangles(positions: Vec<[f32; 3]>, indices: Vec<u32>) -> Mesh {
    use bevy::render::mesh::{Indices, PrimitiveTopology};
    use bevy::render::render_asset::RenderAssetUsages;

    let normals: Vec<[f32; 3]> = vec![[0.0, 0.0, 1.0]; positions.len()];
    let uvs: Vec<[f32; 2]> = positions.iter().map(|p| [p[0], p[1]]).collect();

    let mut mesh = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::RENDER_WORLD);
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
    mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, normals);
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, uvs);
    mesh.insert_indices(Indices::U32(indices));
    mesh
}
