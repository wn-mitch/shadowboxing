use bevy::prelude::*;

use crate::events::{ClearPlayerUnits, RemoveModelUnits, SpawnUnit, UnitMoved};
use crate::resources::{ActiveLayout, ActivePattern, BoardConfig, DeploymentPatterns, OverlaySettings, TerrainLayouts};
use crate::types::terrain::TerrainPiece;
use crate::types::timeline::{AdvanceIndicator, GameTimeline, MovementRangeRing};
use crate::types::units::{BaseShape, Player, UnitBase};
use crate::types::visibility::{AnalysisMode, SelectedUnitForAnalysis, VisibilityState};
use crate::los::shapes::point_in_shape;

#[derive(Component)]
pub struct ZoneRingMarker;

pub struct UnitsPlugin;

impl Plugin for UnitsPlugin {
    fn build(&self, app: &mut App) {
        app.add_event::<ClearPlayerUnits>()
            .add_event::<RemoveModelUnits>()
            .add_systems(
                Update,
                (
                    on_spawn_unit,
                    handle_drag,
                    update_validity_indicators,
                    sync_validity_rings,
                    on_clear_player_units,
                    on_remove_model_units,
                    handle_unit_click_for_fade,
                    sync_unit_tint,
                ),
            );
    }
}

fn sync_validity_rings(
    mut q: Query<&mut Visibility, With<ZoneRingMarker>>,
    settings: Res<OverlaySettings>,
) {
    if !settings.is_changed() {
        return;
    }
    let v = vis(settings.show_validity_rings);
    for mut vis in &mut q {
        *vis = v;
    }
}

fn vis(b: bool) -> Visibility {
    if b { Visibility::Visible } else { Visibility::Hidden }
}

fn on_spawn_unit(
    mut commands: Commands,
    mut events: EventReader<SpawnUnit>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<ColorMaterial>>,
    board: Res<BoardConfig>,
    patterns: Res<DeploymentPatterns>,
    active_pattern: Res<ActivePattern>,
    layouts: Res<TerrainLayouts>,
    active_layout: Res<ActiveLayout>,
) {
    for ev in events.read() {
        // Find the deployment zone for this player.
        let zone_verts = active_pattern
            .0
            .as_ref()
            .and_then(|id| patterns.0.iter().find(|p| &p.id == id))
            .and_then(|pat| pat.zones.iter().find(|z| z.to_player() == ev.player))
            .map(|z| z.world_vertices());

        let terrain_pieces: Vec<TerrainPiece> = active_layout
            .0
            .as_ref()
            .and_then(|id| layouts.0.iter().find(|l| &l.id == id))
            .map(|l| l.pieces.clone())
            .unwrap_or_default();

        for i in 0..ev.count {
            let start_pos = find_valid_spawn_pos(
                &ev.base_shape,
                zone_verts.as_deref(),
                &terrain_pieces,
                &board,
                i,
            );

            spawn_base(
                &mut commands,
                &mut meshes,
                &mut materials,
                &ev.unit_name,
                &ev.model_name,
                &ev.base_shape,
                ev.player,
                ev.color,
                ev.movement_inches,
                start_pos,
            );
        }
    }
}

fn find_valid_spawn_pos(
    base: &BaseShape,
    zone_verts: Option<&[Vec2]>,
    pieces: &[TerrainPiece],
    board: &BoardConfig,
    index: u32,
) -> Vec2 {
    let rx = base.radius_x_inches();
    let ry = base.radius_y_inches();

    // Search in deployment zone at 1" spacing, or fall back to board center area.
    let search_verts: Vec<Vec2>;
    let use_verts: &[Vec2] = if let Some(z) = zone_verts {
        z
    } else {
        search_verts = vec![
            Vec2::new(0.0, 0.0),
            Vec2::new(board.width, 0.0),
            Vec2::new(board.width, board.height),
            Vec2::new(0.0, board.height),
        ];
        &search_verts
    };

    let (min_x, min_y, max_x, max_y) = bounding_box(use_verts);

    // Scan left-to-right, top-to-bottom at 1" spacing.
    let mut y = min_y + ry;
    let mut candidate_idx: u32 = 0;
    while y <= max_y - ry {
        let mut x = min_x + rx;
        while x <= max_x - rx {
            let pos = Vec2::new(x, y);
            if base_in_zone_optional(pos, base, zone_verts)
                && !overlaps_any_terrain(pos, base, pieces)
                && pos.x >= rx
                && pos.x <= board.width - rx
                && pos.y >= ry
                && pos.y <= board.height - ry
            {
                if candidate_idx == index {
                    return pos;
                }
                candidate_idx += 1;
            }
            x += 1.0;
        }
        y += 1.0;
    }

    // Fallback: board center.
    Vec2::new(board.width / 2.0, board.height / 2.0)
}

fn base_fully_in_zone(pos: Vec2, base: &BaseShape, verts: &[Vec2]) -> bool {
    let rx = base.radius_x_inches();
    let ry = base.radius_y_inches();
    let d = 0.707_f32;
    let check_pts = [
        pos,
        pos + Vec2::new(rx, 0.0),
        pos - Vec2::new(rx, 0.0),
        pos + Vec2::new(0.0, ry),
        pos - Vec2::new(0.0, ry),
        pos + Vec2::new(rx * d, ry * d),
        pos + Vec2::new(-rx * d, ry * d),
        pos + Vec2::new(rx * d, -ry * d),
        pos + Vec2::new(-rx * d, -ry * d),
    ];
    check_pts.iter().all(|&p| crate::types::deployment::point_in_polygon_pub(p, verts))
}

fn base_in_zone_optional(pos: Vec2, base: &BaseShape, verts: Option<&[Vec2]>) -> bool {
    match verts {
        Some(v) => base_fully_in_zone(pos, base, v),
        None => true,
    }
}

fn overlaps_any_terrain(pos: Vec2, base: &BaseShape, pieces: &[TerrainPiece]) -> bool {
    use crate::types::terrain::TerrainShape;
    let rx = base.radius_x_inches();
    let ry = base.radius_y_inches();
    let d = 0.707_f32;
    let check_pts = [
        pos,
        pos + Vec2::new(rx, 0.0),
        pos - Vec2::new(rx, 0.0),
        pos + Vec2::new(0.0, ry),
        pos - Vec2::new(0.0, ry),
        pos + Vec2::new(rx * d, ry * d),
        pos + Vec2::new(-rx * d, ry * d),
        pos + Vec2::new(rx * d, -ry * d),
        pos + Vec2::new(-rx * d, -ry * d),
    ];

    for piece in pieces {
        if !piece.blocking {
            continue;
        }
        for shape in &piece.shapes {
            if !matches!(shape, TerrainShape::Line { .. }) {
                continue; // footprints are passable; only walls block placement
            }
            for &pt in &check_pts {
                if point_in_shape(pt, shape, piece) {
                    return true;
                }
            }
        }
    }
    false
}

fn bases_overlap(pos_a: Vec2, base_a: &BaseShape, pos_b: Vec2, base_b: &BaseShape) -> bool {
    let ra = base_a.radius_x_inches().max(base_a.radius_y_inches());
    let rb = base_b.radius_x_inches().max(base_b.radius_y_inches());
    pos_a.distance(pos_b) < ra + rb
}

fn grey_tint(color: Color) -> Color {
    let s = color.to_srgba();
    let t = 0.5_f32;
    Color::srgb(
        s.red * (1.0 - t) + 0.5 * t,
        s.green * (1.0 - t) + 0.5 * t,
        s.blue * (1.0 - t) + 0.5 * t,
    )
}

fn sync_unit_tint(
    timeline: Res<GameTimeline>,
    units: Query<(&UnitBase, &MeshMaterial2d<ColorMaterial>)>,
    mut materials: ResMut<Assets<ColorMaterial>>,
) {
    let active = timeline.active_player_in_live_view();

    for (unit_base, mat_handle) in &units {
        let target = if timeline.locked {
            match active {
                Some(p) if p == unit_base.player => unit_base.color,
                _ => grey_tint(unit_base.color),
            }
        } else {
            unit_base.color
        };

        // Only write when the color actually needs to change.
        let needs_update = materials.get(mat_handle.id()).map(|m| m.color != target).unwrap_or(false);
        if needs_update {
            if let Some(mat) = materials.get_mut(mat_handle.id()) {
                mat.color = target;
            }
        }
    }
}

fn bounding_box(verts: &[Vec2]) -> (f32, f32, f32, f32) {
    let mut min_x = f32::MAX;
    let mut min_y = f32::MAX;
    let mut max_x = f32::MIN;
    let mut max_y = f32::MIN;
    for v in verts {
        min_x = min_x.min(v.x);
        min_y = min_y.min(v.y);
        max_x = max_x.max(v.x);
        max_y = max_y.max(v.y);
    }
    (min_x, min_y, max_x, max_y)
}

fn spawn_base(
    commands: &mut Commands,
    meshes: &mut ResMut<Assets<Mesh>>,
    materials: &mut ResMut<Assets<ColorMaterial>>,
    unit_name: &str,
    model_name: &str,
    base_shape: &BaseShape,
    player: Player,
    color: Color,
    movement_inches: Option<f32>,
    pos: Vec2,
) {
    let rx = base_shape.radius_x_inches();
    let ry = base_shape.radius_y_inches();

    let mesh: Mesh = if base_shape.is_circular() {
        Circle::new(rx).into()
    } else {
        Ellipse::new(rx, ry).into()
    };

    let ring_inner = rx.max(ry);
    let ring = Annulus::new(ring_inner, ring_inner + 0.18);

    commands
        .spawn((
            Mesh2d(meshes.add(mesh)),
            MeshMaterial2d(materials.add(ColorMaterial::from_color(color))),
            Transform::from_xyz(pos.x, pos.y, 4.0),
            UnitBase {
                unit_name: unit_name.to_string(),
                model_name: model_name.to_string(),
                base_shape: base_shape.clone(),
                locked: false,
                movement_inches,
                player,
                color,
                last_valid_pos: pos,
            },
            PickingBehavior::default(),
        ))
        .with_children(|parent| {
            // White outline ring — always visible just outside the model edge.
            parent.spawn((
                Mesh2d(meshes.add(Annulus::new(ring_inner, ring_inner + 0.12))),
                MeshMaterial2d(materials.add(ColorMaterial::from_color(Color::WHITE))),
                Transform::from_xyz(0.0, 0.0, 0.05),
            ));

            // Zone violation ring (hidden by default; z=0.15 covers the white ring when shown).
            parent.spawn((
                Mesh2d(meshes.add(ring)),
                MeshMaterial2d(materials.add(ColorMaterial::from_color(
                    Color::srgba(1.0, 0.15, 0.15, 0.9),
                ))),
                Transform::from_xyz(0.0, 0.0, 0.15),
                Visibility::Hidden,
                ZoneRingMarker,
            ));

            // Name label.
            parent.spawn((
                Text2d::new(model_name.to_string()),
                TextFont { font_size: 10.0, ..default() },
                TextColor(Color::WHITE),
                Transform::from_xyz(0.0, 0.0, 0.2).with_scale(Vec3::splat(0.08)),
            ));

            // "ADV" badge — appears when the unit has advanced.
            // Range rings are spawned as standalone entities by TimelinePlugin on lock.
            if movement_inches.is_some() {
                parent.spawn((
                    Text2d::new("ADV"),
                    TextFont { font_size: 10.0, ..default() },
                    TextColor(Color::srgb(1.0, 1.0, 0.0)),
                    Transform::from_xyz(0.0, -0.35, 0.25).with_scale(Vec3::splat(0.08)),
                    Visibility::Hidden,
                    AdvanceIndicator,
                    PickingBehavior::IGNORE,
                ));
            }
        });
}

fn update_validity_indicators(
    units: Query<(&UnitBase, &Transform, &Children)>,
    mut rings: Query<&mut Visibility, With<ZoneRingMarker>>,
    patterns: Res<DeploymentPatterns>,
    active_pattern: Res<ActivePattern>,
    overlay_settings: Res<OverlaySettings>,
) {
    let zones = active_pattern
        .0
        .as_ref()
        .and_then(|id| patterns.0.iter().find(|p| &p.id == id))
        .map(|p| p.zones.as_slice())
        .unwrap_or(&[]);

    for (unit_base, transform, children) in &units {
        let pos = transform.translation.truncate();
        let zone_verts = zones
            .iter()
            .find(|z| z.to_player() == unit_base.player)
            .map(|z| z.world_vertices());

        let in_zone = match zone_verts.as_deref() {
            Some(verts) => base_fully_in_zone(pos, &unit_base.base_shape, verts),
            None => true,
        };

        for &child in children.iter() {
            if let Ok(mut vis) = rings.get_mut(child) {
                *vis = if !overlay_settings.show_validity_rings || in_zone {
                    Visibility::Hidden
                } else {
                    Visibility::Visible
                };
            }
        }
    }
}

fn on_clear_player_units(
    mut commands: Commands,
    mut ev_clear: EventReader<ClearPlayerUnits>,
    units: Query<(Entity, &UnitBase)>,
) {
    for ev in ev_clear.read() {
        for (entity, base) in &units {
            if base.player == ev.player {
                commands.entity(entity).despawn_recursive();
            }
        }
    }
}

fn on_remove_model_units(
    mut commands: Commands,
    mut ev: EventReader<RemoveModelUnits>,
    units: Query<(Entity, &UnitBase)>,
) {
    for ev in ev.read() {
        for (entity, base) in &units {
            if base.player == ev.player
                && base.unit_name == ev.unit_name
                && base.model_name == ev.model_name
            {
                commands.entity(entity).despawn_recursive();
            }
        }
    }
}

fn handle_unit_click_for_fade(
    mut click_events: EventReader<Pointer<Click>>,
    bases: Query<Entity, With<UnitBase>>,
    vis_state: Res<VisibilityState>,
    mut selected_unit: ResMut<SelectedUnitForAnalysis>,
    timeline: Res<GameTimeline>,
    mut ring_query: Query<&mut Visibility, With<MovementRangeRing>>,
) {
    for ev in click_events.read() {
        if bases.get(ev.target).is_err() {
            continue;
        }

        // When deployment is locked, show the clicked unit's standalone range rings.
        if timeline.locked {
            let to_show = timeline.ring_entities.get(&ev.target).copied();
            // Hide all rings first.
            for mut vis in &mut ring_query {
                *vis = Visibility::Hidden;
            }
            // Show the pair for the clicked unit.
            if let Some([nr, ar]) = to_show {
                if let Ok(mut v) = ring_query.get_mut(nr) {
                    *v = Visibility::Visible;
                }
                if let Ok(mut v) = ring_query.get_mut(ar) {
                    *v = Visibility::Visible;
                }
            }
        }

        if vis_state.mode != AnalysisMode::UnitPositions {
            continue;
        }
        // Toggle: clicking the same unit again deselects it.
        selected_unit.0 = match selected_unit.0 {
            Some(e) if e == ev.target => None,
            _ => Some(ev.target),
        };
    }
}

/// Drag handling via Bevy Picking pointer events.
fn handle_drag(
    mut bases: Query<(Entity, &mut Transform, &mut UnitBase)>,
    mut drag_events: EventReader<Pointer<Drag>>,
    mut drag_end_events: EventReader<Pointer<DragEnd>>,
    board: Res<BoardConfig>,
    layouts: Res<TerrainLayouts>,
    active_layout: Res<ActiveLayout>,
    camera_q: Query<(&Camera, &GlobalTransform)>,
    timeline: Res<GameTimeline>,
    mut ev_unit_moved: EventWriter<UnitMoved>,
) {
    let terrain_pieces: Vec<TerrainPiece> = active_layout
        .0
        .as_ref()
        .and_then(|id| layouts.0.iter().find(|l| &l.id == id))
        .map(|l| l.pieces.clone())
        .unwrap_or_default();

    // Snapshot all unit positions before any mutations for overlap checking on DragEnd.
    let unit_snapshot: Vec<(Entity, Vec2, BaseShape)> = bases
        .iter()
        .map(|(e, t, ub)| (e, t.translation.truncate(), ub.base_shape.clone()))
        .collect();

    for ev in drag_events.read() {
        let Ok((_, mut transform, mut unit_base)) = bases.get_mut(ev.target) else {
            continue;
        };
        if unit_base.locked {
            continue;
        }
        // Skip units that don't belong to the active player's turn.
        if timeline.locked {
            if let Some(active) = timeline.active_player_in_live_view() {
                if unit_base.player != active {
                    continue;
                }
            }
        }

        // Bevy Picking Drag delta is in logical pixels.
        // Convert to world units: we derive scale from the camera's NDC viewport.
        let delta_world = if let Ok((cam, cam_gt)) = camera_q.get_single() {
            // Map two screen points through the camera to get world scale.
            let origin_ndc = Vec2::ZERO;
            let offset_ndc = Vec2::new(1.0, 0.0);
            let world_origin = cam
                .ndc_to_world(cam_gt, origin_ndc.extend(0.0))
                .map(|p| p.truncate());
            let world_offset = cam
                .ndc_to_world(cam_gt, offset_ndc.extend(0.0))
                .map(|p| p.truncate());

            if let (Some(wo), Some(woff)) = (world_origin, world_offset) {
                let vp_size = cam.logical_viewport_size().unwrap_or(Vec2::new(1.0, 1.0));
                let world_per_px = (woff - wo).length() / (vp_size.x / 2.0);
                Vec2::new(ev.delta.x * world_per_px, -ev.delta.y * world_per_px)
            } else {
                Vec2::ZERO
            }
        } else {
            Vec2::ZERO
        };

        transform.translation.x += delta_world.x;
        transform.translation.y += delta_world.y;
    }

    for ev in drag_end_events.read() {
        let entity = ev.target;
        let Ok((_, mut transform, mut unit_base)) = bases.get_mut(entity) else {
            continue;
        };
        if unit_base.locked {
            continue;
        }
        // Snap back if this unit doesn't belong to the active player's turn.
        if timeline.locked {
            if let Some(active) = timeline.active_player_in_live_view() {
                if unit_base.player != active {
                    transform.translation.x = unit_base.last_valid_pos.x;
                    transform.translation.y = unit_base.last_valid_pos.y;
                    continue;
                }
            }
        }

        // In a historical view, discard the drag and snap back.
        if timeline.locked && timeline.current_index < timeline.snapshots.len() {
            transform.translation.x = unit_base.last_valid_pos.x;
            transform.translation.y = unit_base.last_valid_pos.y;
            continue;
        }

        let pos = transform.translation.truncate();
        let rx = unit_base.base_shape.radius_x_inches();
        let ry = unit_base.base_shape.radius_y_inches();

        // Clamp to board bounds.
        let clamped = Vec2::new(
            pos.x.clamp(rx, board.width - rx),
            pos.y.clamp(ry, board.height - ry),
        );

        // Check terrain and unit-unit overlap.
        let blocked = overlaps_any_terrain(clamped, &unit_base.base_shape, &terrain_pieces)
            || unit_snapshot.iter().any(|(other, other_pos, other_shape)| {
                *other != entity && bases_overlap(clamped, &unit_base.base_shape, *other_pos, other_shape)
            });
        if blocked {
            // Snap back to last valid position.
            transform.translation.x = unit_base.last_valid_pos.x;
            transform.translation.y = unit_base.last_valid_pos.y;
        } else {
            let from = timeline
                .phase_start_positions
                .get(&entity)
                .copied()
                .unwrap_or(clamped);

            transform.translation.x = clamped.x;
            transform.translation.y = clamped.y;
            unit_base.last_valid_pos = clamped;

            if timeline.locked {
                ev_unit_moved.send(UnitMoved { entity, from, to: clamped });
            }
        }
    }
}
