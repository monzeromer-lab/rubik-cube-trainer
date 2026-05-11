//! Bevy plugin: drag-to-turn input.
//!
//! Mesh-picking finds which sticker the cursor is over. On left-mouse-down
//! we record the picked sticker's cubie index, its outward face, and the
//! starting cursor position. On drag past the threshold, we pick the
//! dominant in-plane axis, derive the slice face from the cubie's position
//! along the OTHER in-plane axis, then enumerate `Cw / Ccw / Half` of that
//! slice and choose whichever moves the cubie most into the drag direction.
//!
//! See plan §11.1.

use bevy::picking::pointer::PointerLocation;
use bevy::prelude::*;
use cube_core::{Face, Move, Turn};
use cube_render::{CubeState, CubiePiece, PendingMoves, Sticker};
use glam::IVec3;

/// Drag distance, in pixels, that has to be exceeded before a move
/// commits. Tuned to feel deliberate without making the cube feel laggy.
pub const DRAG_THRESHOLD_PIXELS: f32 = 14.0;

#[derive(Resource, Default, Debug)]
pub struct DragState(pub Option<DragInfo>);

#[derive(Debug, Clone, Copy)]
pub struct DragInfo {
    pub cubie_idx: usize,
    /// Unit world-space outward normal of the picked sticker at click
    /// time. We snapshot this rather than storing the sticker's home
    /// `Face`, because every move rotates the cubie and the sticker
    /// migrates to a different world face — using the home face later
    /// would project the drag onto the wrong plane and flip the
    /// move-decision sign.
    pub world_face: IVec3,
    pub start_cursor: Vec2,
    /// Once the move has been committed (enqueued), further drag is
    /// ignored until the user releases the mouse.
    pub committed: bool,
}

pub struct DragInputPlugin;

impl Plugin for DragInputPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(bevy::picking::mesh_picking::MeshPickingPlugin)
            .init_resource::<DragState>()
            .add_observer(on_sticker_pressed)
            .add_observer(on_pointer_released)
            .add_systems(Update, update_drag);
    }
}

/// Mirror of [`on_sticker_pressed`] for the release half of the gesture.
/// Listens on the global `Pointer<Release>` stream so touch lift-up
/// clears `DragState` exactly like a mouse-up does on desktop.  This
/// also handles pointer-cancel (e.g. multi-touch starting while a drag
/// is in progress) — any release event ends the drag.
fn on_pointer_released(
    trigger: On<Pointer<Release>>,
    mut drag: ResMut<DragState>,
) {
    if trigger.event().button == PointerButton::Primary {
        drag.0 = None;
    }
}

fn on_sticker_pressed(
    trigger: On<Pointer<Press>>,
    stickers: Query<(&Sticker, &ChildOf)>,
    cubies: Query<&CubiePiece>,
    cube_state: Res<CubeState>,
    mut drag: ResMut<DragState>,
) {
    if trigger.event().button != PointerButton::Primary {
        return;
    }
    let entity = trigger.entity;
    let Ok((sticker, parent)) = stickers.get(entity) else {
        return;
    };
    let Ok(piece) = cubies.get(parent.parent()) else {
        return;
    };
    let Some(cubie) = cube_state.cube.cubies.get(piece.idx) else {
        return;
    };
    let world_face = cubie.orientation.apply(home_face_outward(sticker.face));
    drag.0 = Some(DragInfo {
        cubie_idx: piece.idx,
        world_face,
        start_cursor: trigger.event().pointer_location.position,
        committed: false,
    });
}

fn home_face_outward(face: Face) -> IVec3 {
    match face {
        Face::R => IVec3::X,
        Face::L => IVec3::NEG_X,
        Face::U => IVec3::Y,
        Face::D => IVec3::NEG_Y,
        Face::F => IVec3::Z,
        Face::B => IVec3::NEG_Z,
    }
}

fn update_drag(
    mut drag: ResMut<DragState>,
    mut pending: ResMut<PendingMoves>,
    cube_state: Res<CubeState>,
    pointers: Query<&PointerLocation>,
    cameras: Query<(&Camera, &GlobalTransform)>,
) {
    // Drag start/end are observed through `Pointer<Press>`/`Pointer<Release>`
    // events (see `on_sticker_pressed` and `on_pointer_released`). That
    // flow works for both mouse and touch via bevy_picking. This system
    // just polls the active pointer position to compute the screen-space
    // delta and commit the move once the drag exceeds threshold.
    let Some(info) = drag.0.as_mut() else {
        return;
    };
    if info.committed {
        return;
    }

    // Find the active cursor position. There's only one mouse pointer in
    // a desktop app; pick the first PointerLocation with a window.
    let Some(cursor_now) = pointers
        .iter()
        .find_map(|p| p.location.as_ref().map(|loc| loc.position))
    else {
        return;
    };
    let delta_screen = cursor_now - info.start_cursor;
    if delta_screen.length() < DRAG_THRESHOLD_PIXELS {
        return;
    }

    // Convert screen-space drag into world-space direction. We do this by
    // raycasting two cursor positions through the camera and taking the
    // delta of the points where each ray meets the picked face's plane.
    let Ok((camera, cam_xform)) = cameras.single() else {
        return;
    };
    let Some(world_drag) = world_drag_on_face_plane(
        camera,
        cam_xform,
        info.start_cursor,
        cursor_now,
        info.world_face,
        info.cubie_idx,
        &cube_state,
    ) else {
        return;
    };

    let cubie_pos = cube_state.cube.cubies[info.cubie_idx].current_pos;
    if let Some(m) = decide_move(info.world_face, cubie_pos, world_drag, cube_state.cube.size) {
        pending.enqueue(m);
        info.committed = true;
    }
}

/// Project both the start and current cursor onto the picked face's plane
/// (in world space) and return their delta. `world_face` is the world-space
/// outward direction of the picked sticker (snapshotted at click time).
fn world_drag_on_face_plane(
    camera: &Camera,
    cam_xform: &GlobalTransform,
    cursor_start: Vec2,
    cursor_now: Vec2,
    world_face: IVec3,
    cubie_idx: usize,
    state: &CubeState,
) -> Option<Vec3> {
    let cubie = state.cube.cubies.get(cubie_idx)?;
    let plane_normal = world_face_to_vec(world_face);
    let plane_point = lattice_to_world(cubie.current_pos)
        + plane_normal * cube_render::CUBIE_SIZE * 0.5;

    let ray_a = camera.viewport_to_world(cam_xform, cursor_start).ok()?;
    let ray_b = camera.viewport_to_world(cam_xform, cursor_now).ok()?;
    let p_a = ray_plane_intersect(ray_a, plane_point, plane_normal)?;
    let p_b = ray_plane_intersect(ray_b, plane_point, plane_normal)?;
    Some(p_b - p_a)
}

fn ray_plane_intersect(ray: Ray3d, plane_point: Vec3, plane_normal: Vec3) -> Option<Vec3> {
    let denom = ray.direction.dot(plane_normal);
    if denom.abs() < 1e-6 {
        return None;
    }
    let t = (plane_point - ray.origin).dot(plane_normal) / denom;
    if t < 0.0 {
        return None;
    }
    Some(ray.origin + ray.direction * t)
}

/// Pick the move from world face direction + world-space drag direction +
/// picked cubie position. Returns `None` only if no slice rotation moves
/// the cubie into the drag direction (shouldn't happen on well-formed
/// input).
fn decide_move(world_face: IVec3, cubie_pos: IVec3, world_drag: Vec3, size: u8) -> Option<Move> {
    let face_axis = world_face_axis_index(world_face);
    // The two in-plane world axes (anything but face_axis).
    let (axis_u, axis_v) = match face_axis {
        0 => (1, 2),
        1 => (0, 2),
        2 => (0, 1),
        _ => unreachable!(),
    };
    let unit_u = axis_unit(axis_u);
    let unit_v = axis_unit(axis_v);
    let du = world_drag.dot(unit_u);
    let dv = world_drag.dot(unit_v);
    let (chosen_axis, other_axis, drag_along) = if du.abs() > dv.abs() {
        (axis_u, axis_v, du.signum() * unit_u)
    } else {
        (axis_v, axis_u, dv.signum() * unit_v)
    };
    let _ = chosen_axis;

    let cubie_other = component_i32(cubie_pos, other_axis);
    let slice_face;
    let depth: u8;
    let outer = (size as i32) - 1;
    if cubie_other == outer {
        slice_face = positive_face_on_axis(other_axis);
        depth = 1;
    } else if cubie_other == -outer {
        slice_face = negative_face_on_axis(other_axis);
        depth = 1;
    } else if cubie_other == 0 {
        // Equator-like middle slice on odd-size cubes.
        slice_face = positive_face_on_axis(other_axis);
        depth = (size + 1) / 2;
    } else {
        // Inner layer on a 4×4 / 5×5: pick the closer face.
        if cubie_other > 0 {
            slice_face = positive_face_on_axis(other_axis);
            depth = ((size as i32 + 1 - cubie_other) / 2) as u8;
        } else {
            slice_face = negative_face_on_axis(other_axis);
            depth = ((size as i32 + 1 + cubie_other) / 2) as u8;
        }
    }

    // Try Cw / Ccw / Half — pick the one whose effect on this cubie has
    // the largest projection onto the drag direction.
    let mut best: Option<(Move, f32)> = None;
    for turn in [Turn::Cw, Turn::Ccw, Turn::Half] {
        let m = Move { face: slice_face, turn, depth, wide: false };
        let new_pos = m.rotation().apply(cubie_pos);
        let motion = new_pos - cubie_pos;
        let motion_v = Vec3::new(motion.x as f32, motion.y as f32, motion.z as f32);
        let alignment = motion_v.dot(drag_along);
        if alignment > 0.0 && best.map_or(true, |(_, a)| alignment > a) {
            best = Some((m, alignment));
        }
    }
    best.map(|(m, _)| m)
}

fn axis_unit(axis: usize) -> Vec3 {
    match axis {
        0 => Vec3::X,
        1 => Vec3::Y,
        2 => Vec3::Z,
        _ => unreachable!(),
    }
}

fn positive_face_on_axis(axis: usize) -> Face {
    match axis {
        0 => Face::R,
        1 => Face::U,
        2 => Face::F,
        _ => unreachable!(),
    }
}

fn negative_face_on_axis(axis: usize) -> Face {
    match axis {
        0 => Face::L,
        1 => Face::D,
        2 => Face::B,
        _ => unreachable!(),
    }
}

fn component_i32(v: IVec3, axis: usize) -> i32 {
    match axis {
        0 => v.x,
        1 => v.y,
        2 => v.z,
        _ => unreachable!(),
    }
}

fn world_face_to_vec(world_face: IVec3) -> Vec3 {
    Vec3::new(world_face.x as f32, world_face.y as f32, world_face.z as f32)
}

fn world_face_axis_index(world_face: IVec3) -> usize {
    if world_face.x != 0 {
        0
    } else if world_face.y != 0 {
        1
    } else {
        2
    }
}

fn lattice_to_world(p: IVec3) -> Vec3 {
    Vec3::new(
        p.x as f32 * cube_render::LATTICE_SCALE,
        p.y as f32 * cube_render::LATTICE_SCALE,
        p.z as f32 * cube_render::LATTICE_SCALE,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The classic case: pick the URF cubie's R-face sticker (world face
    /// = +X), drag toward the back of the cube (–Z direction). Under WCA
    /// "CW from outside" convention U-cw cycles UFR → ULF → UBL → UBR
    /// (URF moves in –X), so the move that pushes URF in –Z is U-ccw.
    #[test]
    fn drag_back_on_urf_r_sticker_is_u_ccw() {
        let move_data = decide_move(IVec3::X, IVec3::new(2, 2, 2), -Vec3::Z, 3).unwrap();
        assert_eq!(move_data.face, Face::U);
        assert_eq!(move_data.turn, Turn::Ccw);
        assert_eq!(move_data.depth, 1);
        assert!(!move_data.wide);
    }

    #[test]
    fn drag_down_on_urf_r_sticker_is_f_cw() {
        // Chosen axis = Y; OTHER axis = Z. Cubie z=+2 → slice F.
        // F-cw cycles UFR → DFR (URF (2,2,2) → (2,-2,2)): drag direction –Y.
        let m = decide_move(IVec3::X, IVec3::new(2, 2, 2), -Vec3::Y, 3).unwrap();
        assert_eq!(m.face, Face::F);
        assert_eq!(m.turn, Turn::Cw);
    }

    #[test]
    fn middle_slice_drag_uses_inner_depth() {
        // Pick a sticker on R face at the equator (cubie y=0). Drag –Z.
        // Chosen axis Z, OTHER axis Y, cubie y=0 → middle slice (depth 2
        // on the positive Y face = E-equator-style).
        let m = decide_move(IVec3::X, IVec3::new(2, 0, 2), -Vec3::Z, 3).unwrap();
        assert_eq!(m.face, Face::U);
        assert_eq!(m.depth, 2);
        assert!(!m.wide);
    }

    #[test]
    fn drag_on_4x4_inner_layer_picks_inner_slice() {
        // Pick a sticker on the R face of a 4×4 inner-column cubie: world
        // face = +X, cubie at (3, 3, 1). Drag in –Y direction. Chosen
        // axis Y, OTHER axis Z. Cubie z=+1 is INNER (size=4 → outer=±3),
        // so the move is on F face at depth 2 (single-layer inner slice).
        let m = decide_move(IVec3::X, IVec3::new(3, 3, 1), -Vec3::Y, 4).unwrap();
        assert_eq!(m.face, Face::F);
        assert_eq!(m.depth, 2);
        assert!(!m.wide);
        // And the rotation actually moves the cubie in -Y.
        let after = m.rotation().apply(IVec3::new(3, 3, 1));
        let dy = (after.y - 3) as f32;
        assert!(dy < 0.0, "expected -Y motion; got {after:?}");
    }

    /// Regression for the user's "second drag goes the wrong way" report.
    /// After F-cw, the URF cubie is at UFL with the R-sticker rotated to
    /// world +Y. Dragging it in the +X direction must move that cubie in
    /// the +X direction (whichever face turn achieves that). Under the
    /// pre-fix logic that ignored the cubie's accumulated orient, the
    /// move would have been picked on the wrong axis and produced motion
    /// orthogonal to or against the drag.
    #[test]
    fn world_face_drives_decision_after_rotation() {
        let world_face_after = IVec3::Y;          // sticker rotated to +Y
        let cubie_pos_after = IVec3::new(-2, 2, 2); // UFL position
        let m = decide_move(world_face_after, cubie_pos_after, Vec3::X, 3).unwrap();
        let after = m.rotation().apply(cubie_pos_after);
        let dx = (after.x - cubie_pos_after.x) as f32;
        assert!(dx > 0.0,
            "drag +X must move cubie in +X; got move={m:?}, motion.x={dx}");
    }

}
