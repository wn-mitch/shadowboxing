use bevy::prelude::*;
use bevy::tasks::{AsyncComputeTaskPool, Task};
use futures_lite::future;

use crate::events::{AnalysisComplete, ClearAnalysis, TriggerAnalysis};
use crate::los::{extract_footprint_edges, extract_solid_edges, run_analysis, sample_zone_sources, unit_sources};
use crate::resources::{ActiveLayout, ActivePattern, DeploymentPatterns, OverlaySettings, TerrainLayouts};
use crate::types::units::{Player, UnitBase};
use crate::types::visibility::{
    AnalysisMode, DangerRegionMesh, SelectedSourceEntity, SourceIndex, SourcePointMarker,
    SourceRayVerts, VisibilityState,
};

pub struct VisibilityPlugin;

impl Plugin for VisibilityPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<SelectedSourceEntity>()
            .add_event::<ClearAnalysis>()
            .add_systems(
                Update,
                (
                    trigger_analysis,
                    poll_analysis_task,
                    on_analysis_complete,
                    clear_analysis,
                    draw_selected_source_rays,
                    sync_source_point_visibility,
                    sync_danger_region_visibility,
                    draw_collision_boxes,
                ),
            );
    }
}

/// Component holding the in-flight analysis task.
#[derive(Component)]
struct AnalysisTask(Task<(geo::MultiPolygon<f64>, Vec<(Vec2, Vec<Vec2>)>)>);

fn trigger_analysis(
    mut commands: Commands,
    mut events: EventReader<TriggerAnalysis>,
    mut vis_state: ResMut<VisibilityState>,
    mut selected: ResMut<SelectedSourceEntity>,
    layouts: Res<TerrainLayouts>,
    active_layout: Res<ActiveLayout>,
    patterns: Res<DeploymentPatterns>,
    active_pattern: Res<ActivePattern>,
    unit_bases: Query<(&Transform, &UnitBase)>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<ColorMaterial>>,
    overlay_settings: Res<OverlaySettings>,
    existing_markers: Query<Entity, With<SourcePointMarker>>,
) {
    for ev in events.read() {
        if vis_state.analyzing {
            continue;
        }
        vis_state.analyzing = true;
        vis_state.mode = ev.0;
        selected.0 = None;

        let pieces = active_layout
            .0
            .as_ref()
            .and_then(|id| layouts.0.iter().find(|l| &l.id == id))
            .map(|l| l.pieces.clone())
            .unwrap_or_default();

        let sources: Vec<Vec2> = match ev.0 {
            AnalysisMode::ZoneCoverage => active_pattern
                .0
                .as_ref()
                .and_then(|id| patterns.0.iter().find(|p| &p.id == id))
                .and_then(|pat| pat.zones.iter().find(|z| z.to_player() == Player::Attacker))
                .map(|z| sample_zone_sources(z))
                .unwrap_or_default(),
            AnalysisMode::UnitPositions => unit_bases
                .iter()
                .filter(|(_, ub)| ub.player == Player::Attacker)
                .flat_map(|(t, ub)| {
                    let center = t.translation.truncate();
                    let rx = ub.base_shape.radius_x_inches();
                    let ry = ub.base_shape.radius_y_inches();
                    let movement = ub.movement_inches.unwrap_or(0.0);
                    unit_sources(&[(center, rx, ry, movement)])
                })
                .collect(),
        };

        info!("[LOS] mode={:?} sources={} first={:?}", ev.0, sources.len(), sources.first());

        // Despawn old source point markers.
        for e in existing_markers.iter() {
            commands.entity(e).despawn();
        }

        // Spawn new source point markers with SourceIndex for later ray attachment.
        let marker_mesh = meshes.add(Circle::new(0.08));
        let marker_mat =
            materials.add(ColorMaterial::from_color(Color::srgba(1.0, 0.9, 0.1, 0.8)));
        let init_vis = if overlay_settings.show_source_points {
            Visibility::Visible
        } else {
            Visibility::Hidden
        };
        for (i, &pt) in sources.iter().enumerate() {
            commands
                .spawn((
                    Mesh2d(marker_mesh.clone()),
                    MeshMaterial2d(marker_mat.clone()),
                    Transform::from_xyz(pt.x, pt.y, 4.5),
                    init_vis,
                    SourcePointMarker,
                    SourceIndex(i),
                    PickingBehavior::default(),
                ))
                .observe(
                    |trigger: Trigger<Pointer<Click>>,
                     mut selected: ResMut<SelectedSourceEntity>,
                     ray_verts: Query<&SourceRayVerts>| {
                        let entity = trigger.entity();
                        selected.0 = Some(entity);
                        if let Ok(rays) = ray_verts.get(entity) {
                            let verts_str: String = rays.endpoints
                                .iter()
                                .map(|v| format!("({:.3},{:.3})", v.x, v.y))
                                .collect::<Vec<_>>()
                                .join(" ");
                            info!("[LOS-RAYS] src=({:.3},{:.3}) verts={}", rays.source.x, rays.source.y, verts_str);
                        }
                    },
                );
        }

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
            analysis_complete.send(AnalysisComplete(result.0, result.1));
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
    dot_query: Query<(Entity, &SourceIndex), With<SourcePointMarker>>,
    overlay_settings: Res<OverlaySettings>,
) {
    for ev in events.read() {
        vis_state.analyzing = false;
        vis_state.danger_area_sq_in = crate::los::multi_polygon_area(&ev.0);
        vis_state.danger_region = Some(ev.0.clone());

        for e in existing.iter() {
            commands.entity(e).despawn_recursive();
        }

        let (positions, indices) = crate::los::triangulate_multi_polygon(&ev.0);
        if !positions.is_empty() {
            let mesh = build_mesh_from_triangles(positions, indices);
            let danger_color = Color::srgba(0.85, 0.1, 0.1, 0.4);
            let init_vis = if overlay_settings.show_danger_region {
                Visibility::Visible
            } else {
                Visibility::Hidden
            };

            commands.spawn((
                Mesh2d(meshes.add(mesh)),
                MeshMaterial2d(materials.add(ColorMaterial::from_color(danger_color))),
                Transform::from_xyz(0.0, 0.0, 1.5),
                init_vis,
                DangerRegionMesh,
            ));
        }

        // Attach SourceRayVerts to each source dot by matching SourceIndex.
        for (i, (src, verts)) in ev.1.iter().enumerate() {
            if let Some((entity, _)) = dot_query.iter().find(|(_, idx)| idx.0 == i) {
                commands.entity(entity).insert(SourceRayVerts {
                    source: *src,
                    endpoints: verts.clone(),
                });
            }
        }
    }
}

fn clear_analysis(
    mut events: EventReader<ClearAnalysis>,
    mut commands: Commands,
    mut vis_state: ResMut<VisibilityState>,
    mut selected: ResMut<SelectedSourceEntity>,
    danger_meshes: Query<Entity, With<DangerRegionMesh>>,
    source_dots: Query<Entity, With<SourcePointMarker>>,
) {
    for _ in events.read() {
        for e in danger_meshes.iter() {
            commands.entity(e).despawn();
        }
        for e in source_dots.iter() {
            commands.entity(e).despawn();
        }
        vis_state.danger_region = None;
        vis_state.danger_area_sq_in = 0.0;
        selected.0 = None;
    }
}

fn draw_selected_source_rays(
    selected: Res<SelectedSourceEntity>,
    query: Query<&SourceRayVerts>,
    mut gizmos: Gizmos,
) {
    let Some(entity) = selected.0 else { return };
    let Ok(rays) = query.get(entity) else { return };
    let color = Color::srgba(1.0, 1.0, 1.0, 0.35);
    for &endpoint in &rays.endpoints {
        gizmos.line_2d(rays.source, endpoint, color);
    }
}

fn sync_source_point_visibility(
    mut q: Query<&mut Visibility, With<SourcePointMarker>>,
    settings: Res<OverlaySettings>,
) {
    if !settings.is_changed() {
        return;
    }
    let v = vis(settings.show_source_points);
    for mut vis in &mut q {
        *vis = v;
    }
}

fn sync_danger_region_visibility(
    mut q: Query<&mut Visibility, With<DangerRegionMesh>>,
    settings: Res<OverlaySettings>,
) {
    if !settings.is_changed() {
        return;
    }
    let v = vis(settings.show_danger_region);
    for mut vis in &mut q {
        *vis = v;
    }
}

fn draw_collision_boxes(
    settings: Res<OverlaySettings>,
    layouts: Res<TerrainLayouts>,
    active: Res<ActiveLayout>,
    mut gizmos: Gizmos,
) {
    if !settings.show_collision_boxes {
        return;
    }
    let Some(name) = &active.0 else { return };
    let Some(layout) = layouts.0.iter().find(|l| &l.id == name) else { return };
    let empty: std::collections::HashSet<&str> = Default::default();
    let solid = extract_solid_edges(&layout.pieces, &empty);
    let one_way = extract_footprint_edges(&layout.pieces, &empty);
    for [a, b] in solid {
        gizmos.line_2d(a, b, Color::srgba(0.0, 1.0, 1.0, 0.9));
    }
    for ([a, b], _) in one_way {
        gizmos.line_2d(a, b, Color::srgba(1.0, 0.6, 0.0, 0.7));
    }
}

fn vis(b: bool) -> Visibility {
    if b { Visibility::Visible } else { Visibility::Hidden }
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
