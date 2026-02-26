use bevy::prelude::*;

use crate::events::LoadDeploymentPattern;
use crate::resources::DeploymentPatterns;
use crate::types::deployment::DeploymentZoneMarker;

pub struct DeploymentPlugin;

impl Plugin for DeploymentPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, on_load_deployment_pattern);
    }
}

fn on_load_deployment_pattern(
    mut commands: Commands,
    mut events: EventReader<LoadDeploymentPattern>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<ColorMaterial>>,
    existing: Query<Entity, With<DeploymentZoneMarker>>,
    patterns: Res<DeploymentPatterns>,
) {
    for ev in events.read() {
        // Despawn existing zone overlays.
        for e in existing.iter() {
            commands.entity(e).despawn_recursive();
        }

        let pattern = match patterns.0.iter().find(|p| p.id == ev.0) {
            Some(p) => p,
            None => {
                warn!("Deployment pattern not found: {}", ev.0);
                continue;
            }
        };

        for zone in &pattern.zones {
            let player = zone.to_player();
            let color = parse_hex_color(&zone.color, 0.25);
            let verts = zone.world_vertices();
            let mesh = polygon_mesh(&verts);

            commands.spawn((
                Mesh2d(meshes.add(mesh)),
                MeshMaterial2d(materials.add(ColorMaterial::from_color(color))),
                Transform::from_xyz(0.0, 0.0, 1.0),
                DeploymentZoneMarker { player },
            ));
        }
    }
}

/// Parse a "#rrggbb" hex color with given alpha.
fn parse_hex_color(hex: &str, alpha: f32) -> Color {
    let hex = hex.trim_start_matches('#');
    if hex.len() == 6 {
        if let (Ok(r), Ok(g), Ok(b)) = (
            u8::from_str_radix(&hex[0..2], 16),
            u8::from_str_radix(&hex[2..4], 16),
            u8::from_str_radix(&hex[4..6], 16),
        ) {
            return Color::srgba(r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0, alpha);
        }
    }
    Color::srgba(0.5, 0.5, 1.0, alpha)
}

fn polygon_mesh(verts: &[Vec2]) -> Mesh {
    use bevy::render::mesh::{Indices, PrimitiveTopology};
    use bevy::render::render_asset::RenderAssetUsages;

    let positions: Vec<[f32; 3]> = verts.iter().map(|v| [v.x, v.y, 0.0]).collect();
    let normals = vec![[0.0f32, 0.0, 1.0]; verts.len()];
    let uvs: Vec<[f32; 2]> = verts.iter().map(|v| [v.x, v.y]).collect();

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
