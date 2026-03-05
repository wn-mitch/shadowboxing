use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use bevy_egui::{egui, EguiContexts};

use crate::army_list::base_lookup::{BaseDatabase, BaseDatabase as BD};
use crate::army_list::parse_listforge;
use crate::events::{
    AdvancePhase, ClearAnalysis, ClearPlayerUnits, ConfirmAction, ConfirmKill, LockDeployment,
    LoadDeploymentPattern, LoadTerrainLayout, RemoveModelUnits, RewindToSnapshot, SpawnUnit,
    TriggerAnalysis,
};
use crate::resources::{
    ActiveLayout, ActivePattern, BattleshockToolState, ChargeToolState, DeploymentPatterns,
    EnforceMaxMove, KillToolState, OverlaySettings, PanelWidth, PhaseState, RangeRingToolState,
    RightPanelWidth, ShootToolState, TerrainLayouts,
};
use crate::types::phase::{ActiveTool, GamePhase};
use crate::types::timeline::{
    GameTimeline, PersistentRangeRing, ShooterRangeRing,
};
use crate::types::units::{ArmyUnit, Player, UnitBase};
use crate::types::visibility::{AnalysisMode, VisibilityState};

pub struct UiPlugin;

impl Plugin for UiPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<UiState>()
            .init_resource::<PanelWidth>()
            .add_systems(Update, draw_left_panel)
            .add_systems(Update, draw_right_panel);
    }
}

/// Bundles per-tool state resources to stay under 16-param limit.
#[derive(SystemParam)]
struct ToolStates<'w> {
    kill: ResMut<'w, KillToolState>,
    shoot: ResMut<'w, ShootToolState>,
    charge: ResMut<'w, ChargeToolState>,
    battleshock: ResMut<'w, BattleshockToolState>,
    range_ring: ResMut<'w, RangeRingToolState>,
    enforce_max: ResMut<'w, EnforceMaxMove>,
}

/// Bundles timeline events.
#[derive(SystemParam)]
struct TimelineEvents<'w> {
    lock: EventWriter<'w, LockDeployment>,
    rewind: EventWriter<'w, RewindToSnapshot>,
    advance: EventWriter<'w, AdvancePhase>,
    confirm_kill: EventWriter<'w, ConfirmKill>,
    confirm_action: EventWriter<'w, ConfirmAction>,
}

#[derive(Resource)]
struct UiState {
    active_tab: UiTab,
    // Attacker
    attacker_list_text: String,
    attacker_units: Vec<ArmyUnit>,
    attacker_submitted: bool,
    // Defender
    defender_list_text: String,
    defender_units: Vec<ArmyUnit>,
    defender_submitted: bool,
    // shared
    movement_override: f32,
    selected_analysis_mode: AnalysisMode,
}

impl Default for UiState {
    fn default() -> Self {
        Self {
            active_tab: UiTab::default(),
            attacker_list_text: String::new(),
            attacker_units: Vec::new(),
            attacker_submitted: false,
            defender_list_text: String::new(),
            defender_units: Vec::new(),
            defender_submitted: false,
            movement_override: 0.0,
            selected_analysis_mode: AnalysisMode::UnitPositions,
        }
    }
}

#[derive(Default, PartialEq, Eq, Clone, Copy)]
enum UiTab {
    #[default]
    Setup,
    Army,
    Analysis,
}

const ATTACKER_COLOR: Color = Color::srgb(0.85, 0.15, 0.15);
const DEFENDER_COLOR: Color = Color::srgb(0.15, 0.35, 0.85);

fn draw_left_panel(
    mut contexts: EguiContexts,
    mut ui_state: ResMut<UiState>,
    vis_state: Res<VisibilityState>,
    layouts: Res<TerrainLayouts>,
    patterns: Res<DeploymentPatterns>,
    mut active_layout: ResMut<ActiveLayout>,
    mut active_pattern: ResMut<ActivePattern>,
    mut ev_load_layout: EventWriter<LoadTerrainLayout>,
    mut ev_load_pattern: EventWriter<LoadDeploymentPattern>,
    mut ev_trigger: EventWriter<TriggerAnalysis>,
    mut ev_clear: EventWriter<ClearAnalysis>,
    mut ev_spawn: EventWriter<SpawnUnit>,
    mut ev_clear_player: EventWriter<ClearPlayerUnits>,
    mut ev_remove: EventWriter<RemoveModelUnits>,
    mut panel_width: ResMut<PanelWidth>,
    mut overlay_settings: ResMut<OverlaySettings>,
) {
    let ctx = contexts.ctx_mut();

    let panel = egui::SidePanel::left("control_panel")
        .min_width(240.0)
        .max_width(300.0)
        .show(ctx, |ui| {
            ui.heading("Deployment Helper");
            ui.separator();

            ui.horizontal(|ui| {
                ui.selectable_value(&mut ui_state.active_tab, UiTab::Setup, "Setup");
                ui.selectable_value(&mut ui_state.active_tab, UiTab::Army, "Army");
                ui.selectable_value(&mut ui_state.active_tab, UiTab::Analysis, "Analysis");
            });
            ui.separator();

            match ui_state.active_tab {
                UiTab::Setup => draw_setup_tab(
                    ui,
                    &mut ui_state,
                    &layouts,
                    &patterns,
                    &mut active_layout,
                    &mut active_pattern,
                    &mut ev_load_layout,
                    &mut ev_load_pattern,
                    &mut overlay_settings,
                ),
                UiTab::Army => draw_army_tab(ui, &mut ui_state, &mut ev_spawn, &mut ev_clear_player, &mut ev_remove),
                UiTab::Analysis => draw_analysis_tab(
                    ui,
                    &mut ui_state,
                    &vis_state,
                    &mut ev_trigger,
                    &mut ev_clear,
                ),
            }
        });
    panel_width.0 = panel.response.rect.width();
}

fn draw_setup_tab(
    ui: &mut egui::Ui,
    _ui_state: &mut UiState,
    layouts: &TerrainLayouts,
    patterns: &DeploymentPatterns,
    active_layout: &mut ActiveLayout,
    active_pattern: &mut ActivePattern,
    ev_load_layout: &mut EventWriter<LoadTerrainLayout>,
    ev_load_pattern: &mut EventWriter<LoadDeploymentPattern>,
    overlay_settings: &mut OverlaySettings,
) {
    ui.label("Terrain Layout:");
    let current_layout = active_layout.0.clone().unwrap_or_default();
    egui::ComboBox::from_id_salt("terrain_layout")
        .selected_text(&current_layout)
        .show_ui(ui, |ui| {
            for layout in &layouts.0 {
                let selected = active_layout.0.as_deref() == Some(&layout.id);
                if ui.selectable_label(selected, &layout.name).clicked() {
                    active_layout.0 = Some(layout.id.clone());
                    ev_load_layout.send(LoadTerrainLayout(layout.id.clone()));
                }
            }
        });

    ui.add_space(8.0);
    ui.label("Deployment Pattern:");
    let current_pattern = active_pattern.0.clone().unwrap_or_default();
    egui::ComboBox::from_id_salt("deployment_pattern")
        .selected_text(&current_pattern)
        .show_ui(ui, |ui| {
            for pattern in &patterns.0 {
                let selected = active_pattern.0.as_deref() == Some(&pattern.id);
                if ui.selectable_label(selected, &pattern.name).clicked() {
                    active_pattern.0 = Some(pattern.id.clone());
                    ev_load_pattern.send(LoadDeploymentPattern(pattern.id.clone()));
                }
            }
        });

    ui.add_space(8.0);
    ui.collapsing("Display", |ui| {
        ui.checkbox(&mut overlay_settings.show_source_points, "Source Points (debug)");
        ui.checkbox(&mut overlay_settings.show_danger_region, "Danger Region");
        ui.checkbox(&mut overlay_settings.show_deployment_zones, "Deployment Zones");
        ui.checkbox(&mut overlay_settings.show_validity_rings, "Validity Rings");
        ui.checkbox(&mut overlay_settings.show_terrain_debug, "Terrain Labels & Dots");
        ui.checkbox(&mut overlay_settings.show_collision_boxes, "Collision Boxes");
    });
}

fn import_list(text: &str, player: Player) -> Vec<ArmyUnit> {
    let parsed = parse_listforge(text);
    let base_db = BaseDatabase::load(
        include_str!("../../assets/Datasheets.json"),
        include_str!("../../assets/Datasheets_models.json"),
        include_str!("../../assets/Datasheets_wargear.json"),
    );
    let color = match player {
        Player::Attacker => ATTACKER_COLOR,
        Player::Defender => DEFENDER_COLOR,
    };
    let mut army_units = Vec::new();
    for unit in parsed {
        let valid_models: Vec<(String, u32)> = unit
            .model_counts
            .iter()
            .filter(|(model_name, _)| base_db.has_model(&unit.name, model_name))
            .map(|(k, v)| (k.clone(), *v))
            .collect();
        let models_to_spawn: Vec<(String, u32)> = if valid_models.is_empty() {
            vec![(unit.name.clone(), 1)]
        } else {
            valid_models
        };
        for (model_name, count) in &models_to_spawn {
            let (base_shape, movement) = base_db.lookup(&unit.name, model_name);
            army_units.push(ArmyUnit {
                unit_name: unit.name.clone(),
                model_name: model_name.clone(),
                count: *count,
                placed: 0,
                base_shape,
                movement_inches: movement,
                color,
                player,
            });
        }
    }
    army_units
}

fn draw_player_section(
    ui: &mut egui::Ui,
    label: &str,
    label_color: egui::Color32,
    list_text: &mut String,
    units: &mut Vec<ArmyUnit>,
    submitted: &mut bool,
    player: Player,
    ev_spawn: &mut EventWriter<SpawnUnit>,
    ev_clear_player: &mut EventWriter<ClearPlayerUnits>,
    ev_remove: &mut EventWriter<RemoveModelUnits>,
) {
    ui.colored_label(label_color, label);
    if !*submitted {
        ui.add(
            egui::TextEdit::multiline(list_text)
                .desired_rows(6)
                .desired_width(f32::INFINITY),
        );
        if ui.button("Import List").clicked() {
            *units = import_list(list_text, player);
            *submitted = true;
        }
    } else {
        egui::ScrollArea::vertical()
            .id_salt(format!("{}_scroll", label))
            .show(ui, |ui| {
                let units_len = units.len();
                for i in 0..units_len {
                    let (unit_name, model_name, base_shape, base_shape_label, placed, count, color, movement_inches) = {
                        let u = &units[i];
                        (
                            u.unit_name.clone(),
                            u.model_name.clone(),
                            u.base_shape.clone(),
                            u.base_shape.label(),
                            u.placed,
                            u.count,
                            u.color,
                            u.movement_inches,
                        )
                    };

                    ui.horizontal(|ui| {
                        let [r, g, b, _] = color.to_srgba().to_f32_array();
                        let egui_color = egui::Color32::from_rgb(
                            (r * 255.0) as u8,
                            (g * 255.0) as u8,
                            (b * 255.0) as u8,
                        );
                        ui.colored_label(egui_color, "■");
                        ui.label(format!("{}/{} {} — {}", placed, count, model_name, base_shape_label));
                    });

                    if placed < count {
                        if ui.button(format!("Add {} to Board", model_name)).clicked() {
                            ev_spawn.send(SpawnUnit {
                                unit_name: unit_name.clone(),
                                model_name: model_name.clone(),
                                base_shape,
                                count: count - placed,
                                color,
                                movement_inches,
                                player,
                            });
                            units[i].placed = count;
                        }
                    } else if ui.button(format!("Remove {} from Board", model_name)).clicked() {
                        ev_remove.send(RemoveModelUnits {
                            unit_name: unit_name.clone(),
                            model_name: model_name.clone(),
                            player,
                        });
                        units[i].placed = 0;
                    }

                    ui.separator();
                }
            });
        if ui.button("Clear List").clicked() {
            ev_clear_player.send(ClearPlayerUnits { player });
            units.clear();
            list_text.clear();
            *submitted = false;
        }
    }
}

fn draw_army_tab(
    ui: &mut egui::Ui,
    ui_state: &mut UiState,
    ev_spawn: &mut EventWriter<SpawnUnit>,
    ev_clear_player: &mut EventWriter<ClearPlayerUnits>,
    ev_remove: &mut EventWriter<RemoveModelUnits>,
) {
    draw_player_section(
        ui,
        "ATTACKER",
        egui::Color32::from_rgb(217, 38, 38),
        &mut ui_state.attacker_list_text,
        &mut ui_state.attacker_units,
        &mut ui_state.attacker_submitted,
        Player::Attacker,
        ev_spawn,
        ev_clear_player,
        ev_remove,
    );

    ui.separator();

    draw_player_section(
        ui,
        "DEFENDER",
        egui::Color32::from_rgb(38, 89, 217),
        &mut ui_state.defender_list_text,
        &mut ui_state.defender_units,
        &mut ui_state.defender_submitted,
        Player::Defender,
        ev_spawn,
        ev_clear_player,
        ev_remove,
    );
}

// ── Right panel ──────────────────────────────────────────────────────────────

fn draw_right_panel(
    mut contexts: EguiContexts,
    mut timeline: ResMut<GameTimeline>,
    mut right_panel_width: ResMut<RightPanelWidth>,
    mut events: TimelineEvents,
    phase_state: Res<PhaseState>,
    tool_state: Res<State<ActiveTool>>,
    mut next_tool: ResMut<NextState<ActiveTool>>,
    units: Query<(Entity, &UnitBase, &Transform)>,
    base_db: Option<Res<BaseDatabase>>,
    mut tools: ToolStates,
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<ColorMaterial>>,
) {
    let ctx = contexts.ctx_mut();

    let panel = egui::SidePanel::right("timeline_panel")
        .min_width(200.0)
        .max_width(300.0)
        .show(ctx, |ui| {
            ui.heading("Timeline");
            ui.separator();

            if !timeline.locked {
                ui.label("First player:");
                ui.radio_value(
                    &mut timeline.first_player,
                    crate::types::timeline::FirstPlayer::Attacker,
                    "Attacker",
                );
                ui.radio_value(
                    &mut timeline.first_player,
                    crate::types::timeline::FirstPlayer::Defender,
                    "Defender",
                );
                ui.add_space(8.0);
                if ui.button("Lock In Deployment").clicked() {
                    events.lock.send(LockDeployment);
                }
                return;
            }

            // Snapshot history.
            let snap_count = timeline.snapshots.len();
            let current = timeline.current_index;
            let in_live = current >= snap_count;

            ui.label("History:");
            egui::ScrollArea::vertical()
                .id_salt("timeline_scroll")
                .max_height(200.0)
                .show(ui, |ui| {
                    for (idx, snapshot) in timeline.snapshots.iter().enumerate() {
                        let selected = current == idx;
                        if ui.selectable_label(selected, &snapshot.label).clicked() {
                            events.rewind.send(RewindToSnapshot(idx));
                        }
                    }
                    let live_selected = in_live;
                    if ui.selectable_label(live_selected, "▶ Live").clicked() {
                        events.rewind.send(RewindToSnapshot(snap_count));
                    }
                });

            if !in_live {
                return;
            }

            ui.separator();

            // Phase header.
            let turn = phase_state.turn_number.max(1);
            ui.strong(format!("Turn {} — {}", turn, phase_state.active_player.label()));
            ui.strong(format!("═══ {} ═══", phase_state.phase.label()));
            ui.separator();

            // Tool palette.
            let active_tool = *tool_state.get();
            let available = phase_state.phase.available_tools();
            ui.horizontal_wrapped(|ui| {
                for &tool in available {
                    let selected = active_tool == tool;
                    if ui.selectable_label(selected, tool.label()).clicked() && !selected {
                        next_tool.set(tool);
                    }
                }
            });
            ui.separator();

            // Per-tool panel content.
            match active_tool {
                ActiveTool::Select => {
                    draw_select_panel(ui, &units, &timeline, &phase_state);
                }
                ActiveTool::Move | ActiveTool::Advance | ActiveTool::FallBack
                | ActiveTool::Reactive | ActiveTool::PileIn | ActiveTool::Consolidate => {
                    ui.checkbox(&mut tools.enforce_max.0, "Enforce max movement");
                    draw_movement_panel(ui, &units, &timeline, &phase_state, active_tool);
                }
                ActiveTool::Kill => {
                    draw_kill_panel(ui, &units, &mut tools.kill, &mut events.confirm_kill);
                }
                ActiveTool::ShootAnnotate => {
                    draw_shoot_panel(
                        ui, &units, &mut tools.shoot, &phase_state, base_db.as_deref(),
                        &mut events.confirm_kill, &mut events.confirm_action,
                        &mut commands, &mut meshes, &mut materials,
                    );
                }
                ActiveTool::Charge => {
                    draw_charge_panel(ui, &units, &mut tools.charge);
                }
                ActiveTool::Battleshock => {
                    draw_battleshock_panel(ui, &units, &mut tools.battleshock);
                }
                ActiveTool::RangeRing => {
                    draw_rangering_panel(
                        ui, &units, &mut tools.range_ring,
                        &mut commands, &mut meshes, &mut materials,
                    );
                }
                ActiveTool::PerformAction => {
                    draw_action_panel(ui, &units, &phase_state, &mut events.confirm_action);
                }
                ActiveTool::Measure => {
                    ui.label("Click two points on the board to measure distance.");
                }
                ActiveTool::DeployReserves => {
                    ui.label("Select a reserves unit from the list, then click the board to place it.");
                }
                ActiveTool::EnterReserves => {
                    ui.label("Click a unit on the board to send it back to reserves.");
                }
            }

            ui.add_space(8.0);
            let end_label = if phase_state.phase == GamePhase::Fight {
                "End Turn →"
            } else {
                "End Phase →"
            };
            if ui.button(end_label).clicked() {
                events.advance.send(AdvancePhase);
            }
        });

    right_panel_width.0 = panel.response.rect.width();
}

// ── Per-tool panel content ───────────────────────────────────────────────────

fn draw_select_panel(
    ui: &mut egui::Ui,
    units: &Query<(Entity, &UnitBase, &Transform)>,
    timeline: &GameTimeline,
    phase_state: &PhaseState,
) {
    ui.label("Units:");
    egui::ScrollArea::vertical()
        .id_salt("select_scroll")
        .max_height(200.0)
        .show(ui, |ui| {
            for (entity, unit, transform) in units.iter() {
                if unit.is_killed {
                    ui.weak(format!("✗ {}", unit.model_name));
                    continue;
                }
                let dist = timeline.live_cumulative_distance.get(&entity).copied().unwrap_or(0.0);
                let label = if dist > 0.05 {
                    format!("{} [{:.1}\"]", unit.model_name, dist)
                } else {
                    unit.model_name.clone()
                };
                let side = if unit.player == phase_state.active_player { "" } else { " (enemy)" };
                ui.label(format!("{}{}", label, side));
            }
        });
}

fn draw_movement_panel(
    ui: &mut egui::Ui,
    units: &Query<(Entity, &UnitBase, &Transform)>,
    timeline: &GameTimeline,
    phase_state: &PhaseState,
    active_tool: ActiveTool,
) {
    ui.label(format!("{} — drag units to move.", active_tool.label()));
    ui.add_space(4.0);
    ui.label("Units:");
    egui::ScrollArea::vertical()
        .id_salt("move_scroll")
        .max_height(160.0)
        .show(ui, |ui| {
            for (entity, unit, transform) in units.iter() {
                if unit.player != phase_state.active_player || unit.is_killed {
                    continue;
                }
                let dist = timeline.live_cumulative_distance.get(&entity).copied().unwrap_or(0.0);
                let moved = dist > 0.05;
                let adv = unit.movement_inches.map(|m| dist > m + 0.01).unwrap_or(false);

                let label = if adv {
                    format!("{} [ADV {:.1}\"]", unit.model_name, dist)
                } else if moved {
                    format!("{} [{:.1}\"]", unit.model_name, dist)
                } else {
                    unit.model_name.clone()
                };
                ui.label(label);
            }
        });
}

fn draw_kill_panel(
    ui: &mut egui::Ui,
    units: &Query<(Entity, &UnitBase, &Transform)>,
    kill_state: &mut KillToolState,
    ev_confirm_kill: &mut EventWriter<ConfirmKill>,
) {
    ui.label("Click any unit to mark for killing.");

    if let Some(target_entity) = kill_state.pending_target {
        if let Ok((_, target_unit, _)) = units.get(target_entity) {
            ui.separator();
            ui.label(format!("Kill: {}?", target_unit.model_name));
            ui.horizontal(|ui| {
                if ui.button("Confirm Kill").clicked() {
                    ev_confirm_kill.send(ConfirmKill(target_entity));
                    kill_state.pending_target = None;
                }
                if ui.button("Cancel").clicked() {
                    kill_state.pending_target = None;
                }
            });
        } else {
            kill_state.pending_target = None;
        }
    } else {
        egui::ScrollArea::vertical()
            .id_salt("kill_scroll")
            .max_height(120.0)
            .show(ui, |ui| {
                for (_, unit, _) in units.iter() {
                    if unit.is_killed {
                        ui.weak(format!("✗ {}", unit.model_name));
                    } else {
                        ui.label(&unit.model_name);
                    }
                }
            });
    }
}

fn draw_shoot_panel(
    ui: &mut egui::Ui,
    units: &Query<(Entity, &UnitBase, &Transform)>,
    shoot_state: &mut ShootToolState,
    phase_state: &PhaseState,
    base_db: Option<&BaseDatabase>,
    ev_confirm_kill: &mut EventWriter<ConfirmKill>,
    ev_confirm_action: &mut EventWriter<ConfirmAction>,
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<ColorMaterial>,
) {
    // Friendly unit list.
    ui.label("Select shooter:");
    egui::ScrollArea::vertical()
        .id_salt("shoot_units")
        .max_height(120.0)
        .show(ui, |ui| {
            for (entity, unit, _) in units.iter() {
                if unit.player != phase_state.active_player || unit.is_killed {
                    continue;
                }
                let selected = shoot_state.selected_shooter == Some(entity);
                if ui.selectable_label(selected, &unit.model_name).clicked() {
                    shoot_state.selected_shooter = Some(entity);
                    shoot_state.selected_weapon_idx = None;
                    shoot_state.pending_target = None;
                }
            }
        });

    // Weapon selection + target.
    if let Some(shooter_entity) = shoot_state.selected_shooter {
        if let Ok((_, shooter_unit, shooter_transform)) = units.get(shooter_entity) {
            ui.separator();
            ui.label(format!("Shooter: {}", shooter_unit.model_name));

            if let Some(db) = base_db {
                let weapons: Vec<_> = db.weapons_for_unit(&shooter_unit.unit_name)
                    .iter()
                    .filter(|w| w.range.trim() != "Melee")
                    .collect();

                if weapons.is_empty() {
                    ui.label("(no ranged weapons)");
                } else {
                    ui.label("Weapons:");
                    for (i, w) in weapons.iter().enumerate() {
                        let range_str = BD::weapon_range_inches(w)
                            .map(|r| format!("{}\"", r as u32))
                            .unwrap_or_else(|| w.range.clone());
                        let label = format!("{} ({})", w.name, range_str);
                        let selected = shoot_state.selected_weapon_idx == Some(i);
                        if ui.selectable_label(selected, label).clicked() {
                            shoot_state.selected_weapon_idx = Some(i);
                            shoot_state.pending_target = None;

                            // Spawn blue range ring.
                            if let Some(range) = BD::weapon_range_inches(w) {
                                let shooter_r = shooter_unit.base_shape.radius_x_inches()
                                    .max(shooter_unit.base_shape.radius_y_inches());
                                let ring_r = range + shooter_r;
                                let pos = shooter_transform.translation.truncate();
                                commands.spawn((
                                    Mesh2d(meshes.add(Annulus::new(ring_r, ring_r + 0.12))),
                                    MeshMaterial2d(materials.add(ColorMaterial::from_color(
                                        Color::srgba(0.2, 0.4, 1.0, 0.85),
                                    ))),
                                    Transform::from_xyz(pos.x, pos.y, 0.5),
                                    Visibility::Visible,
                                    ShooterRangeRing,
                                    PickingBehavior::IGNORE,
                                ));
                            }
                        }
                    }
                }

                // Pending target.
                if let Some(target_entity) = shoot_state.pending_target {
                    if let Ok((_, target_unit, target_transform)) = units.get(target_entity) {
                        let shooter_r = shooter_unit.base_shape.radius_x_inches()
                            .max(shooter_unit.base_shape.radius_y_inches());
                        let target_r = target_unit.base_shape.radius_x_inches()
                            .max(target_unit.base_shape.radius_y_inches());
                        let center_dist = shooter_transform
                            .translation
                            .truncate()
                            .distance(target_transform.translation.truncate());
                        let edge_dist = (center_dist - shooter_r - target_r).max(0.0);
                        ui.separator();
                        ui.label(format!("Target: {} ({:.1}\" away)", target_unit.model_name, edge_dist));
                        ui.horizontal(|ui| {
                            if ui.button("Kill it").clicked() {
                                ev_confirm_kill.send(ConfirmKill(target_entity));
                                shoot_state.pending_target = None;
                            }
                            if ui.button("Cancel").clicked() {
                                shoot_state.pending_target = None;
                            }
                        });
                    }
                } else if shoot_state.selected_weapon_idx.is_some() {
                    ui.label("→ Click an enemy on the board");
                }
            } else {
                ui.label("(weapon database not loaded)");
            }

            // Mark as performing action.
            ui.add_space(4.0);
            if ui.button("Mark performing action").clicked() {
                if let Some(e) = shoot_state.selected_shooter {
                    ev_confirm_action.send(ConfirmAction(e));
                }
                shoot_state.selected_shooter = None;
                shoot_state.selected_weapon_idx = None;
                shoot_state.pending_target = None;
            }
        }
    }
}

fn draw_charge_panel(
    ui: &mut egui::Ui,
    units: &Query<(Entity, &UnitBase, &Transform)>,
    charge_state: &mut ChargeToolState,
) {
    ui.label("Select charger, then click enemies to declare targets.");

    if let Some(charger_entity) = charge_state.declared_charger {
        if let Ok((_, charger_unit, charger_transform)) = units.get(charger_entity) {
            ui.separator();
            ui.label(format!("Charger: {}", charger_unit.model_name));

            // Show targets with distances.
            if !charge_state.charge_targets.is_empty() {
                ui.label("Targets:");
                for &target_entity in &charge_state.charge_targets {
                    if let Ok((_, target_unit, target_transform)) = units.get(target_entity) {
                        let charger_r = charger_unit.base_shape.radius_x_inches()
                            .max(charger_unit.base_shape.radius_y_inches());
                        let target_r = target_unit.base_shape.radius_x_inches()
                            .max(target_unit.base_shape.radius_y_inches());
                        let center_dist = charger_transform
                            .translation
                            .truncate()
                            .distance(target_transform.translation.truncate());
                        let edge_dist = (center_dist - charger_r - target_r).max(0.0);
                        let in_range = edge_dist <= 12.0;
                        ui.label(format!(
                            "  {} ({:.1}\" — {})",
                            target_unit.model_name,
                            edge_dist,
                            if in_range { "in range" } else { "out of range" }
                        ));
                    }
                }

                ui.add_space(4.0);
                match charge_state.charge_declared {
                    None => {
                        ui.horizontal(|ui| {
                            if ui.button("Declare Success").clicked() {
                                charge_state.charge_declared = Some(true);
                            }
                            if ui.button("Declare Failure").clicked() {
                                charge_state.charge_declared = Some(false);
                            }
                        });
                    }
                    Some(true) => {
                        ui.label("Charge SUCCESS — drag charger into position");
                    }
                    Some(false) => {
                        ui.label("Charge failed");
                    }
                }
            } else {
                ui.label("→ Click enemy units on the board");
            }
        }
    }
}

fn draw_battleshock_panel(
    ui: &mut egui::Ui,
    units: &Query<(Entity, &UnitBase, &Transform)>,
    bs_state: &mut BattleshockToolState,
) {
    ui.label("Click a unit to mark as battleshocked.");

    if let Some(target_entity) = bs_state.pending_target {
        if let Ok((_, target_unit, _)) = units.get(target_entity) {
            ui.separator();
            ui.label(format!("Battleshock: {}?", target_unit.model_name));
            ui.horizontal(|ui| {
                if ui.button("Confirm").clicked() {
                    // The battleshock flag is set directly since we have mutable access
                    // through the event system — but we need a dedicated event or direct query.
                    // For now, mark via pending and let a system handle it.
                    bs_state.pending_target = None;
                }
                if ui.button("Cancel").clicked() {
                    bs_state.pending_target = None;
                }
            });
        } else {
            bs_state.pending_target = None;
        }
    }
}

fn draw_rangering_panel(
    ui: &mut egui::Ui,
    units: &Query<(Entity, &UnitBase, &Transform)>,
    rr_state: &mut RangeRingToolState,
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<ColorMaterial>,
) {
    ui.label("Click a unit, enter radius, add ring.");

    if let Some(unit_entity) = rr_state.selected_unit {
        if let Ok((_, unit_base, unit_transform)) = units.get(unit_entity) {
            ui.separator();
            ui.label(format!("Unit: {}", unit_base.model_name));
            ui.horizontal(|ui| {
                ui.label("Radius:");
                ui.text_edit_singleline(&mut rr_state.radius_input);
                ui.label("\"");
            });
            if ui.button("Add Ring").clicked() {
                if let Ok(radius) = rr_state.radius_input.parse::<f32>() {
                    if radius > 0.0 {
                        let unit_r = unit_base.base_shape.radius_x_inches()
                            .max(unit_base.base_shape.radius_y_inches());
                        let ring_r = radius + unit_r;
                        let pos = unit_transform.translation.truncate();
                        commands.spawn((
                            Mesh2d(meshes.add(Annulus::new(ring_r, ring_r + 0.12))),
                            MeshMaterial2d(materials.add(ColorMaterial::from_color(
                                Color::srgba(0.8, 0.8, 0.8, 0.6),
                            ))),
                            Transform::from_xyz(pos.x, pos.y, 0.4),
                            Visibility::Visible,
                            PersistentRangeRing { unit: unit_entity, radius },
                            PickingBehavior::IGNORE,
                        ));
                    }
                }
            }
        }
    }

    ui.add_space(4.0);
    if ui.button("Clear All Rings").clicked() {
        // This is handled by querying PersistentRangeRing entities — but we don't have
        // access to the query here, so we'll use commands to despawn via marker.
        // For now, leave this as a TODO or add a dedicated event.
    }
}

fn draw_action_panel(
    ui: &mut egui::Ui,
    units: &Query<(Entity, &UnitBase, &Transform)>,
    phase_state: &PhaseState,
    ev_confirm_action: &mut EventWriter<ConfirmAction>,
) {
    ui.label("Click a friendly unit to mark as performing action.");
    egui::ScrollArea::vertical()
        .id_salt("action_scroll")
        .max_height(160.0)
        .show(ui, |ui| {
            for (entity, unit, _) in units.iter() {
                if unit.player != phase_state.active_player || unit.is_killed {
                    continue;
                }
                let label = if unit.is_performing_action {
                    format!("{} (action)", unit.model_name)
                } else {
                    unit.model_name.clone()
                };
                if ui.selectable_label(unit.is_performing_action, label).clicked() {
                    if !unit.is_performing_action {
                        ev_confirm_action.send(ConfirmAction(entity));
                    }
                }
            }
        });
}

fn draw_analysis_tab(
    ui: &mut egui::Ui,
    ui_state: &mut UiState,
    vis_state: &VisibilityState,
    ev_trigger: &mut EventWriter<TriggerAnalysis>,
    ev_clear: &mut EventWriter<ClearAnalysis>,
) {
    ui.label("Analysis Mode:");
    ui.horizontal(|ui| {
        if ui
            .selectable_label(
                ui_state.selected_analysis_mode == AnalysisMode::ZoneCoverage,
                "Zone Coverage",
            )
            .clicked()
        {
            ui_state.selected_analysis_mode = AnalysisMode::ZoneCoverage;
        }
        if ui
            .selectable_label(
                ui_state.selected_analysis_mode == AnalysisMode::UnitPositions,
                "Unit Positions",
            )
            .clicked()
        {
            ui_state.selected_analysis_mode = AnalysisMode::UnitPositions;
        }
    });

    if ui_state.selected_analysis_mode == AnalysisMode::UnitPositions {
        ui.add_space(4.0);
        ui.label("Movement override (inches):");
        ui.add(egui::Slider::new(&mut ui_state.movement_override, 0.0..=24.0).text("\""));
    }

    ui.add_space(8.0);

    let button_text = if vis_state.analyzing {
        "Running..."
    } else {
        "Run Analysis"
    };

    let btn = ui.add_enabled(!vis_state.analyzing, egui::Button::new(button_text));
    if btn.clicked() {
        ev_trigger.send(TriggerAnalysis(ui_state.selected_analysis_mode));
    }

    if let Some(area) = vis_state
        .danger_region
        .as_ref()
        .map(|_| vis_state.danger_area_sq_in)
    {
        ui.add_space(8.0);
        ui.label(format!("Danger area: {:.1} sq\"", area));
        let pct = area / 2640.0 * 100.0;
        ui.label(format!("Coverage: {:.1}% of board", pct));

        ui.add_space(4.0);
        if ui.button("Clear Analysis").clicked() {
            ev_clear.send(ClearAnalysis);
        }
    }
}
