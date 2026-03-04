use std::collections::{HashMap, HashSet};

use bevy::prelude::*;

use crate::events::{AdvancePhase, LockDeployment, RecordSnapshot, RewindToSnapshot, UnitMoved};
use crate::resources::PhaseState;
use crate::types::phase::GamePhase;
use crate::types::timeline::{
    AdvanceIndicator, FirstPlayer, GameTimeline, GhostUnit, MovementArrow, MovementRangeRing,
    TimelineSnapshot,
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

        // Snapshot deployment positions with no advances.
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

        // Spawn ghost and ring entities for each unit.
        for (entity, transform, unit_base) in units.iter() {
            let pos = transform.translation.truncate();
            let rx = unit_base.base_shape.radius_x_inches();
            let ry = unit_base.base_shape.radius_y_inches();

            // Ghost — semi-transparent copy at phase-start position.
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

            // Range rings — only for units with movement data.
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

        // current_index = snapshots.len() = live sentinel (index 1 after pushing).
        timeline.current_index = timeline.snapshots.len();
    }
}

fn on_unit_moved(
    mut events: EventReader<UnitMoved>,
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<ColorMaterial>>,
    mut timeline: ResMut<GameTimeline>,
    units: Query<&UnitBase>,
) {
    for ev in events.read() {
        if !timeline.locked || timeline.current_index < timeline.snapshots.len() {
            continue;
        }

        // Despawn old arrow for this unit if it exists.
        if let Some(old_arrow) = timeline.live_arrows.remove(&ev.entity) {
            commands.entity(old_arrow).despawn_recursive();
        }

        let color = units
            .get(ev.entity)
            .map(|b| b.color)
            .unwrap_or(Color::WHITE);

        let arrow_entity = spawn_movement_arrow(
            &mut commands,
            &mut *meshes,
            &mut *materials,
            ev.entity,
            ev.from,
            ev.to,
            color,
        );

        timeline.live_arrows.insert(ev.entity, arrow_entity);
        timeline.live_unit_positions.insert(ev.entity, ev.to);

        // Make this unit's ghost visible.
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

        // Collect unit states before touching timeline.
        let unit_states: Vec<(Entity, Vec2, bool)> = units
            .iter()
            .map(|(entity, transform, unit_base)| {
                let current_pos = transform.translation.truncate();
                let advanced = if let Some(m) = unit_base.movement_inches {
                    let start = timeline
                        .phase_start_positions
                        .get(&entity)
                        .copied()
                        .unwrap_or(current_pos);
                    current_pos.distance(start) > m + 0.01
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

        let arrow_entities: Vec<Entity> = timeline.live_arrows.values().copied().collect();
        let player = timeline.active_player_in_live_view();

        // Hide all arrows, ghosts, and rings — fresh phase, nothing shown yet.
        for mut vis in &mut all_vis {
            *vis = Visibility::Hidden;
        }

        timeline.snapshots.push(TimelineSnapshot {
            label: ev.label.clone(),
            player,
            positions,
            arrow_entities,
        });

        // Advance phase: new start positions = current positions.
        for &(entity, current_pos, _) in &unit_states {
            timeline.phase_start_positions.insert(entity, current_pos);
            timeline.live_unit_positions.insert(entity, current_pos);
        }

        timeline.live_arrows.clear();
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
            timeline.live_arrows.values().copied().collect()
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

        // Build lookup structures for ghost and ring classification.
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

        // Update visibility: arrows shown per snapshot, ghosts shown in live when moved, rings hidden.
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

        // Teleport units.
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
                let start = timeline
                    .phase_start_positions
                    .get(&entity)
                    .copied()
                    .unwrap_or_else(|| transform.translation.truncate());
                transform.translation.truncate().distance(start) > m + 0.01
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
        if let Some(arrow) = timeline.live_arrows.remove(&entity) {
            commands.entity(arrow).despawn_recursive();
        }
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

// ── Phase advancement ─────────────────────────────────────────────────────────

fn on_advance_phase(
    mut events: EventReader<AdvancePhase>,
    mut phase_state: ResMut<PhaseState>,
    mut timeline: ResMut<GameTimeline>,
    mut units: Query<(Entity, &mut UnitBase)>,
    mut commands: Commands,
    mut ev_record: EventWriter<RecordSnapshot>,
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
                // Despawn units killed during shooting.
                for (entity, unit) in &units {
                    if unit.is_killed && unit.killed_this_phase {
                        if let Some(arrow) = timeline.live_arrows.remove(&entity) {
                            commands.entity(arrow).despawn_recursive();
                        }
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
            }
            GamePhase::Fight => {
                // Despawn all killed units.
                for (entity, unit) in &units {
                    if unit.is_killed {
                        if let Some(arrow) = timeline.live_arrows.remove(&entity) {
                            commands.entity(arrow).despawn_recursive();
                        }
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

                // Reset phase flags on surviving units.
                for (_, mut unit) in &mut units {
                    if !unit.is_killed {
                        unit.has_advanced = false;
                        unit.is_performing_action = false;
                        unit.killed_this_phase = false;
                    }
                }

                // Record end-of-turn snapshot.
                let turn = phase_state.turn_number.max(1);
                let player_str = phase_state.active_player.label();
                ev_record.send(RecordSnapshot {
                    label: format!("Turn {} — {} Fight", turn, player_str),
                });

                // Increment turn and swap active player after Fight.
                phase_state.turn_number += 1;
                phase_state.active_player = phase_state.active_player.other();
            }
            _ => {}
        }

        // Despawn transient rings.
        if let Some(ring) = phase_state.charge_ring_entity.take() {
            commands.entity(ring).despawn_recursive();
        }
        if let Some(ring) = phase_state.shooter_range_ring.take() {
            commands.entity(ring).despawn_recursive();
        }

        // Clear all per-phase selection state.
        phase_state.selected_shooter = None;
        phase_state.selected_weapon_idx = None;
        phase_state.pending_target = None;
        phase_state.declared_charger = None;
        phase_state.declared_charge_target = None;
        phase_state.charge_declared = None;
        phase_state.pending_kill_target = None;

        // Reset killed_this_phase for all units at phase boundary.
        for (_, mut unit) in &mut units {
            unit.killed_this_phase = false;
        }

        phase_state.phase = match next {
            Some(p) => p,
            None => GamePhase::Command,
        };
    }
}

// ── Arrow spawning ────────────────────────────────────────────────────────────

fn spawn_movement_arrow(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<ColorMaterial>,
    unit: Entity,
    from: Vec2,
    to: Vec2,
    color: Color,
) -> Entity {
    let diff = to - from;
    let len = diff.length();

    // Degenerate arrow — still spawn the marker entity for tracking.
    if len < 0.05 {
        return commands
            .spawn((
                Transform::from_xyz(from.x, from.y, 3.5),
                Visibility::Inherited,
                MovementArrow { unit, from, to },
            ))
            .id();
    }

    let midpoint = (from + to) / 2.0;
    let angle = diff.to_angle();
    let head_size: f32 = 0.4;
    let body_len = (len - head_size).max(0.01);

    let srgba = color.to_srgba();
    let arrow_color = Color::srgba(srgba.red, srgba.green, srgba.blue, 0.75);
    let mat = materials.add(ColorMaterial::from_color(arrow_color));
    let dist_label = format!("{:.1}\"", len);

    commands
        .spawn((
            Transform::from_xyz(midpoint.x, midpoint.y, 3.5)
                .with_rotation(Quat::from_rotation_z(angle)),
            Visibility::Inherited,
            MovementArrow { unit, from, to },
        ))
        .with_children(|parent| {
            // Shaft — centered in local space, shifted left so head occupies the tip.
            parent.spawn((
                Mesh2d(meshes.add(Rectangle::new(body_len, 0.08))),
                MeshMaterial2d(mat.clone()),
                Transform::from_xyz(-(head_size / 2.0), 0.0, 0.0),
                PickingBehavior::IGNORE,
            ));

            // Arrowhead — triangle pointing in local +X.
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
                TextColor(Color::WHITE),
                Transform::from_xyz(0.0, 0.3, 0.0).with_scale(Vec3::splat(0.08)),
                PickingBehavior::IGNORE,
            ));
        })
        .id()
}
