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

        for (i, piece) in layout.pieces.iter().enumerate() {
            spawn_terrain_piece(&mut commands, &mut meshes, &mut materials, piece, i + 1);
        }
    }
}

fn spawn_terrain_piece(
    commands: &mut Commands,
    meshes: &mut ResMut<Assets<Mesh>>,
    materials: &mut ResMut<Assets<ColorMaterial>>,
    piece: &TerrainPiece,
    piece_index: usize,
) {
    let pos = piece.world_position();

    // Parent entity at piece position + rotation (JSON CW → negate for Bevy y-up CCW).
    let parent = commands
        .spawn((
            Transform::from_xyz(pos.x, pos.y, 2.0)
                .with_rotation(Quat::from_rotation_z(-piece.rotation.to_radians()))
                .with_scale(match piece.mirror {
                    crate::types::terrain::Mirror::Horizontal => Vec3::new(-1.0, 1.0, 1.0),
                    crate::types::terrain::Mirror::Vertical => Vec3::new(1.0, -1.0, 1.0),
                    crate::types::terrain::Mirror::None => Vec3::ONE,
                }),
            Visibility::Visible,
            TerrainPieceMarker {
                piece_id: piece.id.clone(),
            },
        ))
        .id();

    for (shape_idx, shape) in piece.shapes.iter().enumerate() {
        let color = shape_color(piece.blocking, shape_idx == 0);
        let child = spawn_shape_mesh(commands, meshes, materials, shape, color, shape_idx);
        commands.entity(parent).add_child(child);
    }

    // Label at the first shape's visual center.
    let label_offset = match piece.shapes.first() {
        Some(TerrainShape::Rectangle { width, height, .. }) => {
            Vec2::new(width / 2.0, -height / 2.0)
        }
        _ => Vec2::ZERO,
    };
    let label = commands
        .spawn((
            Text2d::new(piece.name.clone()),
            TextFont {
                font_size: 10.0,
                ..default()
            },
            TextColor(Color::WHITE),
            Transform::from_xyz(label_offset.x, label_offset.y, 0.5)
                .with_scale(Vec3::splat(0.08)),
        ))
        .id();
    commands.entity(parent).add_child(label);

    // Pink origin dot with debug label at local (0, 0) — marks the JSON top-left-corner anchor.
    let mirror_str = match piece.mirror {
        crate::types::terrain::Mirror::None => "M:none",
        crate::types::terrain::Mirror::Horizontal => "M:horiz",
        crate::types::terrain::Mirror::Vertical => "M:vert",
    };
    let debug_text = format!(
        "({:.1},{:.1}) {:.0}° {}",
        pos.x, pos.y, piece.rotation, mirror_str
    );

    let dot = commands
        .spawn((
            Mesh2d(meshes.add(Mesh::from(Circle::new(0.25)))),
            MeshMaterial2d(materials.add(ColorMaterial::from_color(Color::srgb(
                1.0, 0.08, 0.58,
            )))),
            Transform::from_xyz(0.0, 0.0, 3.0),
            PickingBehavior::IGNORE,
        ))
        .id();
    commands.entity(parent).add_child(dot);

    // Number label spawned at world position so it doesn't rotate with the piece.
    commands.spawn((
        Text2d::new(format!("{}\n{}", piece_index, debug_text)),
        TextFont {
            font_size: 10.0,
            ..default()
        },
        TextColor(Color::BLACK),
        Transform::from_xyz(pos.x + 0.3, pos.y, 3.5).with_scale(Vec3::splat(0.08)),
    ));
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
    shape_idx: usize,
) -> Entity {
    let (mesh, offset, angle) = shape_to_mesh(shape);
    let z = shape_idx as f32 * 0.01;
    commands
        .spawn((
            Mesh2d(meshes.add(mesh)),
            MeshMaterial2d(materials.add(ColorMaterial::from_color(color))),
            Transform::from_xyz(offset.x, offset.y, z)
                .with_rotation(Quat::from_rotation_z(angle)),
            PickingBehavior::IGNORE,
        ))
        .id()
}

/// Returns (mesh, local_offset, z_rotation_radians).
/// Local offset is in the parent's (piece's) coordinate frame.
fn shape_to_mesh(shape: &TerrainShape) -> (Mesh, Vec2, f32) {
    match shape {
        TerrainShape::Rectangle { width, height, x, y } => {
            // JSON pivot is the top-left corner of the piece. In Bevy y-up, the rectangle
            // center sits at (x + half_width, -y - half_height) relative to that corner.
            (
                Rectangle::new(*width, *height).into(),
                Vec2::new(x + width / 2.0, -y - height / 2.0),
                0.0,
            )
        }
        TerrainShape::Polygon { points } => {
            // JSON polygon vertices are y-down local coords; flip y for Bevy.
            let verts: Vec<Vec2> = points.iter().map(|p| Vec2::new(p.x, -p.y)).collect();
            (polygon_mesh_earcut(&verts), Vec2::ZERO, 0.0)
        }
        TerrainShape::Circle { radius } => (Circle::new(*radius).into(), Vec2::ZERO, 0.0),
        TerrainShape::Line {
            start,
            end,
            thickness,
        } => {
            // JSON line endpoints are y-down local coords; flip y for Bevy.
            let s = Vec2::new(start.x, -start.y);
            let e = Vec2::new(end.x, -end.y);
            let center = (s + e) / 2.0;
            let length = (e - s).length();
            let angle = (e - s).to_angle();
            (Rectangle::new(length, *thickness).into(), center, angle)
        }
    }
}

/// Build a `Mesh` from a polygon using earcut triangulation (handles concave shapes).
fn polygon_mesh_earcut(verts: &[Vec2]) -> Mesh {
    use bevy::render::mesh::{Indices, PrimitiveTopology};
    use bevy::render::render_asset::RenderAssetUsages;
    use geo::{Coord, LineString, Polygon, TriangulateEarcut};

    let exterior: Vec<Coord<f64>> = verts
        .iter()
        .map(|v| Coord { x: v.x as f64, y: v.y as f64 })
        .collect();
    let geo_poly = Polygon::new(LineString::new(exterior), vec![]);
    let triangles = geo_poly.earcut_triangles();

    let positions: Vec<[f32; 3]> = triangles
        .iter()
        .flat_map(|tri| [tri.0, tri.1, tri.2])
        .map(|c| [c.x as f32, c.y as f32, 0.0])
        .collect();
    let n = positions.len();
    let normals = vec![[0.0f32, 0.0, 1.0]; n];
    let uvs: Vec<[f32; 2]> = positions.iter().map(|p| [p[0], p[1]]).collect();
    let indices: Vec<u32> = (0..n as u32).collect();

    let mut mesh = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::RENDER_WORLD);
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
    mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, normals);
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, uvs);
    mesh.insert_indices(Indices::U32(indices));
    mesh
}
