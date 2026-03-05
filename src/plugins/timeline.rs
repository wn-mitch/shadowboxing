use std::collections::{HashMap, HashSet};

use bevy::prelude::*;

use crate::events::{AdvancePhase, LockDeployment, RecordSnapshot, RewindToSnapshot, UnitMoved};
use crate::resources::PhaseState;
use crate::types::phase::{ActiveTool, GamePhase};
use crate::types::timeline::{
    AdvanceIndicator, FirstPlayer, GameTimeline, GhostUnit, MoveType, MovementArrow,
    MovementRangeRing, PersistentRangeRing, TimelineSnapshot,
};
use crate::types::units::{Player, UnitBase};
use crate::types::visibility::VisibilityState;

pub struct TimelinePlugin;

impl Plugin for TimelinePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<GameTimeline>()
            .add_event::<LockDeployment>()
            .add_event::<RecordSnapshot>()
            .add_event::<RewindToSnapshot>()
            .add_event::<UnitMoved>()
            .add_event::<AdvancePhase>()
            .add_systems(
                Update,
                (
                    on_lock_deployment,
                    on_unit_moved,
                    on_record_snapshot,
                    on_rewind_to_snapshot,
                    on_advance_phase,
                    update_advance_indicators,
                    cleanup_despawned_unit_arrows,
                    sync_ghost_positions,
                    sync_ring_positions,
                    sync_active_analysis_player,
                    sync_persistent_rings,
                )
                    .chain(),
            );
    }
}

// ── Systems ──────────────────────────────────────────────────────────────────

fn on_lock_deployment(
    mut events: EventReader<LockDeployment>,
    mut timeline: ResMut<GameTimeline>,
    mut phase_state: ResMut<PhaseState>,
    units: Query<(Entity, &Transform, &UnitBase)>,
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<ColorMaterial>>,
    mut next_tool: ResMut<NextState<ActiveTool>>,
) {
    for _ in events.read() {
        if timeline.locked {
            continue;
        }
        timeline.locked = true;

        phase_state.active_player = match timeline.first_player {
            FirstPlayer::Attacker => Player::Attacker,
            FirstPlayer::Defender => Player::Defender,
        };

        let positions: HashMap<Entity, (Vec2, bool)> = units
            .iter()
            .map(|(e, t, _)| (e, (t.translation.truncate(), false)))
            .collect();

        timeline.phase_start_positions = positions
            .iter()
            .map(|(&e, &(p, _))| (e, p))
            .collect();
        timeline.live_unit_positions = timeline.phase_start_positions.clone();

        timeline.snapshots.push(TimelineSnapshot {
            label: "Deployment".to_string(),
            player: None,
            positions,
            arrow_entities: vec![],
        });

        for (entity, transform, unit_base) in units.iter() {
            let pos = transform.translation.truncate();
            let rx = unit_base.base_shape.radius_x_inches();
            let ry = unit_base.base_shape.radius_y_inches();

            let ghost_mesh: Mesh = if unit_base.base_shape.is_circular() {
                Circle::new(rx).into()
            } else {
                Ellipse::new(rx, ry).into()
            };
            let srgba = unit_base.color.to_srgba();
            let ghost_color = Color::srgba(srgba.red, srgba.green, srgba.blue, 0.25);
            let ghost = commands
                .spawn((
                    Mesh2d(meshes.add(ghost_mesh)),
                    MeshMaterial2d(materials.add(ColorMaterial::from_color(ghost_color))),
                    Transform::from_xyz(pos.x, pos.y, 3.8),
                    Visibility::Hidden,
                    GhostUnit { unit: entity },
                    PickingBehavior::IGNORE,
                ))
                .id();
            timeline.ghost_entities.insert(entity, ghost);

            if let Some(m) = unit_base.movement_inches {
                let adv_max = m + 6.0;
                let normal = commands
                    .spawn((
                        Mesh2d(meshes.add(Annulus::new(m, m + 0.12))),
                        MeshMaterial2d(materials.add(ColorMaterial::from_color(Color::srgba(
                            0.2, 0.9, 0.2, 0.85,
                        )))),
                        Transform::from_xyz(pos.x, pos.y, 0.5),
                        Visibility::Hidden,
                        MovementRangeRing,
                        PickingBehavior::IGNORE,
                    ))
                    .id();
                let advance = commands
                    .spawn((
                        Mesh2d(meshes.add(Annulus::new(adv_max, adv_max + 0.12))),
                        MeshMaterial2d(materials.add(ColorMaterial::from_color(Color::srgba(
                            1.0, 0.6, 0.0, 0.85,
                        )))),
                        Transform::from_xyz(pos.x, pos.y, 0.5),
                        Visibility::Hidden,
                        MovementRangeRing,
                        PickingBehavior::IGNORE,
                    ))
                    .id();
                timeline.ring_entities.insert(entity, [normal, advance]);
            }
        }

        timeline.current_index = timeline.snapshots.len();

        // Set the default tool for the first phase (Command).
        next_tool.set(phase_state.phase.default_tool());
    }
}

fn on_unit_moved(
    mut events: EventReader<UnitMoved>,
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<ColorMaterial>>,
    mut timeline: ResMut<GameTimeline>,
) {
    for ev in events.read() {
        if !timeline.locked || timeline.current_index < timeline.snapshots.len() {
            continue;
        }

        let arrow_entity = spawn_movement_arrow(
            &mut commands,
            &mut *meshes,
            &mut *materials,
            ev.entity,
            ev.from,
            ev.to,
            ev.move_type,
        );

        timeline.live_arrows.entry(ev.entity).or_default().push(arrow_entity);

        let segment_dist = ev.from.distance(ev.to);
        *timeline.live_cumulative_distance.entry(ev.entity).or_insert(0.0) += segment_dist;

        timeline.live_unit_positions.insert(ev.entity, ev.to);

        if let Some(&ghost) = timeline.ghost_entities.get(&ev.entity) {
            commands.entity(ghost).insert(Visibility::Visible);
        }
    }
}

fn on_record_snapshot(
    mut events: EventReader<RecordSnapshot>,
    mut timeline: ResMut<GameTimeline>,
    units: Query<(Entity, &Transform, &UnitBase)>,
    mut all_vis: Query<
        &mut Visibility,
        Or<(With<MovementArrow>, With<GhostUnit>, With<MovementRangeRing>)>,
    >,
) {
    for ev in events.read() {
        if !timeline.locked {
            continue;
        }

        let unit_states: Vec<(Entity, Vec2, bool)> = units
            .iter()
            .map(|(entity, transform, unit_base)| {
                let current_pos = transform.translation.truncate();
                let advanced = if let Some(m) = unit_base.movement_inches {
                    let cumulative = timeline
                        .live_cumulative_distance
                        .get(&entity)
                        .copied()
                        .unwrap_or(0.0);
                    cumulative > m + 0.01
                } else {
                    false
                };
                (entity, current_pos, advanced)
            })
            .collect();

        let positions: HashMap<Entity, (Vec2, bool)> = unit_states
            .iter()
            .map(|&(e, p, adv)| (e, (p, adv)))
            .collect();

        let arrow_entities: Vec<Entity> = timeline.live_arrows.values().flat_map(|v| v.iter().copied()).collect();
        let player = timeline.active_player_in_live_view();

        for mut vis in &mut all_vis {
            *vis = Visibility::Hidden;
        }

        timeline.snapshots.push(TimelineSnapshot {
            label: ev.label.clone(),
            player,
            positions,
            arrow_entities,
        });

        for &(entity, current_pos, _) in &unit_states {
            timeline.phase_start_positions.insert(entity, current_pos);
            timeline.live_unit_positions.insert(entity, current_pos);
        }

        timeline.live_arrows.clear();
        timeline.live_cumulative_distance.clear();
        timeline.current_index = timeline.snapshots.len();
    }
}

fn on_rewind_to_snapshot(
    mut events: EventReader<RewindToSnapshot>,
    mut timeline: ResMut<GameTimeline>,
    mut units: Query<(Entity, &mut Transform, &mut UnitBase)>,
    mut vis_q: Query<
        (Entity, &mut Visibility),
        Or<(With<MovementArrow>, With<GhostUnit>, With<MovementRangeRing>)>,
    >,
) {
    for ev in events.read() {
        let idx = ev.0;
        let in_live = idx >= timeline.snapshots.len();

        let arrows_to_show: HashSet<Entity> = if in_live {
            timeline.live_arrows.values().flat_map(|v| v.iter().copied()).collect()
        } else {
            timeline.snapshots[idx]
                .arrow_entities
                .iter()
                .copied()
                .collect()
        };

        let positions_to_restore: Vec<(Entity, Vec2)> = if in_live {
            timeline
                .live_unit_positions
                .iter()
                .map(|(&e, &p)| (e, p))
                .collect()
        } else {
            timeline.snapshots[idx]
                .positions
                .iter()
                .map(|(&e, &(p, _))| (e, p))
                .collect()
        };

        let ghost_unit_map: HashMap<Entity, Entity> = timeline
            .ghost_entities
            .iter()
            .map(|(&unit, &ghost)| (ghost, unit))
            .collect();
        let ring_set: HashSet<Entity> = timeline
            .ring_entities
            .values()
            .flat_map(|arr| arr.iter().copied())
            .collect();

        timeline.current_index = idx;

        for (entity, mut vis) in &mut vis_q {
            if let Some(&unit) = ghost_unit_map.get(&entity) {
                *vis = if in_live && timeline.live_arrows.contains_key(&unit) {
                    Visibility::Visible
                } else {
                    Visibility::Hidden
                };
            } else if ring_set.contains(&entity) {
                *vis = Visibility::Hidden;
            } else {
                *vis = if arrows_to_show.contains(&entity) {
                    Visibility::Visible
                } else {
                    Visibility::Hidden
                };
            }
        }

        for (e, pos) in &positions_to_restore {
            if let Ok((_, mut transform, mut unit_base)) = units.get_mut(*e) {
                transform.translation.x = pos.x;
                transform.translation.y = pos.y;
                unit_base.last_valid_pos = *pos;
            }
        }
    }
}

fn update_advance_indicators(
    timeline: Res<GameTimeline>,
    units: Query<(Entity, &Transform, &UnitBase, &Children)>,
    mut indicators: Query<&mut Visibility, With<AdvanceIndicator>>,
) {
    if !timeline.locked {
        for mut vis in &mut indicators {
            *vis = Visibility::Hidden;
        }
        return;
    }

    let in_live = timeline.current_index >= timeline.snapshots.len();

    for (entity, transform, unit_base, children) in &units {
        let advanced = if in_live {
            if let Some(m) = unit_base.movement_inches {
                let cumulative = timeline
                    .live_cumulative_distance
                    .get(&entity)
                    .copied()
                    .unwrap_or(0.0);
                cumulative > m + 0.01
            } else {
                false
            }
        } else {
            timeline
                .snapshots
                .get(timeline.current_index)
                .and_then(|s| s.positions.get(&entity))
                .map(|&(_, adv)| adv)
                .unwrap_or(false)
        };

        for &child in children.iter() {
            if let Ok(mut vis) = indicators.get_mut(child) {
                *vis = if advanced {
                    Visibility::Visible
                } else {
                    Visibility::Hidden
                };
            }
        }
    }
}

fn cleanup_despawned_unit_arrows(
    mut removed: RemovedComponents<UnitBase>,
    mut timeline: ResMut<GameTimeline>,
    mut commands: Commands,
) {
    for entity in removed.read() {
        if let Some(arrows) = timeline.live_arrows.remove(&entity) {
            for arrow in arrows {
                commands.entity(arrow).despawn_recursive();
            }
        }
        timeline.live_cumulative_distance.remove(&entity);
        if let Some(ghost) = timeline.ghost_entities.remove(&entity) {
            commands.entity(ghost).despawn_recursive();
        }
        if let Some(rings) = timeline.ring_entities.remove(&entity) {
            for ring in rings {
                commands.entity(ring).despawn_recursive();
            }
        }
        timeline.phase_start_positions.remove(&entity);
        timeline.live_unit_positions.remove(&entity);
    }
}

fn sync_ghost_positions(
    timeline: Res<GameTimeline>,
    mut ghost_q: Query<(&mut Transform, &GhostUnit)>,
) {
    if !timeline.locked || timeline.current_index < timeline.snapshots.len() {
        return;
    }
    for (mut transform, ghost) in &mut ghost_q {
        if let Some(&start) = timeline.phase_start_positions.get(&ghost.unit) {
            transform.translation.x = start.x;
            transform.translation.y = start.y;
        }
    }
}

fn sync_ring_positions(
    timeline: Res<GameTimeline>,
    mut transforms: Query<&mut Transform, With<MovementRangeRing>>,
) {
    if !timeline.locked || timeline.current_index < timeline.snapshots.len() {
        return;
    }
    for (&unit, &[nr, ar]) in &timeline.ring_entities {
        if let Some(&start) = timeline.phase_start_positions.get(&unit) {
            if let Ok(mut t) = transforms.get_mut(nr) {
                t.translation.x = start.x;
                t.translation.y = start.y;
            }
            if let Ok(mut t) = transforms.get_mut(ar) {
                t.translation.x = start.x;
                t.translation.y = start.y;
            }
        }
    }
}

fn sync_active_analysis_player(
    timeline: Res<GameTimeline>,
    mut vis_state: ResMut<VisibilityState>,
) {
    let player = if timeline.current_index < timeline.snapshots.len() {
        timeline
            .snapshots
            .get(timeline.current_index)
            .and_then(|s| s.player)
    } else {
        timeline.active_player_in_live_view()
    };
    if vis_state.active_analysis_player != player {
        vis_state.active_analysis_player = player;
    }
}

/// Keeps persistent range rings centered on their associated unit.
fn sync_persistent_rings(
    units: Query<&Transform, With<UnitBase>>,
    mut rings: Query<(&mut Transform, &PersistentRangeRing), Without<UnitBase>>,
) {
    for (mut ring_t, pr) in &mut rings {
        if let Ok(unit_t) = units.get(pr.unit) {
            ring_t.translation.x = unit_t.translation.x;
            ring_t.translation.y = unit_t.translation.y;
        }
    }
}

// ── Phase advancement ─────────────────────────────────────────────────────────

/// Generalized cleanup of killed units and their timeline data.
fn despawn_killed_units(
    timeline: &mut GameTimeline,
    commands: &mut Commands,
    units: &Query<(Entity, &mut UnitBase)>,
    only_this_phase: bool,
) {
    for (entity, unit) in units.iter() {
        let should_despawn = if only_this_phase {
            unit.is_killed && unit.killed_this_phase
        } else {
            unit.is_killed
        };
        if !should_despawn {
            continue;
        }
        if let Some(arrows) = timeline.live_arrows.remove(&entity) {
            for arrow in arrows {
                commands.entity(arrow).despawn_recursive();
            }
        }
        timeline.live_cumulative_distance.remove(&entity);
        if let Some(ghost) = timeline.ghost_entities.remove(&entity) {
            commands.entity(ghost).despawn_recursive();
        }
        if let Some(rings) = timeline.ring_entities.remove(&entity) {
            for ring in rings {
                commands.entity(ring).despawn_recursive();
            }
        }
        timeline.phase_start_positions.remove(&entity);
        timeline.live_unit_positions.remove(&entity);
        commands.entity(entity).despawn_recursive();
    }
}

fn on_advance_phase(
    mut events: EventReader<AdvancePhase>,
    mut phase_state: ResMut<PhaseState>,
    mut timeline: ResMut<GameTimeline>,
    mut units: Query<(Entity, &mut UnitBase)>,
    mut commands: Commands,
    mut ev_record: EventWriter<RecordSnapshot>,
    mut next_tool: ResMut<NextState<ActiveTool>>,
) {
    for _ in events.read() {
        if !timeline.locked {
            continue;
        }

        let current = phase_state.phase;
        let next = current.next();

        match current {
            GamePhase::Movement => {
                let turn = phase_state.turn_number.max(1);
                let player_str = phase_state.active_player.label();
                ev_record.send(RecordSnapshot {
                    label: format!("Turn {} — {} Move", turn, player_str),
                });
            }
            GamePhase::Shooting => {
                despawn_killed_units(&mut timeline, &mut commands, &units, true);
            }
            GamePhase::Fight => {
                despawn_killed_units(&mut timeline, &mut commands, &units, false);

                for (_, mut unit) in &mut units {
                    if !unit.is_killed {
                        unit.has_advanced = false;
                        unit.has_fallen_back = false;
                        unit.is_performing_action = false;
                        unit.is_battleshocked = false;
                        unit.killed_this_phase = false;
                    }
                }

                let turn = phase_state.turn_number.max(1);
                let player_str = phase_state.active_player.label();
                ev_record.send(RecordSnapshot {
                    label: format!("Turn {} — {} Fight", turn, player_str),
                });

                phase_state.turn_number += 1;
                phase_state.active_player = phase_state.active_player.other();
            }
            _ => {}
        }

        // Reset killed_this_phase at every phase boundary.
        for (_, mut unit) in &mut units {
            unit.killed_this_phase = false;
        }

        timeline.live_cumulative_distance.clear();

        let new_phase = match next {
            Some(p) => p,
            None => GamePhase::Command,
        };
        phase_state.phase = new_phase;

        // Switch to the new phase's default tool.
        next_tool.set(new_phase.default_tool());
    }
}

// ── Arrow spawning ────────────────────────────────────────────────────────────

pub fn spawn_movement_arrow(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<ColorMaterial>,
    unit: Entity,
    from: Vec2,
    to: Vec2,
    move_type: MoveType,
) -> Entity {
    let diff = to - from;
    let len = diff.length();

    if len < 0.05 {
        return commands
            .spawn((
                Transform::from_xyz(from.x, from.y, 3.5),
                Visibility::Inherited,
                MovementArrow { unit, from, to, move_type },
            ))
            .id();
    }

    let midpoint = (from + to) / 2.0;
    let angle = diff.to_angle();
    let head_size: f32 = 0.55;
    let body_len = (len - head_size).max(0.01);

    let arrow_color = move_type.color();
    let mat = materials.add(ColorMaterial::from_color(arrow_color));
    let dist_label = format!("{:.1}\"", len);

    commands
        .spawn((
            Transform::from_xyz(midpoint.x, midpoint.y, 3.5)
                .with_rotation(Quat::from_rotation_z(angle)),
            Visibility::Inherited,
            MovementArrow { unit, from, to, move_type },
        ))
        .with_children(|parent| {
            if move_type.is_dashed() {
                // Dashed shaft for charge arrows.
                let dash_len: f32 = 0.3;
                let gap_len: f32 = 0.15;
                let total_len = body_len;
                let mut offset = -total_len / 2.0;
                while offset < total_len / 2.0 {
                    let seg_len = dash_len.min(total_len / 2.0 - offset);
                    if seg_len > 0.01 {
                        parent.spawn((
                            Mesh2d(meshes.add(Rectangle::new(seg_len, 0.18))),
                            MeshMaterial2d(mat.clone()),
                            Transform::from_xyz(
                                offset + seg_len / 2.0 - head_size / 2.0,
                                0.0,
                                0.0,
                            ),
                            PickingBehavior::IGNORE,
                        ));
                    }
                    offset += dash_len + gap_len;
                }
            } else {
                // Solid shaft.
                parent.spawn((
                    Mesh2d(meshes.add(Rectangle::new(body_len, 0.18))),
                    MeshMaterial2d(mat.clone()),
                    Transform::from_xyz(-(head_size / 2.0), 0.0, 0.0),
                    PickingBehavior::IGNORE,
                ));
            }

            // Arrowhead.
            parent.spawn((
                Mesh2d(meshes.add(Triangle2d::new(
                    Vec2::new(head_size / 2.0, 0.0),
                    Vec2::new(-head_size / 2.0, head_size / 2.0),
                    Vec2::new(-head_size / 2.0, -head_size / 2.0),
                ))),
                MeshMaterial2d(mat.clone()),
                Transform::from_xyz(len / 2.0 - head_size / 2.0, 0.0, 0.0),
                PickingBehavior::IGNORE,
            ));

            // Distance label.
            parent.spawn((
                Text2d::new(dist_label),
                TextFont { font_size: 10.0, ..default() },
                TextColor(Color::BLACK),
                Transform::from_xyz(0.0, 0.3, 0.0)
                    .with_rotation(Quat::from_rotation_z(-angle))
                    .with_scale(Vec3::splat(0.08)),
                PickingBehavior::IGNORE,
            ));
        })
        .id()
}
