use bevy::prelude::*;
use bevy::render::camera::ScalingMode;

use crate::resources::BoardConfig;

pub struct BoardPlugin;

impl Plugin for BoardPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, setup_board);
    }
}

fn setup_board(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<ColorMaterial>>,
    board: Res<BoardConfig>,
) {
    // Camera centered at board center. AutoMin ensures both dimensions are visible.
    // Adding a small margin (4" horizontal, 3" vertical) so the board isn't edge-to-edge.
    commands.spawn((
        Camera2d,
        Transform::from_xyz(board.width / 2.0, board.height / 2.0, 999.0),
        OrthographicProjection {
            scaling_mode: ScalingMode::AutoMin {
                min_width: board.width + 4.0,
                min_height: board.height + 3.0,
            },
            ..OrthographicProjection::default_2d()
        },
    ));

    // Board background.
    commands.spawn((
        Mesh2d(meshes.add(Rectangle::new(board.width, board.height))),
        MeshMaterial2d(materials.add(ColorMaterial::from_color(Color::srgb(0.86, 0.82, 0.72)))),
        Transform::from_xyz(board.width / 2.0, board.height / 2.0, 0.0),
        BoardBackground,
    ));

    // Grid lines.
    spawn_grid(&mut commands, &mut meshes, &mut materials, &board);
}

#[derive(Component)]
pub struct BoardBackground;

fn spawn_grid(
    commands: &mut Commands,
    meshes: &mut ResMut<Assets<Mesh>>,
    materials: &mut ResMut<Assets<ColorMaterial>>,
    board: &BoardConfig,
) {
    let thin_color = Color::srgba(0.0, 0.0, 0.0, 0.15);
    let thick_color = Color::srgba(0.0, 0.0, 0.0, 0.35);
    let thin = 0.02;
    let thick = 0.04;

    // Vertical lines.
    let mut x = 1.0_f32;
    while x < board.width {
        let is_5 = (x % 5.0).abs() < 0.01;
        let w = if is_5 { thick } else { thin };
        let color = if is_5 { thick_color } else { thin_color };
        commands.spawn((
            Mesh2d(meshes.add(Rectangle::new(w, board.height))),
            MeshMaterial2d(materials.add(ColorMaterial::from_color(color))),
            Transform::from_xyz(x, board.height / 2.0, 0.1),
        ));
        x += 1.0;
    }

    // Horizontal lines.
    let mut y = 1.0_f32;
    while y < board.height {
        let is_5 = (y % 5.0).abs() < 0.01;
        let h = if is_5 { thick } else { thin };
        let color = if is_5 { thick_color } else { thin_color };
        commands.spawn((
            Mesh2d(meshes.add(Rectangle::new(board.width, h))),
            MeshMaterial2d(materials.add(ColorMaterial::from_color(color))),
            Transform::from_xyz(board.width / 2.0, y, 0.1),
        ));
        y += 1.0;
    }
}
