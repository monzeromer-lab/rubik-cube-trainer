//! Bevy plugin that turns a [`cube_core::Cube`] into 3D entities — a
//! procedurally generated cubie body per piece plus six sticker quads,
//! synced from the cube state every frame.
//!
//! See plan §5 (architecture), §5.5 (animation), §10 (rendering),
//! §11.4 (animation curve).
//!
//! ## Animation model
//!
//! The plan recommends parent-reparenting: spawn an empty parent at the
//! cube center, reparent affected cubies under it, animate the parent's
//! rotation, despawn the parent at the end. We get the same logical
//! invariant ("CubeState only ever observes a clean post-move state")
//! more cheaply by leaving cubies parented to the world root and applying
//! the in-progress rotation directly to their transforms during
//! [`tick_animation`]. When the animation completes we apply the move to
//! the logical [`CubeState`], which trips `state.is_changed()` and lets
//! [`sync_cubie_transforms`] snap to the final position next frame.

use std::collections::VecDeque;

use bevy::color::palettes::css;
use bevy::prelude::*;
use cube_core::{Color as CubeColor, Cube, Cubie, Face, Move, Orient, Turn};
use glam::{IVec3, Mat3};

/// Visual size of a cubie, in world units. Cubies are placed on a grid with
/// unit spacing — pick `CUBIE_SIZE` slightly under 1.0 to leave a thin gap
/// (a real plastic cube has a tiny seam between cubies).
pub const CUBIE_SIZE: f32 = 0.95;
/// Offset of stickers from the cubie surface to avoid z-fighting.
pub const STICKER_OFFSET: f32 = 0.501;
/// Side length of a sticker square as a fraction of the cubie face.
pub const STICKER_SIZE: f32 = 0.84;

/// World scale per integer-lattice unit. cube_core uses a 2-unit grid
/// (positions step by 2); we scale by 0.5 so a cubie at lattice
/// position (2, 0, 0) renders at world position (1, 0, 0).
pub const LATTICE_SCALE: f32 = 0.5;

/// Default per-move animation duration in seconds. Per plan §11.4: ease-out,
/// 120 ms feels snappy for trainer use.
pub const DEFAULT_ANIM_DURATION: f32 = 0.12;

/// The Bevy resource holding the live cube state.
///
/// `history` records every move that's been logically committed to `cube`
/// (in commit order) up to [`Self::MAX_HISTORY`]. `redo_stack` holds moves
/// that have been undone — its top is the most-recently-undone move. Both
/// stacks are managed in [`tick_animation`] (for normal user moves) and
/// in the app's undo/redo handler (for Ctrl+Z / Ctrl+Y).
#[derive(Resource, Debug, Clone)]
pub struct CubeState {
    pub cube: Cube,
    pub history: VecDeque<Move>,
    pub redo_stack: VecDeque<Move>,
}

impl CubeState {
    /// Cap history depth. A WCA scramble + typical solve is < 100 moves,
    /// so 1024 lets the user reach back through several solves before
    /// the oldest entries get rotated out. Keeps memory bounded.
    pub const MAX_HISTORY: usize = 1024;

    pub fn new(cube: Cube) -> Self {
        Self {
            cube,
            history: VecDeque::new(),
            redo_stack: VecDeque::new(),
        }
    }

    pub fn solved(size: u8) -> Self {
        Self::new(Cube::solved(size).expect("size in 2..=5"))
    }

    /// Drop both stacks. Call this whenever the cube is reset to a fresh
    /// state (size switch, scramble setup, drill setup) — keeping stale
    /// history around would let undo desync from the visible cube.
    pub fn reset_history(&mut self) {
        self.history.clear();
        self.redo_stack.clear();
    }
}

/// Origin of the next move to be committed by [`tick_animation`]. Default
/// is `User` — which causes the commit to push to history and clear the
/// redo stack. The undo/redo handler sets this to `Undo`/`Redo` before
/// enqueueing, having already updated the stacks itself; the commit then
/// skips its own bookkeeping.
#[derive(Resource, Debug, Default, Clone, Copy)]
pub struct NextCommitOrigin(pub MoveOrigin);

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum MoveOrigin {
    #[default]
    User,
    Undo,
    Redo,
}

#[derive(Resource, Debug, Clone, Copy)]
pub struct CubeRenderConfig {
    pub size: u8,
    pub animation_duration: f32,
}

impl Default for CubeRenderConfig {
    fn default() -> Self {
        Self {
            size: 3,
            animation_duration: DEFAULT_ANIM_DURATION,
        }
    }
}

/// Queue of moves waiting to animate. Push to the back; the animation
/// system pops from the front when no animation is in flight.
#[derive(Resource, Debug, Default)]
pub struct PendingMoves(pub VecDeque<Move>);

impl PendingMoves {
    pub fn enqueue(&mut self, m: Move) {
        self.0.push_back(m);
    }

    pub fn enqueue_all<I: IntoIterator<Item = Move>>(&mut self, it: I) {
        self.0.extend(it);
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

/// In-flight move animation. Cleared back to `None` once the elapsed time
/// reaches `duration`, at which point the logical move is applied.
#[derive(Resource, Debug, Default)]
pub struct ActiveAnimation(pub Option<MoveAnimation>);

#[derive(Debug, Clone, Copy)]
pub struct MoveAnimation {
    pub move_data: Move,
    pub elapsed: f32,
    pub duration: f32,
}

impl MoveAnimation {
    pub fn new(m: Move, duration: f32) -> Self {
        Self { move_data: m, elapsed: 0.0, duration }
    }

    /// Progress in `[0, 1]`.
    pub fn progress(&self) -> f32 {
        (self.elapsed / self.duration).clamp(0.0, 1.0)
    }

    /// Eased progress (out-quadratic — see plan §11.4).
    pub fn eased(&self) -> f32 {
        let t = self.progress();
        1.0 - (1.0 - t) * (1.0 - t)
    }

    pub fn is_finished(&self) -> bool {
        self.elapsed >= self.duration
    }
}

/// Marker on the visual entity for a single cubie piece.
#[derive(Component, Debug, Clone, Copy)]
pub struct CubiePiece {
    /// Index into `CubeState.cube.cubies` — used to pull current pos/orient.
    pub idx: usize,
}

/// Marker on a sticker child of a cubie. The face indicates which outward
/// face this sticker covers in the cubie's local frame.
#[derive(Component, Debug, Clone, Copy)]
pub struct Sticker {
    pub face: Face,
}

/// Plugin: spawns the cube on startup, animates queued moves, syncs
/// cubie transforms from logical state.
pub struct CubeRenderPlugin;

impl Plugin for CubeRenderPlugin {
    fn build(&self, app: &mut App) {
        let initial_config = CubeRenderConfig::default();
        // Insert CubeState eagerly at plugin build so any system depending
        // on it never sees a missing resource on frame 1. ensure_cube_built
        // updates this resource (rather than re-inserting via Commands)
        // when the configured size changes.
        app.insert_resource(CubeState::solved(initial_config.size))
            .init_resource::<CubeRenderConfig>()
            .init_resource::<PendingMoves>()
            .init_resource::<ActiveAnimation>()
            .init_resource::<NextCommitOrigin>()
            .add_systems(
                Update,
                (
                    ensure_cube_built,
                    start_animation,
                    tick_animation,
                    sync_cubie_transforms,
                )
                    .chain(),
            );
    }
}

/// Number of visible cubies on a size-N cube. The interior `(N-2)³`
/// invisible block is excluded.
pub fn expected_cubie_count(size: u8) -> usize {
    let n = size as i32;
    let total = n.pow(3);
    let interior = (n - 2).max(0).pow(3);
    (total - interior) as usize
}

/// Spawn or rebuild the cube. Runs on the first frame (cubies = 0,
/// expected = 26 on size=3, mismatch → spawn) and again any time
/// `CubeRenderConfig.size` changes (mismatch in counts → despawn + spawn).
pub fn ensure_cube_built(
    mut commands: Commands,
    config: Res<CubeRenderConfig>,
    cubies: Query<Entity, With<CubiePiece>>,
    mut state: ResMut<CubeState>,
    mut pending: ResMut<PendingMoves>,
    mut active: ResMut<ActiveAnimation>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let expected = expected_cubie_count(config.size);
    let current = cubies.iter().count();
    if current == expected {
        return;
    }

    // Tear down the previous instance — children (stickers) despawn with
    // their parent cubie because of Bevy's hierarchy propagation.
    for entity in &cubies {
        commands.entity(entity).despawn();
    }
    pending.0.clear();
    active.0 = None;

    let cube_state = Cube::solved(config.size).expect("valid size");
    spawn_cube_entities(&mut commands, &mut meshes, &mut materials, &cube_state);
    state.cube = cube_state;
    state.reset_history();
}

fn spawn_cube_entities(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    cube_state: &Cube,
) {
    let palette = ColorPalette::default();
    let cubie_mesh = meshes.add(Cuboid::from_size(Vec3::splat(CUBIE_SIZE)));
    let sticker_mesh = meshes.add(Rectangle::from_size(Vec2::splat(STICKER_SIZE)));

    let plastic_mat = materials.add(plastic_material());
    let face_mat: [(Face, Handle<StandardMaterial>); 6] = [
        (Face::U, materials.add(sticker_material(palette.color(CubeColor::U)))),
        (Face::D, materials.add(sticker_material(palette.color(CubeColor::D)))),
        (Face::F, materials.add(sticker_material(palette.color(CubeColor::F)))),
        (Face::B, materials.add(sticker_material(palette.color(CubeColor::B)))),
        (Face::R, materials.add(sticker_material(palette.color(CubeColor::R)))),
        (Face::L, materials.add(sticker_material(palette.color(CubeColor::L)))),
    ];

    let outer = (cube_state.size as i32) - 1;
    for (idx, cubie) in cube_state.cubies.iter().enumerate() {
        let world_pos = lattice_to_world(cubie.current_pos);
        let cubie_id = commands
            .spawn((
                CubiePiece { idx },
                Mesh3d(cubie_mesh.clone()),
                MeshMaterial3d(plastic_mat.clone()),
                Transform::from_translation(world_pos),
                Visibility::Inherited,
            ))
            .id();

        for face in faces_of(cubie, outer) {
            let mat = face_material(face, &face_mat);
            let local_xform = sticker_transform(face);
            commands.entity(cubie_id).with_children(|parent| {
                parent.spawn((
                    Sticker { face },
                    Mesh3d(sticker_mesh.clone()),
                    MeshMaterial3d(mat),
                    local_xform,
                    Visibility::Inherited,
                ));
            });
        }
    }
}

/// Pop the head of the queue into [`ActiveAnimation`] when no animation is
/// running. Order matters: this must run before `tick_animation` so the
/// new animation gets one tick of progress in the same frame.
pub fn start_animation(
    mut pending: ResMut<PendingMoves>,
    mut active: ResMut<ActiveAnimation>,
    config: Res<CubeRenderConfig>,
) {
    if active.0.is_some() {
        return;
    }
    if let Some(m) = pending.0.pop_front() {
        active.0 = Some(MoveAnimation::new(m, config.animation_duration));
    }
}

pub fn tick_animation(
    time: Res<Time>,
    mut active: ResMut<ActiveAnimation>,
    mut state: ResMut<CubeState>,
    mut origin: ResMut<NextCommitOrigin>,
    mut query: Query<(&CubiePiece, &mut Transform)>,
) {
    let Some(mut anim) = active.0.take() else {
        return;
    };
    anim.elapsed += time.delta_secs();

    let target_angle = move_angle_radians(anim.move_data);
    let axis = move_axis_world(anim.move_data);
    let current_angle = anim.eased() * target_angle;
    let partial_rot = Quat::from_axis_angle(axis, current_angle);

    let coords: Vec<i32> = anim.move_data.affected_layer_coords(state.cube.size).collect();
    let move_axis_idx = anim.move_data.face.axis() as usize;

    for (piece, mut xform) in &mut query {
        let cubie = &state.cube.cubies[piece.idx];
        let pre_orient_quat = orient_to_quat(cubie.orientation);
        let v = component(cubie.current_pos, move_axis_idx);
        if coords.contains(&v) {
            // Affected: rotate this cubie's pre-move world frame around
            // the cube center by the partial move rotation. The cubie
            // body's stickers (children with local transforms baked for
            // the home outward faces) come along with it via Bevy's
            // transform propagation.
            let base_pos = lattice_to_world(cubie.current_pos);
            xform.translation = partial_rot * base_pos;
            xform.rotation = partial_rot * pre_orient_quat;
        } else {
            // Untouched: hold its existing world frame, including any
            // orientation accumulated by past moves.
            xform.translation = lattice_to_world(cubie.current_pos);
            xform.rotation = pre_orient_quat;
        }
    }

    if anim.is_finished() {
        // Atomic logical commit. `sync_cubie_transforms` handles the snap
        // next frame because `state` is now `is_changed`.
        let _ = state.cube.apply(anim.move_data);
        match origin.0 {
            MoveOrigin::User => {
                state.history.push_back(anim.move_data);
                if state.history.len() > CubeState::MAX_HISTORY {
                    state.history.pop_front();
                }
                state.redo_stack.clear();
            }
            MoveOrigin::Undo | MoveOrigin::Redo => {
                // Stack bookkeeping was done by the undo/redo handler at
                // enqueue time. Reset to default for the next move.
            }
        }
        origin.0 = MoveOrigin::User;
        active.0 = None;
    } else {
        active.0 = Some(anim);
    }
}

pub fn sync_cubie_transforms(
    state: Res<CubeState>,
    active: Res<ActiveAnimation>,
    mut query: Query<(&CubiePiece, &mut Transform)>,
) {
    // Don't fight with the animation tick when one is in flight.
    if active.0.is_some() {
        return;
    }
    if !state.is_changed() {
        return;
    }
    for (piece, mut xform) in &mut query {
        if let Some(cubie) = state.cube.cubies.get(piece.idx) {
            xform.translation = lattice_to_world(cubie.current_pos);
            xform.rotation = orient_to_quat(cubie.orientation);
        }
    }
}

/// Convert a `cube_core::Orient` (signed-permutation rotation matrix) to a
/// `Quat`. The orient group is the 24 chiral octahedral rotations; each
/// element is an honest rotation matrix (det +1), so `Quat::from_mat3` is
/// well-defined.
fn orient_to_quat(o: Orient) -> Quat {
    let data = o.data();
    let mut cols = [Vec3::ZERO; 3];
    // Row i of the matrix has its single nonzero entry at column
    // out_from[i] with value out_sign[i].
    for i in 0..3 {
        let col = data.out_from[i] as usize;
        cols[col][i] = data.out_sign[i] as f32;
    }
    Quat::from_mat3(&Mat3::from_cols(cols[0], cols[1], cols[2]))
}

fn lattice_to_world(p: IVec3) -> Vec3 {
    Vec3::new(
        p.x as f32 * LATTICE_SCALE,
        p.y as f32 * LATTICE_SCALE,
        p.z as f32 * LATTICE_SCALE,
    )
}

fn component(v: IVec3, axis: usize) -> i32 {
    match axis {
        0 => v.x,
        1 => v.y,
        2 => v.z,
        _ => unreachable!(),
    }
}

/// World rotation axis (unit vector) for a move. Points outward from the
/// affected face: a Cw turn is then a positive right-hand rotation.
fn move_axis_world(m: Move) -> Vec3 {
    let sign = m.face.axis_sign() as f32;
    match m.face.axis() {
        0 => Vec3::X * sign,
        1 => Vec3::Y * sign,
        2 => Vec3::Z * sign,
        _ => unreachable!(),
    }
}

/// Signed quarter-turn angle in radians around [`move_axis_world`].
///
/// WCA "CW from outside the +s face" is right-hand rotation around -s,
/// i.e. a negative angle when expressed around the outward axis +s. So
/// `Turn::Cw` returns `-π/2` here, matching the rotation that
/// [`Move::rotation`] computes in the cube's logical group.
fn move_angle_radians(m: Move) -> f32 {
    match m.turn {
        Turn::Cw => -std::f32::consts::FRAC_PI_2,
        Turn::Half => std::f32::consts::PI,
        Turn::Ccw => std::f32::consts::FRAC_PI_2,
    }
}

fn faces_of(cubie: &Cubie, outer: i32) -> Vec<Face> {
    let mut out = Vec::with_capacity(3);
    let p = cubie.solved_pos;
    if p.x == outer { out.push(Face::R); }
    if p.x == -outer { out.push(Face::L); }
    if p.y == outer { out.push(Face::U); }
    if p.y == -outer { out.push(Face::D); }
    if p.z == outer { out.push(Face::F); }
    if p.z == -outer { out.push(Face::B); }
    out
}

fn sticker_transform(face: Face) -> Transform {
    let (rot, offset) = match face {
        Face::F => (Quat::IDENTITY, Vec3::Z),
        Face::B => (Quat::from_rotation_y(std::f32::consts::PI), -Vec3::Z),
        Face::R => (Quat::from_rotation_y(std::f32::consts::FRAC_PI_2), Vec3::X),
        Face::L => (Quat::from_rotation_y(-std::f32::consts::FRAC_PI_2), -Vec3::X),
        Face::U => (Quat::from_rotation_x(-std::f32::consts::FRAC_PI_2), Vec3::Y),
        Face::D => (Quat::from_rotation_x(std::f32::consts::FRAC_PI_2), -Vec3::Y),
    };
    Transform::from_translation(offset * STICKER_OFFSET).with_rotation(rot)
}

fn face_material(face: Face, table: &[(Face, Handle<StandardMaterial>); 6]) -> Handle<StandardMaterial> {
    table
        .iter()
        .find(|(f, _)| *f == face)
        .map(|(_, m)| m.clone())
        .unwrap()
}

fn plastic_material() -> StandardMaterial {
    StandardMaterial {
        base_color: Color::srgb(0.05, 0.05, 0.05),
        perceptual_roughness: 0.6,
        reflectance: 0.5,
        metallic: 0.0,
        ..default()
    }
}

fn sticker_material(c: Color) -> StandardMaterial {
    // Stickers are emissive at full intensity so the colour shows on
    // every backend regardless of lighting and tonemapping. `unlit`
    // alone wasn't enough on Mali-G57 — the unlit path still appeared
    // washed-grey there. Setting `emissive` to a scaled `base_color`
    // forces the PBR shader's add-emissive branch to dominate, which
    // produces a guaranteed-visible colour. `LinearRgba::from(srgb)`
    // does the sRGB → linear conversion so the on-screen colour
    // matches what the palette declares.
    let linear = LinearRgba::from(c);
    StandardMaterial {
        base_color: c,
        emissive: LinearRgba::rgb(linear.red * 4.0, linear.green * 4.0, linear.blue * 4.0),
        perceptual_roughness: 1.0,
        reflectance: 0.0,
        metallic: 0.0,
        ..default()
    }
}

/// Mapping from logical sticker color to a rendered color. The default
/// scheme follows WCA convention.
#[derive(Resource, Debug, Clone, Copy)]
pub struct ColorPalette {
    pub up: Color,
    pub down: Color,
    pub front: Color,
    pub back: Color,
    pub right: Color,
    pub left: Color,
}

impl Default for ColorPalette {
    fn default() -> Self {
        Self {
            up: Color::srgb(0.95, 0.95, 0.95),       // white
            down: Color::Srgba(css::YELLOW),
            front: Color::srgb(0.0, 0.6, 0.18),       // WCA green
            back: Color::srgb(0.0, 0.3, 0.85),        // WCA blue
            right: Color::srgb(0.85, 0.05, 0.05),     // red
            left: Color::srgb(1.0, 0.45, 0.0),        // orange
        }
    }
}

impl ColorPalette {
    pub fn color(&self, c: CubeColor) -> Color {
        match c {
            CubeColor::U => self.up,
            CubeColor::D => self.down,
            CubeColor::F => self.front,
            CubeColor::B => self.back,
            CubeColor::R => self.right,
            CubeColor::L => self.left,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cube_core::Turn;

    /// Drive an `App` headlessly until the queued moves all commit, then
    /// verify the logical state matches what `cube_core` would produce
    /// when the moves are applied directly. Animation duration is set to
    /// effectively zero so each move commits on its first tick — the
    /// invariant under test is that the schedule reaches the right
    /// terminal state, not the visual easing.
    #[test]
    fn animation_completes_and_logical_state_matches_cube_core() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.add_plugins(bevy::asset::AssetPlugin::default());
        app.init_asset::<Mesh>();
        app.init_asset::<StandardMaterial>();
        app.insert_resource(CubeRenderConfig { size: 3, animation_duration: 0.0 });
        app.init_resource::<PendingMoves>();
        app.init_resource::<ActiveAnimation>();
        app.init_resource::<NextCommitOrigin>();
        app.insert_resource(CubeState::solved(3));
        app.add_systems(
            Update,
            (start_animation, tick_animation, sync_cubie_transforms).chain(),
        );

        let scramble = vec![
            Move::face(Face::R, Turn::Cw),
            Move::face(Face::U, Turn::Cw),
            Move::face(Face::R, Turn::Ccw),
            Move::face(Face::U, Turn::Ccw),
        ];
        {
            let mut pending = app.world_mut().resource_mut::<PendingMoves>();
            for m in scramble.iter().copied() {
                pending.enqueue(m);
            }
        }

        // With duration=0, each frame: start_animation pops a move,
        // tick_animation finishes it immediately. Two scramble moves per
        // frame is plausible — bound generously.
        let mut steps = 0;
        loop {
            app.update();
            steps += 1;
            let pending_empty = app.world().resource::<PendingMoves>().is_empty();
            let no_anim = app.world().resource::<ActiveAnimation>().0.is_none();
            if pending_empty && no_anim {
                break;
            }
            assert!(steps < 100, "schedule didn't drain in 100 frames");
        }

        let mut expected = Cube::solved(3).unwrap();
        for m in scramble {
            expected.apply(m).unwrap();
        }
        let actual = app.world().resource::<CubeState>().cube.clone();
        assert_eq!(actual, expected);
    }

    /// User moves are recorded into `history` at commit time, and the redo
    /// stack is cleared. Verifies the default `NextCommitOrigin::User`
    /// path inside `tick_animation`.
    #[test]
    fn user_moves_are_recorded_to_history_in_commit_order() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.add_plugins(bevy::asset::AssetPlugin::default());
        app.init_asset::<Mesh>();
        app.init_asset::<StandardMaterial>();
        app.insert_resource(CubeRenderConfig { size: 3, animation_duration: 0.0 });
        app.init_resource::<PendingMoves>();
        app.init_resource::<ActiveAnimation>();
        app.init_resource::<NextCommitOrigin>();
        app.insert_resource(CubeState::solved(3));
        app.add_systems(
            Update,
            (start_animation, tick_animation, sync_cubie_transforms).chain(),
        );

        // Pre-populate redo_stack so we can confirm the User commit clears it.
        {
            let mut state = app.world_mut().resource_mut::<CubeState>();
            state.redo_stack.push_back(Move::face(Face::F, Turn::Half));
        }

        let moves = [
            Move::face(Face::R, Turn::Cw),
            Move::face(Face::U, Turn::Ccw),
            Move::face(Face::L, Turn::Half),
        ];
        {
            let mut pending = app.world_mut().resource_mut::<PendingMoves>();
            for m in moves.iter().copied() {
                pending.enqueue(m);
            }
        }
        for _ in 0..50 {
            app.update();
            if app.world().resource::<PendingMoves>().is_empty()
                && app.world().resource::<ActiveAnimation>().0.is_none()
            {
                break;
            }
        }

        let state = app.world().resource::<CubeState>();
        assert_eq!(state.history.len(), 3);
        assert_eq!(state.history[0], moves[0]);
        assert_eq!(state.history[2], moves[2]);
        assert!(state.redo_stack.is_empty());
    }

    /// When the next commit's origin is `Undo` or `Redo`, `tick_animation`
    /// must not touch history or redo (the handler already updated them
    /// at enqueue time) and must reset origin to `User` after the commit.
    #[test]
    fn undo_origin_commit_skips_history_bookkeeping_and_resets_origin() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.add_plugins(bevy::asset::AssetPlugin::default());
        app.init_asset::<Mesh>();
        app.init_asset::<StandardMaterial>();
        app.insert_resource(CubeRenderConfig { size: 3, animation_duration: 0.0 });
        app.init_resource::<PendingMoves>();
        app.init_resource::<ActiveAnimation>();
        app.init_resource::<NextCommitOrigin>();
        app.insert_resource(CubeState::solved(3));
        app.add_systems(
            Update,
            (start_animation, tick_animation, sync_cubie_transforms).chain(),
        );

        // Pre-set state as if an undo handler had just run: history with
        // R already popped, redo holds R, pending holds R'.
        let r = Move::face(Face::R, Turn::Cw);
        {
            let mut state = app.world_mut().resource_mut::<CubeState>();
            state.redo_stack.push_back(r);
        }
        {
            let mut origin = app.world_mut().resource_mut::<NextCommitOrigin>();
            origin.0 = MoveOrigin::Undo;
        }
        {
            let mut pending = app.world_mut().resource_mut::<PendingMoves>();
            pending.enqueue(r.inverse());
        }
        for _ in 0..10 {
            app.update();
            if app.world().resource::<PendingMoves>().is_empty()
                && app.world().resource::<ActiveAnimation>().0.is_none()
            {
                break;
            }
        }

        let state = app.world().resource::<CubeState>();
        // Commit must not have pushed R' to history nor cleared redo.
        assert!(state.history.is_empty(), "history was modified by Undo commit");
        assert_eq!(state.redo_stack.len(), 1, "redo was cleared by Undo commit");
        // Origin must be back to User for the next move.
        assert_eq!(
            app.world().resource::<NextCommitOrigin>().0,
            MoveOrigin::User
        );
    }

    /// With a non-zero animation duration, a single move requires several
    /// frames to complete, and the logical `CubeState` only changes after
    /// the elapsed time crosses the duration threshold.
    #[test]
    fn animation_takes_multiple_ticks_with_nonzero_duration() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.add_plugins(bevy::asset::AssetPlugin::default());
        app.init_asset::<Mesh>();
        app.init_asset::<StandardMaterial>();
        // Long-ish duration so multiple ticks of MinimalPlugins time
        // accumulation fit before completion.
        app.insert_resource(CubeRenderConfig { size: 3, animation_duration: 0.5 });
        app.init_resource::<PendingMoves>();
        app.init_resource::<ActiveAnimation>();
        app.init_resource::<NextCommitOrigin>();
        app.insert_resource(CubeState::solved(3));
        app.add_systems(
            Update,
            (start_animation, tick_animation, sync_cubie_transforms).chain(),
        );

        app.world_mut().resource_mut::<PendingMoves>()
            .enqueue(Move::face(Face::R, Turn::Cw));

        // First tick: start animation; logical state still solved.
        app.update();
        assert!(app.world().resource::<ActiveAnimation>().0.is_some(),
            "animation should be active after first tick");
        assert!(app.world().resource::<CubeState>().cube.is_solved(),
            "logical commit must wait for animation completion");
    }

    #[test]
    fn orient_to_quat_identity() {
        assert_eq!(orient_to_quat(Orient::IDENTITY), Quat::IDENTITY);
    }

    #[test]
    fn orient_to_quat_matches_axis_rotation_for_face_turns() {
        // For a face turn with rotation R (Orient), orient_to_quat(R)
        // applied to a vector must match the rotation's effect on that
        // vector — sanity-checked against the face turn quaternion.
        for face in [Face::U, Face::R, Face::F] {
            for turn in [Turn::Cw, Turn::Half, Turn::Ccw] {
                let m = Move::face(face, turn);
                let orient_quat = orient_to_quat(m.rotation());
                let face_quat = Quat::from_axis_angle(
                    move_axis_world(m),
                    move_angle_radians(m),
                );
                let v = Vec3::new(0.7, -0.3, 0.5);
                let a = orient_quat * v;
                let b = face_quat * v;
                assert!((a - b).length() < 1e-5,
                    "{face:?} {turn:?}: orient {a:?} vs axis {b:?}");
            }
        }
    }

    #[test]
    fn cubie_count_matches_known_sizes() {
        assert_eq!(expected_cubie_count(2), 8);
        assert_eq!(expected_cubie_count(3), 26);
        assert_eq!(expected_cubie_count(4), 56);
        assert_eq!(expected_cubie_count(5), 98);
    }

    fn rebuild_test_app() -> App {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.add_plugins(bevy::asset::AssetPlugin::default());
        app.init_asset::<Mesh>();
        app.init_asset::<StandardMaterial>();
        app.init_resource::<CubeRenderConfig>();
        app.init_resource::<PendingMoves>();
        app.init_resource::<ActiveAnimation>();
        app.init_resource::<NextCommitOrigin>();
        // Match the plugin: pre-insert CubeState so ensure_cube_built can
        // read it as ResMut without panicking.
        app.insert_resource(CubeState::solved(3));
        app.add_systems(Update, ensure_cube_built);
        app
    }

    #[test]
    fn ensure_cube_built_spawns_initial_set_of_cubies() {
        let mut app = rebuild_test_app();
        app.update();
        let count = app
            .world_mut()
            .query_filtered::<Entity, With<CubiePiece>>()
            .iter(app.world())
            .count();
        assert_eq!(count, 26);
    }

    #[test]
    fn ensure_cube_built_rebuilds_on_size_change() {
        let mut app = rebuild_test_app();
        app.update();
        app.world_mut().resource_mut::<CubeRenderConfig>().size = 5;
        app.update();
        let count = app
            .world_mut()
            .query_filtered::<Entity, With<CubiePiece>>()
            .iter(app.world())
            .count();
        assert_eq!(count, 98);

        app.world_mut().resource_mut::<CubeRenderConfig>().size = 2;
        app.update();
        let count = app
            .world_mut()
            .query_filtered::<Entity, With<CubiePiece>>()
            .iter(app.world())
            .count();
        assert_eq!(count, 8);
    }

    #[test]
    fn animation_works_for_size_four() {
        // Same end-to-end test as size 3, but on a 4×4 with a wide turn
        // to exercise inner-layer participation.
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.add_plugins(bevy::asset::AssetPlugin::default());
        app.init_asset::<Mesh>();
        app.init_asset::<StandardMaterial>();
        app.insert_resource(CubeRenderConfig { size: 4, animation_duration: 0.0 });
        app.init_resource::<PendingMoves>();
        app.init_resource::<ActiveAnimation>();
        app.init_resource::<NextCommitOrigin>();
        app.insert_resource(CubeState::solved(4));
        app.add_systems(
            Update,
            (start_animation, tick_animation, sync_cubie_transforms).chain(),
        );

        let scramble = vec![
            Move { face: Face::R, turn: Turn::Cw, depth: 1, wide: false },
            Move { face: Face::U, turn: Turn::Cw, depth: 2, wide: true },
            Move { face: Face::F, turn: Turn::Half, depth: 1, wide: false },
        ];
        app.world_mut().resource_mut::<PendingMoves>().enqueue_all(scramble.clone());

        let mut steps = 0;
        loop {
            app.update();
            steps += 1;
            let p_done = app.world().resource::<PendingMoves>().is_empty();
            let a_done = app.world().resource::<ActiveAnimation>().0.is_none();
            if p_done && a_done {
                break;
            }
            assert!(steps < 100);
        }

        let mut expected = Cube::solved(4).unwrap();
        for m in scramble {
            expected.apply(m).unwrap();
        }
        assert_eq!(app.world().resource::<CubeState>().cube, expected);
    }

    #[test]
    fn move_axis_and_angle_sign_consistent() {
        // R-cw rotates around +X by -90° (WCA "CW from outside" =
        // right-hand rule around the inward direction).
        let m = Move::face(Face::R, Turn::Cw);
        assert_eq!(move_axis_world(m), Vec3::X);
        assert!((move_angle_radians(m) + std::f32::consts::FRAC_PI_2).abs() < 1e-6);

        // L-cw: axis is -X, angle is -π/2 (= +π/2 around +X).
        let m = Move::face(Face::L, Turn::Cw);
        assert_eq!(move_axis_world(m), -Vec3::X);
        assert!((move_angle_radians(m) + std::f32::consts::FRAC_PI_2).abs() < 1e-6);

        // F-cw, half turn.
        let m = Move { face: Face::F, turn: Turn::Half, depth: 1, wide: false };
        assert_eq!(move_axis_world(m), Vec3::Z);
        assert!((move_angle_radians(m) - std::f32::consts::PI).abs() < 1e-6);
    }
}
