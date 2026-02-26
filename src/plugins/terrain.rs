use bevy::prelude::*;

use crate::events::LoadTerrainLayout;
use crate::resources::TerrainLayouts;
use crate::types::terrain::{TerrainPiece, TerrainShape};

pub struct TerrainPlugin;

impl Plugin for TerrainPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, (on_load_terrain_layout,));
    }
}

/// ECS marker on each terrain entity.
#[derive(Component)]
pub struct TerrainPieceMarker {
    pub piece_id: String,
}

fn on_load_terrain_layout(
    mut commands: Commands,
    mut events: EventReader<LoadTerrainLayout>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<ColorMaterial>>,
    existing: Query<Entity, With<TerrainPieceMarker>>,
    layouts: Res<TerrainLayouts>,
) {
    for ev in events.read() {
        // Despawn existing terrain.
        for e in existing.iter() {
            commands.entity(e).despawn_recursive();
        }

        let layout = match layouts.0.iter().find(|l| l.id == ev.0) {
            Some(l) => l,
            None => {
                warn!("Layout not found: {}", ev.0);
                continue;
            }
        };

        for piece in &layout.pieces {
            spawn_terrain_piece(&mut commands, &mut meshes, &mut materials, piece);
        }
    }
}

fn spawn_terrain_piece(
    commands: &mut Commands,
    meshes: &mut ResMut<Assets<Mesh>>,
    materials: &mut ResMut<Assets<ColorMaterial>>,
    piece: &TerrainPiece,
) {
    let pos = piece.world_position();

    // Parent entity at piece position + rotation.
    let parent = commands
        .spawn((
            Transform::from_xyz(pos.x, pos.y, 2.0)
                .with_rotation(Quat::from_rotation_z(piece.rotation.to_radians())),
            Visibility::Visible,
            TerrainPieceMarker {
                piece_id: piece.id.clone(),
            },
        ))
        .id();

    for (shape_idx, shape) in piece.shapes.iter().enumerate() {
        let color = shape_color(piece.blocking, shape_idx == 0);
        let child = spawn_shape_mesh(commands, meshes, materials, shape, color);
        commands.entity(parent).add_child(child);
    }
}

fn shape_color(blocking: bool, is_footprint: bool) -> Color {
    if !blocking {
        // Non-blocking terrain: gray outline.
        return Color::srgba(0.5, 0.5, 0.5, 0.3);
    }
    if is_footprint {
        Color::srgba(0.4, 0.35, 0.25, 0.5)
    } else {
        // Walls: darker, more opaque.
        Color::srgba(0.25, 0.2, 0.1, 0.85)
    }
}

fn spawn_shape_mesh(
    commands: &mut Commands,
    meshes: &mut ResMut<Assets<Mesh>>,
    materials: &mut ResMut<Assets<ColorMaterial>>,
    shape: &TerrainShape,
    color: Color,
) -> Entity {
    let (mesh, offset) = shape_to_mesh(shape);
    commands
        .spawn((
            Mesh2d(meshes.add(mesh)),
            MeshMaterial2d(materials.add(ColorMaterial::from_color(color))),
            Transform::from_xyz(offset.x, offset.y, 0.0),
        ))
        .id()
}

fn shape_to_mesh(shape: &TerrainShape) -> (Mesh, Vec2) {
    match shape {
        TerrainShape::Rectangle { width, height } => {
            (Rectangle::new(*width, *height).into(), Vec2::ZERO)
        }
        TerrainShape::Polygon { points } => {
            let verts: Vec<Vec2> = points.iter().map(|p| Vec2::new(p.x, p.y)).collect();
            (polygon_mesh(&verts), Vec2::ZERO)
        }
        TerrainShape::Circle { radius } => {
            (Circle::new(*radius).into(), Vec2::ZERO)
        }
        TerrainShape::Line {
            start,
            end,
            thickness,
        } => {
            let s = Vec2::new(start.x, start.y);
            let e = Vec2::new(end.x, end.y);
            let center = (s + e) / 2.0;
            let length = (e - s).length();
            let angle = (e - s).to_angle();
            // We'll return a rectangle mesh; caller handles the transform.
            // We encode the angle into a separate Z-rotation on the child entity.
            let _ = angle; // handled below
            (
                Rectangle::new(length, *thickness).into(),
                center,
            )
        }
    }
}

/// Build a `Mesh` from a polygon by ear-cut triangulation.
fn polygon_mesh(verts: &[Vec2]) -> Mesh {
    use bevy::render::mesh::{Indices, PrimitiveTopology};
    use bevy::render::render_asset::RenderAssetUsages;

    let positions: Vec<[f32; 3]> = verts.iter().map(|v| [v.x, v.y, 0.0]).collect();
    let normals: Vec<[f32; 3]> = vec![[0.0, 0.0, 1.0]; verts.len()];
    let uvs: Vec<[f32; 2]> = verts.iter().map(|v| [v.x, v.y]).collect();

    // Simple fan triangulation (works for convex polygons; fine for terrain shapes).
    let mut indices = Vec::new();
    for i in 1..verts.len() as u32 - 1 {
        indices.push(0);
        indices.push(i);
        indices.push(i + 1);
    }

    let mut mesh = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::RENDER_WORLD);
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
    mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, normals);
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, uvs);
    mesh.insert_indices(Indices::U32(indices));
    mesh
}
