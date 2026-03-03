use bevy::prelude::*;
use bevy::tasks::{AsyncComputeTaskPool, Task};
use futures_lite::future;

use crate::events::{AnalysisComplete, ClearAnalysis, TriggerAnalysis};
use crate::los::{
    extract_footprint_edges, extract_solid_edges, run_analysis, sample_zone_sources,
    unit_sources_with_candidates, N_PERIMETER,
};
use crate::resources::{ActiveLayout, ActivePattern, DeploymentPatterns, OverlaySettings, TerrainLayouts};
use crate::types::units::{Player, UnitBase};
use crate::types::visibility::{
    AnalysisMode, CandidateIndex, CandidatePointMarker, DangerRegionMesh, SelectedCandidate,
    SelectedSourceEntity, SelectedUnitForAnalysis, SourceIndex, SourcePointMarker, SourceRayVerts,
    UnitAnalysisStage, UnitAnalysisState, VisibilityState,
};

pub struct VisibilityPlugin;

impl Plugin for VisibilityPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<SelectedSourceEntity>()
            .init_resource::<SelectedUnitForAnalysis>()
            .init_resource::<SelectedCandidate>()
            .init_resource::<UnitAnalysisState>()
            .add_event::<ClearAnalysis>()
            .add_systems(
                Update,
                (
                    trigger_analysis,
                    poll_analysis_task,
                    on_analysis_complete,
                    clear_analysis,
                    draw_selected_source_rays,
                    on_candidate_selected,
                    sync_source_point_visibility,
                    sync_danger_region_visibility,
                    apply_unit_fade,
                    draw_movement_reach_gizmos,
                    draw_collision_boxes,
                ),
            );
    }
}

/// Component holding the in-flight analysis task.
#[derive(Component)]
struct AnalysisTask(Task<(geo::MultiPolygon<f64>, Vec<(Vec2, Vec<Vec2>)>)>);

#[allow(clippy::too_many_arguments)]
fn trigger_analysis(
    mut commands: Commands,
    mut events: EventReader<TriggerAnalysis>,
    mut vis_state: ResMut<VisibilityState>,
    mut selected: ResMut<SelectedSourceEntity>,
    mut selected_unit: ResMut<SelectedUnitForAnalysis>,
    mut selected_candidate: ResMut<SelectedCandidate>,
    mut unit_analysis_state: ResMut<UnitAnalysisState>,
    layouts: Res<TerrainLayouts>,
    active_layout: Res<ActiveLayout>,
    patterns: Res<DeploymentPatterns>,
    active_pattern: Res<ActivePattern>,
    unit_bases: Query<(&Transform, &UnitBase)>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<ColorMaterial>>,
    overlay_settings: Res<OverlaySettings>,
    old_markers: Query<Entity, Or<(With<SourcePointMarker>, With<CandidatePointMarker>)>>,
) {
    for ev in events.read() {
        if vis_state.analyzing {
            continue;
        }
        vis_state.analyzing = true;
        vis_state.mode = ev.0;
        selected.0 = None;
        selected_unit.0 = None;
        selected_candidate.0 = None;
        unit_analysis_state.stage = UnitAnalysisStage::Idle;
        unit_analysis_state.candidates.clear();

        let pieces = active_layout
            .0
            .as_ref()
            .and_then(|id| layouts.0.iter().find(|l| &l.id == id))
            .map(|l| l.pieces.clone())
            .unwrap_or_default();

        // Despawn old source point markers and candidate markers.
        for e in old_markers.iter() {
            commands.entity(e).despawn();
        }

        let sources: Vec<Vec2> = match ev.0 {
            AnalysisMode::ZoneCoverage => {
                let srcs = active_pattern
                    .0
                    .as_ref()
                    .and_then(|id| patterns.0.iter().find(|p| &p.id == id))
                    .and_then(|pat| pat.zones.iter().find(|z| z.to_player() == Player::Attacker))
                    .map(|z| sample_zone_sources(z))
                    .unwrap_or_default();

                // Spawn visible yellow source dots immediately for ZoneCoverage.
                let marker_mesh = meshes.add(Circle::new(0.08));
                let marker_mat =
                    materials.add(ColorMaterial::from_color(Color::srgba(1.0, 0.9, 0.1, 0.8)));
                let init_vis = if overlay_settings.show_source_points {
                    Visibility::Visible
                } else {
                    Visibility::Hidden
                };
                for (i, &pt) in srcs.iter().enumerate() {
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
                                    let verts_str: String = rays
                                        .endpoints
                                        .iter()
                                        .map(|v| format!("({:.3},{:.3})", v.x, v.y))
                                        .collect::<Vec<_>>()
                                        .join(" ");
                                    info!(
                                        "[LOS-RAYS] src=({:.3},{:.3}) verts={}",
                                        rays.source.x, rays.source.y, verts_str
                                    );
                                }
                            },
                        );
                }

                srcs
            }

            AnalysisMode::UnitPositions => {
                let bases: Vec<(Vec2, f32, f32, f32)> = unit_bases
                    .iter()
                    .filter(|(_, ub)| ub.player == Player::Attacker)
                    .map(|(t, ub)| {
                        let center = t.translation.truncate();
                        let rx = ub.base_shape.radius_x_inches();
                        let ry = ub.base_shape.radius_y_inches();
                        let movement = ub.movement_inches.unwrap_or(0.0);
                        (center, rx, ry, movement)
                    })
                    .collect();

                let crate::los::UnitCandidateData {
                    sources: unit_srcs,
                    candidates,
                } = unit_sources_with_candidates(&bases, &pieces);

                unit_analysis_state.candidates = candidates;
                unit_analysis_state.stage = UnitAnalysisStage::SelectCandidate;

                // Spawn green candidate dots.
                let cand_mesh = meshes.add(Circle::new(0.35));
                let cand_mat =
                    materials.add(ColorMaterial::from_color(Color::srgba(0.1, 0.9, 0.2, 0.9)));
                for (i, cand_info) in unit_analysis_state.candidates.iter().enumerate() {
                    let pt = cand_info.center;
                    commands
                        .spawn((
                            Mesh2d(cand_mesh.clone()),
                            MeshMaterial2d(cand_mat.clone()),
                            Transform::from_xyz(pt.x, pt.y, 4.6),
                            Visibility::Visible,
                            CandidatePointMarker,
                            CandidateIndex(i),
                            PickingBehavior::default(),
                        ))
                        .observe(
                            |trigger: Trigger<Pointer<Click>>,
                             mut selected_candidate: ResMut<SelectedCandidate>,
                             mut unit_analysis_state: ResMut<UnitAnalysisState>,
                             idx_q: Query<&CandidateIndex>| {
                                if let Ok(idx) = idx_q.get(trigger.entity()) {
                                    selected_candidate.0 = Some(idx.0);
                                    unit_analysis_state.stage = UnitAnalysisStage::SelectSource;
                                }
                            },
                        );
                }

                // Spawn yellow source dots — hidden until a candidate is selected.
                let marker_mesh = meshes.add(Circle::new(0.15));
                let marker_mat =
                    materials.add(ColorMaterial::from_color(Color::srgba(1.0, 0.9, 0.1, 0.8)));
                for (i, &pt) in unit_srcs.iter().enumerate() {
                    commands
                        .spawn((
                            Mesh2d(marker_mesh.clone()),
                            MeshMaterial2d(marker_mat.clone()),
                            Transform::from_xyz(pt.x, pt.y, 4.5),
                            Visibility::Hidden,
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
                                    let verts_str: String = rays
                                        .endpoints
                                        .iter()
                                        .map(|v| format!("({:.3},{:.3})", v.x, v.y))
                                        .collect::<Vec<_>>()
                                        .join(" ");
                                    info!(
                                        "[LOS-RAYS] src=({:.3},{:.3}) verts={}",
                                        rays.source.x, rays.source.y, verts_str
                                    );
                                }
                            },
                        );
                }

                unit_srcs
            }
        };

        info!("[LOS] mode={:?} sources={} first={:?}", ev.0, sources.len(), sources.first());

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
    mut selected_unit: ResMut<SelectedUnitForAnalysis>,
    mut selected_candidate: ResMut<SelectedCandidate>,
    mut unit_analysis_state: ResMut<UnitAnalysisState>,
    danger_meshes: Query<Entity, With<DangerRegionMesh>>,
    source_dots: Query<Entity, With<SourcePointMarker>>,
    candidate_dots: Query<Entity, With<CandidatePointMarker>>,
) {
    for _ in events.read() {
        for e in danger_meshes.iter() {
            commands.entity(e).despawn();
        }
        for e in source_dots.iter() {
            commands.entity(e).despawn();
        }
        for e in candidate_dots.iter() {
            commands.entity(e).despawn();
        }
        vis_state.danger_region = None;
        vis_state.danger_area_sq_in = 0.0;
        selected.0 = None;
        selected_unit.0 = None;
        selected_candidate.0 = None;
        unit_analysis_state.stage = UnitAnalysisStage::Idle;
        unit_analysis_state.candidates.clear();
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

/// Shows yellow source dots for the selected candidate; hides all others.
fn on_candidate_selected(
    selected: Res<SelectedCandidate>,
    mut source_q: Query<(&mut Visibility, &SourceIndex), With<SourcePointMarker>>,
) {
    if !selected.is_changed() {
        return;
    }
    let Some(cand_idx) = selected.0 else { return };
    for (mut vis, src_idx) in &mut source_q {
        *vis = if src_idx.0 / N_PERIMETER == cand_idx {
            Visibility::Visible
        } else {
            Visibility::Hidden
        };
    }
}

fn sync_source_point_visibility(
    mut q: Query<&mut Visibility, With<SourcePointMarker>>,
    settings: Res<OverlaySettings>,
    vis_state: Res<VisibilityState>,
    unit_analysis_state: Res<UnitAnalysisState>,
) {
    if !settings.is_changed() {
        return;
    }
    // Don't override staged visibility in UnitPositions flow.
    if vis_state.mode == AnalysisMode::UnitPositions
        && unit_analysis_state.stage != UnitAnalysisStage::Idle
    {
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

fn apply_unit_fade(
    selected: Res<SelectedUnitForAnalysis>,
    vis_state: Res<VisibilityState>,
    unit_analysis_state: Res<UnitAnalysisState>,
    unit_q: Query<(&Transform, &UnitBase)>,
    danger_q: Query<&MeshMaterial2d<ColorMaterial>, With<DangerRegionMesh>>,
    mut source_q: Query<(&mut Visibility, Option<&SourceRayVerts>), With<SourcePointMarker>>,
    mut materials: ResMut<Assets<ColorMaterial>>,
    overlay: Res<OverlaySettings>,
) {
    if !selected.is_changed() {
        return;
    }
    // Don't override staged visibility in UnitPositions flow.
    if vis_state.mode == AnalysisMode::UnitPositions
        && unit_analysis_state.stage != UnitAnalysisStage::Idle
    {
        return;
    }

    match selected.0 {
        None => {
            let v = vis(overlay.show_source_points);
            for (mut vis, _) in &mut source_q {
                *vis = v;
            }
            for handle in &danger_q {
                if let Some(mat) = materials.get_mut(handle.id()) {
                    mat.color = mat.color.with_alpha(0.4);
                }
            }
        }
        Some(unit_entity) => {
            let Ok((transform, unit_base)) = unit_q.get(unit_entity) else {
                return;
            };
            let unit_pos = transform.translation.truncate();
            let threshold = unit_base.movement_inches.unwrap_or(0.0)
                + 2.0 * unit_base
                    .base_shape
                    .radius_x_inches()
                    .max(unit_base.base_shape.radius_y_inches())
                + 0.5;

            for (mut vis, ray_verts) in &mut source_q {
                let matches = ray_verts
                    .map(|rv| rv.source.distance(unit_pos) <= threshold)
                    .unwrap_or(false);
                *vis = if matches { Visibility::Visible } else { Visibility::Hidden };
            }
            for handle in &danger_q {
                if let Some(mat) = materials.get_mut(handle.id()) {
                    mat.color = mat.color.with_alpha(0.08);
                }
            }
        }
    }
}

/// Draws dashed movement-reach ellipses and a footprint ellipse at the selected candidate.
fn draw_movement_reach_gizmos(
    state: Res<UnitAnalysisState>,
    selected: Res<SelectedCandidate>,
    mut gizmos: Gizmos,
) {
    if state.stage == UnitAnalysisStage::Idle {
        return;
    }
    let green = Color::srgba(0.1, 0.9, 0.2, 0.6);

    // Draw base footprint ellipse at each candidate destination.
    for cand in &state.candidates {
        draw_dashed_ellipse(&mut gizmos, cand.center, cand.rx, cand.ry, green);
    }

    // Highlight the selected candidate in yellow.
    if let Some(idx) = selected.0 {
        if let Some(cand) = state.candidates.get(idx) {
            let yellow = Color::srgba(1.0, 0.9, 0.1, 0.8);
            draw_dashed_ellipse(&mut gizmos, cand.center, cand.rx, cand.ry, yellow);
        }
    }
}

fn draw_dashed_ellipse(gizmos: &mut Gizmos, center: Vec2, rx: f32, ry: f32, color: Color) {
    const N: usize = 48;
    for i in (0..N).step_by(2) {
        let a0 = i as f32 * std::f32::consts::TAU / N as f32;
        let a1 = (i + 1) as f32 * std::f32::consts::TAU / N as f32;
        let p0 = center + Vec2::new(a0.cos() * rx, a0.sin() * ry);
        let p1 = center + Vec2::new(a1.cos() * rx, a1.sin() * ry);
        gizmos.line_2d(p0, p1, color);
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
