//! Binary that opens a window, sets up lighting and an orbit camera, and
//! renders a Rubik's cube via the [`cube_render::CubeRenderPlugin`].
//!
//! Keyboard (M5):
//! - U/D/L/R/F/B: clockwise quarter-turn of that face.
//! - Shift+letter: counter-clockwise.
//! - Alt+letter (or Caps): half-turn.
//! - Space: enqueue 20 random moves (a quick scramble).
//! - Backspace: clear the move queue and reset to solved.
//! - S (M13): solve the current cube using the size-appropriate solver
//!   (2×2/3×3/4×4). First press builds tables and may pause the UI for a
//!   few seconds; subsequent presses are instant.

use std::time::Duration;

use bevy::prelude::*;
use bevy_panorbit_camera::{PanOrbitCamera, PanOrbitCameraPlugin};
use cube_core::{Face, Move, Turn};
use cube_input::DragInputPlugin;
use cube_render::{ActiveAnimation, CubeRenderConfig, CubeRenderPlugin, CubeState, PendingMoves};
use cube_solver::{Solver2x2, Solver3x3, Solver4x4, Solver5x5};
use cube_trainer::{
    SessionStats, SolveFlag, TimedSolve, TimerPhase, TimerState, TrainerPlugin, TrainerRng,
    start_solve,
};
use rand::SeedableRng;
use rand_chacha::ChaCha8Rng;

fn main() {
    App::new()
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window {
                title: "Rubik's Trainer".into(),
                resolution: (1280u32, 800u32).into(),
                ..default()
            }),
            ..default()
        }))
        .add_plugins(PanOrbitCameraPlugin)
        .add_plugins(CubeRenderPlugin)
        .add_plugins(DragInputPlugin)
        .add_plugins(TrainerPlugin)
        .insert_resource(ClearColor(Color::srgb(0.55, 0.55, 0.6)))
        .insert_resource(InputRng(ChaCha8Rng::seed_from_u64(0x00C0_FFEE)))
        .insert_resource(SolverCache::default())
        .add_systems(Startup, setup_scene)
        .add_systems(
            Update,
            (
                keyboard_size_switch,
                keyboard_to_moves,
                keyboard_solve,
                trainer_keyboard_flow,
                detect_solve_completion,
            ),
        )
        .run();
}

/// Keyboard shortcuts for the trainer flow:
/// - `T`: start a new timed solve (scrambles the cube + begins inspection).
/// - `Enter`: end inspection / begin solve (only valid in `Inspecting`).
/// - `Escape`: abandon current solve, reset to idle.
fn trainer_keyboard_flow(
    keys: Res<ButtonInput<KeyCode>>,
    mut timer: ResMut<TimerState>,
    mut state: ResMut<CubeState>,
    mut pending: ResMut<PendingMoves>,
    mut rng: ResMut<TrainerRng>,
) {
    if keys.just_pressed(KeyCode::KeyT) {
        let scr = start_solve(&mut state, &mut pending, &mut timer, &mut rng);
        info!("trainer: scramble started ({} moves) — inspection running", scr.len());
    }
    if keys.just_pressed(KeyCode::Enter) {
        if timer.phase == TimerPhase::Inspecting {
            timer.begin_solving();
            info!("trainer: solve started — drag stickers to solve, watch terminal for time");
        } else {
            info!(
                "trainer: Enter ignored, current phase = {:?} (press T first to scramble)",
                timer.phase
            );
        }
    }
    if keys.just_pressed(KeyCode::Escape) {
        timer.reset();
        info!("trainer: solve abandoned");
    }
}

/// While solving, watch for the cube becoming solved → record the solve.
fn detect_solve_completion(
    cube_state: Res<CubeState>,
    active: Res<ActiveAnimation>,
    mut timer: ResMut<TimerState>,
    mut stats: ResMut<SessionStats>,
) {
    if timer.phase != TimerPhase::Solving {
        return;
    }
    // Only check when no animation is mid-flight; logical state is then
    // canonical.
    if active.0.is_some() {
        return;
    }
    if cube_state.cube.is_solved() {
        let elapsed_ms = timer.elapsed.as_millis().min(u32::MAX as u128) as u32;
        stats.record(TimedSolve {
            scramble: timer.active_scramble.clone(),
            solution: cube_core::MoveSeq::new(),
            time_ms: elapsed_ms,
            flag: SolveFlag::Ok,
        });
        timer.finish();
        info!(
            "Solve recorded: {} ms (count={}, best={:?}, ao5={:?})",
            elapsed_ms,
            stats.count(),
            stats.best(),
            stats.ao5(),
        );
    }
}

fn keyboard_size_switch(
    keys: Res<ButtonInput<KeyCode>>,
    mut config: ResMut<CubeRenderConfig>,
) {
    let new_size = if keys.just_pressed(KeyCode::Digit2) {
        Some(2)
    } else if keys.just_pressed(KeyCode::Digit3) {
        Some(3)
    } else if keys.just_pressed(KeyCode::Digit4) {
        Some(4)
    } else if keys.just_pressed(KeyCode::Digit5) {
        Some(5)
    } else {
        None
    };
    if let Some(size) = new_size
        && size != config.size
    {
        config.size = size;
    }
}

fn setup_scene(mut commands: Commands) {
    // Three-light setup for a "studio shoot" feel — see plan §10.4.
    commands.spawn((
        DirectionalLight {
            illuminance: 18_000.0,
            color: Color::srgb(1.0, 0.96, 0.92),
            shadows_enabled: true,
            ..default()
        },
        Transform::from_xyz(4.0, 8.0, 6.0).looking_at(Vec3::ZERO, Vec3::Y),
    ));
    commands.spawn((
        DirectionalLight {
            illuminance: 8_000.0,
            color: Color::srgb(0.85, 0.9, 1.0),
            shadows_enabled: false,
            ..default()
        },
        Transform::from_xyz(-6.0, 3.0, -4.0).looking_at(Vec3::ZERO, Vec3::Y),
    ));
    commands.spawn((
        DirectionalLight {
            illuminance: 4_000.0,
            color: Color::srgb(1.0, 0.95, 0.8),
            shadows_enabled: false,
            ..default()
        },
        Transform::from_xyz(0.0, -2.0, -7.0).looking_at(Vec3::ZERO, Vec3::Y),
    ));

    commands.spawn((
        Camera3d::default(),
        Transform::from_xyz(4.0, 4.0, 6.0).looking_at(Vec3::ZERO, Vec3::Y),
        PanOrbitCamera {
            focus: Vec3::ZERO,
            radius: Some(7.0),
            // Left-click is reserved for dragging stickers (M6). Orbit
            // moves to the right mouse button; pan to middle-click.
            button_orbit: MouseButton::Right,
            button_pan: MouseButton::Middle,
            ..default()
        },
    ));
}

#[derive(Resource)]
struct InputRng(ChaCha8Rng);

/// Lazily-built solvers per size. Each is `None` until the user first
/// presses `S` for a cube of that size — we don't pay the table-build
/// cost (a few seconds for 3×3) at startup.
#[derive(Resource, Default)]
struct SolverCache {
    two: Option<Solver2x2>,
    three: Option<Solver3x3>,
    four: Option<Solver4x4>,
    five: Option<Solver5x5>,
}

/// `S`: solve the current cube. Picks the appropriate solver for the
/// current size, building it on first press. Solution moves are pushed to
/// [`PendingMoves`] so the renderer animates each turn.
fn keyboard_solve(
    keys: Res<ButtonInput<KeyCode>>,
    state: Res<CubeState>,
    mut pending: ResMut<PendingMoves>,
    mut cache: ResMut<SolverCache>,
) {
    if !keys.just_pressed(KeyCode::KeyS) {
        return;
    }
    let cube = &state.cube;
    let solution: Result<cube_core::MoveSeq, String> = match cube.size {
        2 => {
            if cache.two.is_none() {
                info!("solver: building 2×2 distance table (one-time, ~1–2s)");
                cache.two = Some(Solver2x2::new());
            }
            cache.two.as_ref().unwrap().solve(cube).map_err(|e| e.to_string())
        }
        3 => {
            if cache.three.is_none() {
                info!("solver: building 3×3 pruning tables (one-time, ~few seconds)");
                cache.three = Some(Solver3x3::new());
            }
            cache.three.as_ref().unwrap().solve(cube).map_err(|e| e.to_string())
        }
        4 => {
            if cache.four.is_none() {
                info!("solver: building 4×4 (incl. inner 3×3 tables, one-time, ~few seconds)");
                cache.four = Some(Solver4x4::new());
            }
            cache.four.as_ref().unwrap().solve(cube).map_err(|e| e.to_string())
        }
        5 => {
            if cache.five.is_none() {
                info!("solver: building 5×5 (incl. inner 3×3 tables, one-time, ~few seconds)");
                cache.five = Some(Solver5x5::new());
            }
            cache.five.as_ref().unwrap().solve(cube).map_err(|e| e.to_string())
        }
        n => Err(format!("no solver for size {n}")),
    };
    match solution {
        Ok(seq) => {
            info!("solver: {} moves — {}", seq.len(), seq);
            pending.enqueue_all(seq);
        }
        Err(e) => warn!("solver: {}", e),
    }
}

fn keyboard_to_moves(
    keys: Res<ButtonInput<KeyCode>>,
    mut pending: ResMut<PendingMoves>,
    mut state: ResMut<CubeState>,
    mut rng: ResMut<InputRng>,
) {
    let face_keys = [
        (KeyCode::KeyU, Face::U),
        (KeyCode::KeyD, Face::D),
        (KeyCode::KeyL, Face::L),
        (KeyCode::KeyR, Face::R),
        (KeyCode::KeyF, Face::F),
        (KeyCode::KeyB, Face::B),
    ];

    let shift = keys.pressed(KeyCode::ShiftLeft) || keys.pressed(KeyCode::ShiftRight);
    let alt = keys.pressed(KeyCode::AltLeft) || keys.pressed(KeyCode::AltRight);
    let turn = if alt {
        Turn::Half
    } else if shift {
        Turn::Ccw
    } else {
        Turn::Cw
    };

    for (key, face) in face_keys {
        if keys.just_pressed(key) {
            pending.enqueue(Move::face(face, turn));
        }
    }

    if keys.just_pressed(KeyCode::Space) {
        // Enqueue a 20-move random scramble. random_move_scramble respects
        // size — for a 3×3 it uses the 6 face quarter/half/triple-quarter
        // alphabet.
        let size = state.cube.size;
        let scramble = cube_core::random_move_scramble(size, 20, &mut rng.0);
        pending.enqueue_all(scramble);
    }

    if keys.just_pressed(KeyCode::Backspace) {
        pending.0.clear();
        state.cube = cube_core::Cube::solved(state.cube.size).unwrap();
    }

    // Avoid 'unused' lint when running headless tests.
    let _ = Duration::from_millis(0);
}
