use bevy::prelude::*;
use bevy_egui::{egui, EguiContexts};

use crate::army_list::base_lookup::BaseDatabase;
use crate::army_list::parse_listforge;
use crate::events::{
    DeleteUnit, LoadDeploymentPattern, LoadTerrainLayout, SpawnUnit,
    TriggerAnalysis,
};
use crate::resources::{ActiveLayout, ActivePattern, DeploymentPatterns, PanelWidth, TerrainLayouts};
use crate::types::units::{ArmyUnit, Player};
use crate::types::visibility::{AnalysisMode, VisibilityState};

pub struct UiPlugin;

impl Plugin for UiPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<UiState>()
            .init_resource::<PanelWidth>()
            .add_systems(Update, draw_ui_panel);
    }
}

#[derive(Resource)]
struct UiState {
    active_tab: UiTab,
    /// Raw text from the army list paste box.
    army_list_text: String,
    /// Parsed units ready to display.
    army_units: Vec<ArmyUnit>,
    movement_override: f32,
    selected_player: SelectedPlayer,
    selected_analysis_mode: AnalysisMode,
}

impl Default for UiState {
    fn default() -> Self {
        Self {
            active_tab: UiTab::default(),
            army_list_text: String::new(),
            army_units: Vec::new(),
            movement_override: 0.0,
            selected_player: SelectedPlayer::default(),
            selected_analysis_mode: AnalysisMode::ZoneCoverage,
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

#[derive(Default, PartialEq, Eq, Clone, Copy)]
enum SelectedPlayer {
    #[default]
    Attacker,
    Defender,
}

impl SelectedPlayer {
    fn to_player(&self) -> Player {
        match self {
            SelectedPlayer::Attacker => Player::Attacker,
            SelectedPlayer::Defender => Player::Defender,
        }
    }
}

const ATTACKER_COLOR: Color = Color::srgb(0.85, 0.15, 0.15);
const DEFENDER_COLOR: Color = Color::srgb(0.15, 0.35, 0.85);

fn draw_ui_panel(
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
    mut ev_spawn: EventWriter<SpawnUnit>,
    mut ev_delete: EventWriter<DeleteUnit>,
    mut panel_width: ResMut<PanelWidth>,
) {
    let ctx = contexts.ctx_mut();

    let panel = egui::SidePanel::left("control_panel")
        .min_width(240.0)
        .max_width(300.0)
        .show(ctx, |ui| {
            ui.heading("Deployment Helper");
            ui.separator();

            // Tab bar.
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
                ),
                UiTab::Army => draw_army_tab(ui, &mut ui_state, &mut ev_spawn),
                UiTab::Analysis => draw_analysis_tab(
                    ui,
                    &mut ui_state,
                    &vis_state,
                    &mut ev_trigger,
                ),
            }
        });
    panel_width.0 = panel.response.rect.width();
}

fn draw_setup_tab(
    ui: &mut egui::Ui,
    ui_state: &mut UiState,
    layouts: &TerrainLayouts,
    patterns: &DeploymentPatterns,
    active_layout: &mut ActiveLayout,
    active_pattern: &mut ActivePattern,
    ev_load_layout: &mut EventWriter<LoadTerrainLayout>,
    ev_load_pattern: &mut EventWriter<LoadDeploymentPattern>,
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
}

fn draw_army_tab(
    ui: &mut egui::Ui,
    ui_state: &mut UiState,
    ev_spawn: &mut EventWriter<SpawnUnit>,
) {
    ui.label("Player side:");
    ui.horizontal(|ui| {
        ui.selectable_value(&mut ui_state.selected_player, SelectedPlayer::Attacker, "Attacker");
        ui.selectable_value(&mut ui_state.selected_player, SelectedPlayer::Defender, "Defender");
    });

    ui.add_space(4.0);
    ui.label("Paste Listforge list:");
    ui.add(
        egui::TextEdit::multiline(&mut ui_state.army_list_text)
            .desired_rows(8)
            .desired_width(f32::INFINITY),
    );

    if ui.button("Import List").clicked() {
        let parsed = parse_listforge(&ui_state.army_list_text);
        let base_db = BaseDatabase::load(
            include_str!("../../assets/Datasheets.json"),
            include_str!("../../assets/Datasheets_models.json"),
        );

        let player = ui_state.selected_player.to_player();
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

            // If no bullet lines matched real model variants, treat the unit as a single model.
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
                    base_shape,
                    movement_inches: movement,
                    color,
                    player,
                });
            }
        }
        ui_state.army_units = army_units;
    }

    ui.separator();

    // Unit roster.
    if ui_state.army_units.is_empty() {
        ui.label("No units imported.");
        return;
    }

    egui::ScrollArea::vertical().show(ui, |ui| {
        let units = ui_state.army_units.clone();
        for unit in &units {
            ui.horizontal(|ui| {
                let [r, g, b, _] = unit.color.to_srgba().to_f32_array();
                let egui_color = egui::Color32::from_rgb(
                    (r * 255.0) as u8,
                    (g * 255.0) as u8,
                    (b * 255.0) as u8,
                );
                ui.colored_label(egui_color, "■");
                ui.label(format!(
                    "{}x {} — {}",
                    unit.count,
                    unit.model_name,
                    unit.base_shape.label()
                ));
            });
            if ui.button(format!("Add {} to Board", unit.model_name)).clicked() {
                ev_spawn.send(SpawnUnit {
                    unit_name: unit.unit_name.clone(),
                    model_name: unit.model_name.clone(),
                    base_shape: unit.base_shape.clone(),
                    count: unit.count,
                    color: unit.color,
                    movement_inches: unit.movement_inches,
                    player: unit.player,
                });
            }
            ui.separator();
        }
    });
}

fn draw_analysis_tab(
    ui: &mut egui::Ui,
    ui_state: &mut UiState,
    vis_state: &VisibilityState,
    ev_trigger: &mut EventWriter<TriggerAnalysis>,
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
        // Show percentage of 60×44 board = 2640 sq".
        let pct = area / 2640.0 * 100.0;
        ui.label(format!("Coverage: {:.1}% of board", pct));
    }
}
