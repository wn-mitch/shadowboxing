use bevy::prelude::*;

use crate::army_list::base_lookup::BaseDatabase;
use crate::events::{
    ClearPlayerUnits, ConfirmAction, ConfirmKill, RemoveModelUnits, SpawnUnit, UnitMoved,
};
use crate::resources::{
    ActiveLayout, ActivePattern, BoardConfig, BattleshockToolState, ChargeToolState,
    DeploymentPatterns, EnforceMaxMove, KillToolState, OverlaySettings, PhaseState,
    RangeRingToolState, ShootToolState, TerrainLayouts,
};
use crate::types::phase::ActiveTool;
use crate::types::terrain::TerrainPiece;
use crate::types::timeline::{
    AdvanceIndicator, ChargeRangeRing, GameTimeline, MoveType, MovementRangeRing, ShooterRangeRing,
};
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
            .add_event::<ConfirmKill>()
            .add_event::<ConfirmAction>()
            .add_systems(
                Update,
                (
                    on_spawn_unit,
                    update_validity_indicators,
                    sync_validity_rings,
                    on_clear_player_units,
                    on_remove_model_units,
                    // Shared systems — always run
                    confirm_kills,
                    confirm_action_flag,
                    sync_unit_tint,
                    sync_killed_unit_tint,
                ),
            )
            // Drag — during deployment (unlocked) or when a movement tool is active
            .add_systems(
                Update,
                handle_drag.run_if(
                    |tool: Res<State<ActiveTool>>, timeline: Res<GameTimeline>| {
                        !timeline.locked || tool.get().is_movement_tool()
                    },
                ),
            )
            // Per-tool click handlers
            .add_systems(
                Update,
                handle_select_click.run_if(in_state(ActiveTool::Select)),
            )
            .add_systems(
                Update,
                handle_kill_click.run_if(in_state(ActiveTool::Kill)),
            )
            .add_systems(
                Update,
                handle_shoot_click.run_if(in_state(ActiveTool::ShootAnnotate)),
            )
            .add_systems(
                Update,
                (handle_charge_click, sync_charge_ring_position)
                    .run_if(in_state(ActiveTool::Charge)),
            )
            .add_systems(
                Update,
                handle_battleshock_click.run_if(in_state(ActiveTool::Battleshock)),
            )
            .add_systems(
                Update,
                handle_rangering_click.run_if(in_state(ActiveTool::RangeRing)),
            )
            // Analysis mode click — runs pre-lock regardless of tool
            .add_systems(Update, handle_analysis_click)
            // OnExit cleanup
            .add_systems(OnExit(ActiveTool::Kill), cleanup_kill_tool)
            .add_systems(OnExit(ActiveTool::ShootAnnotate), cleanup_shoot_tool)
            .add_systems(OnExit(ActiveTool::Charge), cleanup_charge_tool)
            .add_systems(OnExit(ActiveTool::Battleshock), cleanup_battleshock_tool)
            .add_systems(OnExit(ActiveTool::RangeRing), cleanup_rangering_tool);
    }
}

// ── Cleanup systems ──────────────────────────────────────────────────────────

fn cleanup_kill_tool(mut state: ResMut<KillToolState>) {
    *state = default();
}

fn cleanup_shoot_tool(
    mut commands: Commands,
    mut state: ResMut<ShootToolState>,
    rings: Query<Entity, With<ShooterRangeRing>>,
) {
    *state = default();
    for ring in &rings {
        commands.entity(ring).despawn_recursive();
    }
}

fn cleanup_charge_tool(
    mut commands: Commands,
    mut state: ResMut<ChargeToolState>,
    rings: Query<Entity, With<ChargeRangeRing>>,
) {
    *state = default();
    for ring in &rings {
        commands.entity(ring).despawn_recursive();
    }
}

fn cleanup_battleshock_tool(mut state: ResMut<BattleshockToolState>) {
    *state = default();
}

fn cleanup_rangering_tool(mut state: ResMut<RangeRingToolState>) {
    // Keep rings alive — just clear selection.
    state.selected_unit = None;
}

// ── Validity ─────────────────────────────────────────────────────────────────

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

// ── Spawning ─────────────────────────────────────────────────────────────────

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
                continue;
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
                has_advanced: false,
                has_fallen_back: false,
                is_performing_action: false,
                is_battleshocked: false,
                is_killed: false,
                killed_this_phase: false,
            },
            PickingBehavior::default(),
        ))
        .with_children(|parent| {
            // White outline ring.
            parent.spawn((
                Mesh2d(meshes.add(Annulus::new(ring_inner, ring_inner + 0.12))),
                MeshMaterial2d(materials.add(ColorMaterial::from_color(Color::WHITE))),
                Transform::from_xyz(0.0, 0.0, 0.05),
            ));

            // Zone violation ring (hidden by default).
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

            // "ADV" badge.
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

// ── Per-tool click handlers ──────────────────────────────────────────────────

/// Select tool: show range rings (Movement), display unit info.
fn handle_select_click(
    mut click_events: EventReader<Pointer<Click>>,
    bases: Query<(Entity, &UnitBase)>,
    timeline: Res<GameTimeline>,
    phase_state: Res<PhaseState>,
    mut ring_query: Query<&mut Visibility, With<MovementRangeRing>>,
) {
    for ev in click_events.read() {
        let Ok((clicked_entity, _clicked_unit)) = bases.get(ev.target) else {
            continue;
        };

        if timeline.locked {
            // Show range rings for clicked unit.
            let to_show = timeline.ring_entities.get(&clicked_entity).copied();
            for mut vis in &mut ring_query {
                *vis = Visibility::Hidden;
            }
            if let Some([nr, ar]) = to_show {
                if let Ok(mut v) = ring_query.get_mut(nr) {
                    *v = Visibility::Visible;
                }
                if let Ok(mut v) = ring_query.get_mut(ar) {
                    *v = Visibility::Visible;
                }
            }
        }
    }
}

/// Analysis mode selection — runs pre-lock regardless of tool.
fn handle_analysis_click(
    mut click_events: EventReader<Pointer<Click>>,
    bases: Query<Entity, With<UnitBase>>,
    vis_state: Res<VisibilityState>,
    mut selected_unit: ResMut<SelectedUnitForAnalysis>,
    timeline: Res<GameTimeline>,
) {
    if timeline.locked || vis_state.mode != AnalysisMode::UnitPositions {
        return;
    }
    for ev in click_events.read() {
        let Ok(clicked_entity) = bases.get(ev.target) else {
            continue;
        };
        selected_unit.0 = match selected_unit.0 {
            Some(e) if e == clicked_entity => None,
            _ => Some(clicked_entity),
        };
    }
}

/// Kill tool: click any non-killed unit to set as pending target.
fn handle_kill_click(
    mut click_events: EventReader<Pointer<Click>>,
    bases: Query<(Entity, &UnitBase)>,
    mut kill_state: ResMut<KillToolState>,
    timeline: Res<GameTimeline>,
) {
    if !timeline.locked {
        return;
    }
    for ev in click_events.read() {
        let Ok((clicked_entity, clicked_unit)) = bases.get(ev.target) else {
            continue;
        };
        if !clicked_unit.is_killed {
            kill_state.pending_target = Some(clicked_entity);
        }
    }
}

/// ShootAnnotate tool: friendly → shooter, enemy → target.
fn handle_shoot_click(
    mut click_events: EventReader<Pointer<Click>>,
    bases: Query<(Entity, &UnitBase, &Transform)>,
    mut shoot_state: ResMut<ShootToolState>,
    phase_state: Res<PhaseState>,
    timeline: Res<GameTimeline>,
    base_db: Option<Res<BaseDatabase>>,
    mut commands: Commands,
    rings: Query<Entity, With<ShooterRangeRing>>,
) {
    if !timeline.locked {
        return;
    }
    for ev in click_events.read() {
        let Ok((clicked_entity, clicked_unit, clicked_transform)) = bases.get(ev.target) else {
            continue;
        };
        let is_friendly = clicked_unit.player == phase_state.active_player;
        let is_enemy = clicked_unit.player != phase_state.active_player;

        if is_friendly && !clicked_unit.is_killed {
            // Despawn old shooter ring.
            for ring in &rings {
                commands.entity(ring).despawn_recursive();
            }
            shoot_state.selected_shooter = Some(clicked_entity);
            shoot_state.selected_weapon_idx = None;
            shoot_state.pending_target = None;
        } else if is_enemy && !clicked_unit.is_killed {
            if shoot_state.selected_shooter.is_some() && shoot_state.selected_weapon_idx.is_some() {
                let in_range = check_weapon_range(
                    &bases,
                    &shoot_state,
                    clicked_entity,
                    clicked_unit,
                    clicked_transform,
                    base_db.as_deref(),
                );
                if in_range {
                    shoot_state.pending_target = Some(clicked_entity);
                }
            }
        }
    }
}

fn check_weapon_range(
    bases: &Query<(Entity, &UnitBase, &Transform)>,
    shoot_state: &ShootToolState,
    _target_entity: Entity,
    target_unit: &UnitBase,
    target_transform: &Transform,
    base_db: Option<&BaseDatabase>,
) -> bool {
    let Some(db) = base_db else { return true };
    let Some(shooter_entity) = shoot_state.selected_shooter else { return false };
    let Ok((_, shooter_unit, shooter_transform)) = bases.get(shooter_entity) else {
        return false;
    };
    let Some(wi) = shoot_state.selected_weapon_idx else { return false };
    let weapons: Vec<_> = db
        .weapons_for_unit(&shooter_unit.unit_name)
        .iter()
        .filter(|w| w.range.trim() != "Melee")
        .collect();
    let Some(weapon) = weapons.get(wi) else { return false };
    let Some(range) = BaseDatabase::weapon_range_inches(weapon) else {
        return false;
    };
    let shooter_r = shooter_unit
        .base_shape
        .radius_x_inches()
        .max(shooter_unit.base_shape.radius_y_inches());
    let target_r = target_unit
        .base_shape
        .radius_x_inches()
        .max(target_unit.base_shape.radius_y_inches());
    let center_dist = shooter_transform
        .translation
        .truncate()
        .distance(target_transform.translation.truncate());
    let edge_dist = (center_dist - shooter_r - target_r).max(0.0);
    edge_dist <= range
}

/// Charge tool: friendly → charger, enemy → add to targets.
fn handle_charge_click(
    mut click_events: EventReader<Pointer<Click>>,
    bases: Query<(Entity, &UnitBase, &Transform)>,
    mut charge_state: ResMut<ChargeToolState>,
    phase_state: Res<PhaseState>,
    timeline: Res<GameTimeline>,
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<ColorMaterial>>,
    rings: Query<Entity, With<ChargeRangeRing>>,
) {
    if !timeline.locked {
        return;
    }
    for ev in click_events.read() {
        let Ok((clicked_entity, clicked_unit, clicked_transform)) = bases.get(ev.target) else {
            continue;
        };
        let is_friendly = clicked_unit.player == phase_state.active_player;
        let is_enemy = clicked_unit.player != phase_state.active_player;

        if is_friendly
            && !clicked_unit.is_killed
            && !clicked_unit.has_advanced
            && !clicked_unit.is_performing_action
        {
            // Despawn old charge ring.
            for ring in &rings {
                commands.entity(ring).despawn_recursive();
            }

            charge_state.declared_charger = Some(clicked_entity);
            charge_state.charge_targets.clear();
            charge_state.charge_declared = None;

            // Spawn 12" charge range ring.
            let charger_radius = clicked_unit
                .base_shape
                .radius_x_inches()
                .max(clicked_unit.base_shape.radius_y_inches());
            let ring_r = 12.0 + charger_radius;
            let pos = clicked_transform.translation.truncate();
            commands.spawn((
                Mesh2d(meshes.add(Annulus::new(ring_r, ring_r + 0.12))),
                MeshMaterial2d(materials.add(ColorMaterial::from_color(
                    Color::srgba(1.0, 0.5, 0.0, 0.85),
                ))),
                Transform::from_xyz(pos.x, pos.y, 0.5),
                Visibility::Visible,
                ChargeRangeRing,
                PickingBehavior::IGNORE,
            ));
        } else if is_enemy && !clicked_unit.is_killed {
            if charge_state.declared_charger.is_some() {
                if !charge_state.charge_targets.contains(&clicked_entity) {
                    charge_state.charge_targets.push(clicked_entity);
                }
            }
        }
    }
}

/// Keep the charge ring centred on the declared charger's current position.
fn sync_charge_ring_position(
    charge_state: Res<ChargeToolState>,
    units: Query<&Transform, With<UnitBase>>,
    mut rings: Query<&mut Transform, (With<ChargeRangeRing>, Without<UnitBase>)>,
) {
    let Some(charger) = charge_state.declared_charger else {
        return;
    };
    if let Ok(unit_t) = units.get(charger) {
        for mut ring_t in &mut rings {
            ring_t.translation.x = unit_t.translation.x;
            ring_t.translation.y = unit_t.translation.y;
        }
    }
}

/// Battleshock tool: click unit to set as pending.
fn handle_battleshock_click(
    mut click_events: EventReader<Pointer<Click>>,
    bases: Query<(Entity, &UnitBase)>,
    mut bs_state: ResMut<BattleshockToolState>,
    timeline: Res<GameTimeline>,
) {
    if !timeline.locked {
        return;
    }
    for ev in click_events.read() {
        let Ok((clicked_entity, clicked_unit)) = bases.get(ev.target) else {
            continue;
        };
        if !clicked_unit.is_killed {
            bs_state.pending_target = Some(clicked_entity);
        }
    }
}

/// RangeRing tool: click unit to select for ring placement.
fn handle_rangering_click(
    mut click_events: EventReader<Pointer<Click>>,
    bases: Query<(Entity, &UnitBase)>,
    mut rr_state: ResMut<RangeRingToolState>,
    timeline: Res<GameTimeline>,
) {
    if !timeline.locked {
        return;
    }
    for ev in click_events.read() {
        let Ok((clicked_entity, _)) = bases.get(ev.target) else {
            continue;
        };
        rr_state.selected_unit = Some(clicked_entity);
    }
}

// ── Tint systems ─────────────────────────────────────────────────────────────

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
    phase_state: Res<PhaseState>,
    units: Query<(&UnitBase, &MeshMaterial2d<ColorMaterial>)>,
    mut materials: ResMut<Assets<ColorMaterial>>,
) {
    for (unit_base, mat_handle) in &units {
        let target = if timeline.locked {
            if unit_base.player == phase_state.active_player {
                unit_base.color
            } else {
                grey_tint(unit_base.color)
            }
        } else {
            unit_base.color
        };

        let needs_update = materials.get(mat_handle.id()).map(|m| m.color != target).unwrap_or(false);
        if needs_update {
            if let Some(mat) = materials.get_mut(mat_handle.id()) {
                mat.color = target;
            }
        }
    }
}

fn sync_killed_unit_tint(
    units: Query<(&UnitBase, &MeshMaterial2d<ColorMaterial>), Changed<UnitBase>>,
    mut materials: ResMut<Assets<ColorMaterial>>,
) {
    for (unit_base, mat_handle) in &units {
        if unit_base.is_killed {
            if let Some(mat) = materials.get_mut(mat_handle.id()) {
                let s = unit_base.color.to_srgba();
                mat.color = Color::srgba(s.red, s.green, s.blue, 0.3);
            }
        }
    }
}

// ── Event-driven confirm systems ─────────────────────────────────────────────

fn confirm_kills(
    mut events: EventReader<ConfirmKill>,
    mut units: Query<&mut UnitBase>,
) {
    for ev in events.read() {
        if let Ok(mut unit) = units.get_mut(ev.0) {
            unit.is_killed = true;
            unit.killed_this_phase = true;
        }
    }
}

fn confirm_action_flag(
    mut events: EventReader<ConfirmAction>,
    mut units: Query<&mut UnitBase>,
) {
    for ev in events.read() {
        if let Ok(mut unit) = units.get_mut(ev.0) {
            unit.is_performing_action = true;
        }
    }
}

// ── Drag handling ────────────────────────────────────────────────────────────

fn handle_drag(
    mut bases: Query<(Entity, &mut Transform, &mut UnitBase)>,
    mut drag_events: EventReader<Pointer<Drag>>,
    mut drag_end_events: EventReader<Pointer<DragEnd>>,
    board: Res<BoardConfig>,
    layouts: Res<TerrainLayouts>,
    active_layout: Res<ActiveLayout>,
    camera_q: Query<(&Camera, &GlobalTransform)>,
    timeline: Res<GameTimeline>,
    phase_state: Res<PhaseState>,
    tool: Res<State<ActiveTool>>,
    charge_state: Res<ChargeToolState>,
    enforce_max: Res<EnforceMaxMove>,
    mut ev_unit_moved: EventWriter<UnitMoved>,
) {
    let terrain_pieces: Vec<TerrainPiece> = active_layout
        .0
        .as_ref()
        .and_then(|id| layouts.0.iter().find(|l| &l.id == id))
        .map(|l| l.pieces.clone())
        .unwrap_or_default();

    let unit_snapshot: Vec<(Entity, Vec2, BaseShape)> = bases
        .iter()
        .map(|(e, t, ub)| (e, t.translation.truncate(), ub.base_shape.clone()))
        .collect();

    let active_tool = *tool.get();

    for ev in drag_events.read() {
        let Ok((entity, mut transform, unit_base)) = bases.get_mut(ev.target) else {
            continue;
        };
        if unit_base.locked {
            continue;
        }
        if timeline.locked && unit_base.player != phase_state.active_player {
            continue;
        }
        // Charge tool: only declared charger after success.
        if active_tool == ActiveTool::Charge {
            if charge_state.charge_declared != Some(true) {
                continue;
            }
            if charge_state.declared_charger != Some(entity) {
                continue;
            }
        }

        let delta_world = if let Ok((cam, cam_gt)) = camera_q.get_single() {
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
        if timeline.locked && unit_base.player != phase_state.active_player {
            transform.translation.x = unit_base.last_valid_pos.x;
            transform.translation.y = unit_base.last_valid_pos.y;
            continue;
        }
        // Charge: snap back if not declared charger after success.
        if active_tool == ActiveTool::Charge {
            let is_charger = charge_state.declared_charger == Some(entity)
                && charge_state.charge_declared == Some(true);
            if !is_charger {
                transform.translation.x = unit_base.last_valid_pos.x;
                transform.translation.y = unit_base.last_valid_pos.y;
                continue;
            }
        }

        // Historical view: snap back.
        if timeline.locked && timeline.current_index < timeline.snapshots.len() {
            transform.translation.x = unit_base.last_valid_pos.x;
            transform.translation.y = unit_base.last_valid_pos.y;
            continue;
        }

        let pos = transform.translation.truncate();
        let rx = unit_base.base_shape.radius_x_inches();
        let ry = unit_base.base_shape.radius_y_inches();

        let clamped = Vec2::new(
            pos.x.clamp(rx, board.width - rx),
            pos.y.clamp(ry, board.height - ry),
        );

        // PileIn / Consolidate: enforce 3" max cumulative path distance.
        if matches!(active_tool, ActiveTool::PileIn | ActiveTool::Consolidate) {
            let cumulative = timeline.live_cumulative_distance.get(&entity).copied().unwrap_or(0.0);
            let segment_dist = unit_base.last_valid_pos.distance(clamped);
            if cumulative + segment_dist > 3.0 {
                transform.translation.x = unit_base.last_valid_pos.x;
                transform.translation.y = unit_base.last_valid_pos.y;
                continue;
            }
        }

        let blocked = overlaps_any_terrain(clamped, &unit_base.base_shape, &terrain_pieces)
            || unit_snapshot.iter().any(|(other, other_pos, other_shape)| {
                *other != entity && bases_overlap(clamped, &unit_base.base_shape, *other_pos, other_shape)
            });
        if blocked {
            transform.translation.x = unit_base.last_valid_pos.x;
            transform.translation.y = unit_base.last_valid_pos.y;
        } else {
            // Each segment starts from the previous endpoint (multi-segment pathing).
            let from = unit_base.last_valid_pos;
            let segment_dist = from.distance(clamped);

            // Enforce max movement distance when enabled.
            if enforce_max.0 && timeline.locked {
                let cumulative = timeline.live_cumulative_distance.get(&entity).copied().unwrap_or(0.0);
                let max_dist = match active_tool {
                    ActiveTool::Advance => unit_base.movement_inches.map(|m| m + 6.0),
                    _ => unit_base.movement_inches,
                };
                if let Some(max) = max_dist {
                    if cumulative + segment_dist > max {
                        transform.translation.x = unit_base.last_valid_pos.x;
                        transform.translation.y = unit_base.last_valid_pos.y;
                        continue;
                    }
                }
            }

            transform.translation.x = clamped.x;
            transform.translation.y = clamped.y;
            unit_base.last_valid_pos = clamped;

            // Determine MoveType from active tool.
            let move_type = match active_tool {
                ActiveTool::Move => MoveType::Normal,
                ActiveTool::Advance => {
                    unit_base.has_advanced = true;
                    MoveType::Advance
                }
                ActiveTool::FallBack => {
                    unit_base.has_fallen_back = true;
                    MoveType::FallBack
                }
                ActiveTool::Reactive => MoveType::Reactive,
                ActiveTool::PileIn => MoveType::PileIn,
                ActiveTool::Consolidate => MoveType::Consolidate,
                ActiveTool::Charge => MoveType::Charge,
                _ => MoveType::Normal,
            };

            if timeline.locked {
                ev_unit_moved.send(UnitMoved {
                    entity,
                    from,
                    to: clamped,
                    move_type,
                });
            }
        }
    }
}
