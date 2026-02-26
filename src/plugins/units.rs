use bevy::prelude::*;

use crate::events::SpawnUnit;
use crate::resources::{ActivePattern, BoardConfig, DeploymentPatterns, TerrainLayouts, ActiveLayout};
use crate::types::terrain::TerrainPiece;
use crate::types::units::{BaseShape, Player, UnitBase};
use crate::los::shapes::{extract_obstacle_edges, point_in_shape};

pub struct UnitsPlugin;

impl Plugin for UnitsPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            (on_spawn_unit, handle_drag),
        );
    }
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
    let obstacle_edges = extract_obstacle_edges(pieces, &Default::default());

    // Scan left-to-right, top-to-bottom at 1" spacing.
    let mut y = min_y + ry;
    let mut candidate_idx: u32 = 0;
    while y <= max_y - ry {
        let mut x = min_x + rx;
        while x <= max_x - rx {
            let pos = Vec2::new(x, y);
            if point_in_polygon_optional(pos, zone_verts)
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

fn point_in_polygon_optional(p: Vec2, verts: Option<&[Vec2]>) -> bool {
    let verts = match verts {
        Some(v) => v,
        None => return true,
    };
    crate::types::deployment::point_in_polygon_pub(p, verts)
}

fn overlaps_any_terrain(pos: Vec2, base: &BaseShape, pieces: &[TerrainPiece]) -> bool {
    let rx = base.radius_x_inches();
    let ry = base.radius_y_inches();

    for piece in pieces {
        if !piece.blocking {
            continue;
        }
        for shape in &piece.shapes {
            // Check if base center + radius overlaps any shape using AABB approximation.
            // For accuracy, check multiple points on the base perimeter.
            let check_pts = [
                pos,
                pos + Vec2::new(rx, 0.0),
                pos - Vec2::new(rx, 0.0),
                pos + Vec2::new(0.0, ry),
                pos - Vec2::new(0.0, ry),
            ];
            for &pt in &check_pts {
                if point_in_shape(pt, shape, piece) {
                    return true;
                }
            }
        }
    }
    false
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

    commands.spawn((
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
    ));
}

/// Drag handling via Bevy Picking pointer events.
fn handle_drag(
    mut bases: Query<(&mut Transform, &mut UnitBase)>,
    mut drag_events: EventReader<Pointer<Drag>>,
    mut drag_end_events: EventReader<Pointer<DragEnd>>,
    board: Res<BoardConfig>,
    layouts: Res<TerrainLayouts>,
    active_layout: Res<ActiveLayout>,
    camera_q: Query<(&Camera, &GlobalTransform)>,
) {
    let terrain_pieces: Vec<TerrainPiece> = active_layout
        .0
        .as_ref()
        .and_then(|id| layouts.0.iter().find(|l| &l.id == id))
        .map(|l| l.pieces.clone())
        .unwrap_or_default();

    // Get camera scale to convert pixel delta to world delta.
    let world_per_pixel = camera_q
        .get_single()
        .map(|(cam, cam_gt)| {
            // Use camera viewport rect to compute scale.
            cam.physical_viewport_size()
                .map(|vp| {
                    // The AutoMin projection ensures min_width inches fits in the viewport.
                    // We can use the camera's logical viewport size.
                    vp
                })
                .is_some()
        })
        .ok();

    for ev in drag_events.read() {
        let Ok((mut transform, mut unit_base)) = bases.get_mut(ev.target) else {
            continue;
        };
        if unit_base.locked {
            continue;
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
        let Ok((mut transform, mut unit_base)) = bases.get_mut(ev.target) else {
            continue;
        };
        if unit_base.locked {
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

        // Check terrain overlap.
        if overlaps_any_terrain(clamped, &unit_base.base_shape, &terrain_pieces) {
            // Snap back to last valid position.
            transform.translation.x = unit_base.last_valid_pos.x;
            transform.translation.y = unit_base.last_valid_pos.y;
        } else {
            transform.translation.x = clamped.x;
            transform.translation.y = clamped.y;
            unit_base.last_valid_pos = clamped;
        }
    }
}
