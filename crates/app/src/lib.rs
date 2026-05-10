//! Library crate behind the desktop binary and the Android cdylib.
//! Exposes [`run_app`] — both entry points (desktop `main.rs` and
//! Android `android_main` below) are thin wrappers over it.
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

use std::path::PathBuf;
use std::time::Duration;

use bevy::prelude::*;
use bevy::tasks::{AsyncComputeTaskPool, Task, block_on, futures_lite::future};
use bevy_panorbit_camera::{PanOrbitCamera, PanOrbitCameraPlugin};
use cube_core::{Face, Move, Turn};
use cube_input::DragInputPlugin;
use cube_render::{
    ActiveAnimation, CubeRenderConfig, CubeRenderPlugin, CubeState, MoveOrigin, NextCommitOrigin,
    PendingMoves,
};
use cube_solver::{Solver2x2, Solver3x3, Solver4x4, Solver5x5};
use cube_trainer::{
    SessionStats, SolveFlag, TimedSolve, TimerPhase, TimerState, TrainerPlugin, TrainerRng,
    start_solve,
};
use cube_trainer::drill::{
    DrillCase, DrillMode, PerCaseStats, pick_drill_case, start_drill, starter_oll_cases,
    starter_pll_cases,
};
use rand::SeedableRng;
use rand_chacha::ChaCha8Rng;

mod hud;
use hud::HudPlugin;

pub fn run_app() {
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
        .add_plugins(HudPlugin)
        .insert_resource(ClearColor(Color::srgb(0.55, 0.55, 0.6)))
        .insert_resource(InputRng(ChaCha8Rng::seed_from_u64(0x00C0_FFEE)))
        .insert_resource(SolverCache::default())
        .insert_resource(DrillSelector::default())
        .init_resource::<SolverBuildState>()
        .init_state::<AppState>()
        .add_systems(Startup, (setup_scene, start_solver_prebuild))
        // Solver prebuild runs regardless of state (the table-build
        // shouldn't pause just because the user hasn't dismissed the
        // menu yet).
        .add_systems(Update, poll_solver_prebuild)
        // Main-menu state: spawn the overlay on enter, despawn on exit,
        // listen for Enter to start.
        .add_systems(OnEnter(AppState::MainMenu), spawn_main_menu)
        .add_systems(OnExit(AppState::MainMenu), despawn_main_menu)
        .add_systems(
            Update,
            main_menu_input.run_if(in_state(AppState::MainMenu)),
        )
        // Gameplay systems only run while playing; HUD displays the
        // cube and stats throughout, so cube rendering (in
        // CubeRenderPlugin) is intentionally not gated.
        .add_systems(
            Update,
            (
                keyboard_size_switch,
                keyboard_to_moves,
                keyboard_solve,
                keyboard_undo_redo,
                trainer_keyboard_flow,
                detect_solve_completion,
                playing_to_menu_input,
            )
                .run_if(in_state(AppState::Playing)),
        )
        .run();
}

/// Keyboard shortcuts for the trainer flow:
/// - `T`: start a new timed solve in the current drill mode.
/// - `M`: cycle drill mode (SpeedSolve ↔ OLL).
/// - `Enter`: end inspection / begin solve (only valid in `Inspecting`).
/// - `Escape`: abandon current solve, reset to idle.
fn trainer_keyboard_flow(
    keys: Res<ButtonInput<KeyCode>>,
    mut timer: ResMut<TimerState>,
    mut state: ResMut<CubeState>,
    mut pending: ResMut<PendingMoves>,
    mut rng: ResMut<TrainerRng>,
    mut selector: ResMut<DrillSelector>,
    per_case: Res<PerCaseStats>,
) {
    if keys.just_pressed(KeyCode::KeyM) {
        selector.mode = next_drill_mode(selector.mode);
        selector.current = None;
        info!("trainer: mode = {}", selector.mode.label());
    }
    if keys.just_pressed(KeyCode::KeyT) {
        match selector.mode {
            DrillMode::SpeedSolve => {
                let scr = start_solve(&mut state, &mut pending, &mut timer, &mut rng);
                selector.current = None;
                info!(
                    "trainer: scramble started ({} moves) — inspection running",
                    scr.len()
                );
            }
            DrillMode::Oll => {
                let cases = starter_oll_cases();
                if let Some(case) = pick_drill_case(&cases, &per_case, &mut rng.0) {
                    let case = case.clone();
                    start_drill(&case, &mut state, &mut pending, &mut timer);
                    info!(
                        "trainer: drill OLL — {} (alg: {})",
                        case.display_name,
                        case.canonical_algorithm.as_deref().unwrap_or("?")
                    );
                    selector.current = Some(case);
                }
            }
            DrillMode::Pll => {
                let cases = starter_pll_cases();
                if let Some(case) = pick_drill_case(&cases, &per_case, &mut rng.0) {
                    let case = case.clone();
                    start_drill(&case, &mut state, &mut pending, &mut timer);
                    info!(
                        "trainer: drill PLL — {} (alg: {})",
                        case.display_name,
                        case.canonical_algorithm.as_deref().unwrap_or("?")
                    );
                    selector.current = Some(case);
                }
            }
            other => {
                warn!(
                    "trainer: drill mode {} has no case library yet",
                    other.label()
                );
                selector.current = None;
            }
        }
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
        selector.current = None;
        info!("trainer: solve abandoned");
    }
}

/// `Ctrl+Z` undoes the most recent committed move; `Ctrl+Y` (or
/// `Ctrl+Shift+Z`) redoes one. Gated on an empty animation queue so the
/// commit-time `NextCommitOrigin` is unambiguous: at most one move in
/// flight at a time, with a known origin.
fn keyboard_undo_redo(
    keys: Res<ButtonInput<KeyCode>>,
    mut state: ResMut<CubeState>,
    mut pending: ResMut<PendingMoves>,
    active: Res<ActiveAnimation>,
    mut origin: ResMut<NextCommitOrigin>,
) {
    let ctrl = keys.pressed(KeyCode::ControlLeft) || keys.pressed(KeyCode::ControlRight);
    if !ctrl {
        return;
    }
    if !pending.is_empty() || active.0.is_some() {
        return;
    }
    let shift = keys.pressed(KeyCode::ShiftLeft) || keys.pressed(KeyCode::ShiftRight);
    let want_redo = keys.just_pressed(KeyCode::KeyY)
        || (keys.just_pressed(KeyCode::KeyZ) && shift);
    let want_undo = keys.just_pressed(KeyCode::KeyZ) && !shift;

    if want_undo {
        if let Some(m) = state.history.pop_back() {
            state.redo_stack.push_back(m);
            origin.0 = MoveOrigin::Undo;
            pending.enqueue(m.inverse());
            info!(
                "undo (history={}, redo={})",
                state.history.len(),
                state.redo_stack.len()
            );
        }
    } else if want_redo {
        if let Some(m) = state.redo_stack.pop_back() {
            state.history.push_back(m);
            origin.0 = MoveOrigin::Redo;
            pending.enqueue(m);
            info!(
                "redo (history={}, redo={})",
                state.history.len(),
                state.redo_stack.len()
            );
        }
    }
}

/// Cycle through the drill modes that have case libraries today. F2L/
/// LastLayer/Cross are listed in [`DrillMode`] but their case sets are
/// future content work, so they're skipped here rather than presented as
/// dead-end picks. PLL covers only the three algorithms that round-trip
/// cleanly against the current simulator (Aa, H, Ua) — the remaining 18
/// PLLs are gated on the alg-library verification pass (plan §7.4).
fn next_drill_mode(current: DrillMode) -> DrillMode {
    match current {
        DrillMode::SpeedSolve => DrillMode::Oll,
        DrillMode::Oll => DrillMode::Pll,
        _ => DrillMode::SpeedSolve,
    }
}

/// While solving, watch for the cube becoming solved → record the solve.
fn detect_solve_completion(
    cube_state: Res<CubeState>,
    active: Res<ActiveAnimation>,
    mut timer: ResMut<TimerState>,
    mut stats: ResMut<SessionStats>,
    mut per_case: ResMut<PerCaseStats>,
    mut selector: ResMut<DrillSelector>,
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
        if let Some(case) = selector.current.take() {
            per_case.record(&case.id, elapsed_ms);
            info!(
                "drill: {} = {} ms (n={}, avg={:?})",
                case.id,
                elapsed_ms,
                per_case.count(&case.id),
                per_case.average(&case.id),
            );
        }
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

/// Top-level app state — plan §5.1. Only the two essential states are
/// modelled today (Trainer / Guide / SolutionPlayback / Settings /
/// StickerInput will get their own variants as those screens land).
/// `Playing` covers free-cube + speed-solve + drill flows; the menu is
/// just a launchpad that gates input until the user dismisses it.
#[derive(States, Default, Debug, Clone, PartialEq, Eq, Hash)]
enum AppState {
    #[default]
    MainMenu,
    Playing,
}

/// Marker for every entity that belongs to the main-menu overlay so
/// `OnExit(MainMenu)` can despawn the whole tree in one query.
#[derive(Component)]
struct MainMenuRoot;

fn spawn_main_menu(mut commands: Commands) {
    commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                top: Val::Px(0.0),
                left: Val::Px(0.0),
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                flex_direction: FlexDirection::Column,
                ..default()
            },
            BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.78)),
            MainMenuRoot,
        ))
        .with_children(|p| {
            p.spawn((
                Text::new("Rubik's Trainer"),
                TextFont { font_size: 56.0, ..default() },
                TextColor(Color::srgb(1.0, 0.95, 0.6)),
            ));
            p.spawn((
                Text::new("\nPress Enter to start"),
                TextFont { font_size: 22.0, ..default() },
                TextColor(Color::srgb(0.85, 0.85, 0.9)),
            ));
            p.spawn((
                Text::new("\nF1 returns here from anywhere"),
                TextFont { font_size: 14.0, ..default() },
                TextColor(Color::srgb(0.55, 0.55, 0.6)),
            ));
        });
}

fn despawn_main_menu(mut commands: Commands, q: Query<Entity, With<MainMenuRoot>>) {
    for e in &q {
        commands.entity(e).despawn();
    }
}

fn main_menu_input(
    keys: Res<ButtonInput<KeyCode>>,
    mut next: ResMut<NextState<AppState>>,
) {
    if keys.just_pressed(KeyCode::Enter) {
        next.set(AppState::Playing);
    }
}

fn playing_to_menu_input(
    keys: Res<ButtonInput<KeyCode>>,
    mut next: ResMut<NextState<AppState>>,
) {
    if keys.just_pressed(KeyCode::F1) {
        next.set(AppState::MainMenu);
    }
}

/// Active drill mode + the case currently being drilled. `current` is
/// `Some` from the moment a drill case is dealt until completion records
/// it into [`PerCaseStats`] (or the user abandons the solve).
#[derive(Resource)]
struct DrillSelector {
    mode: DrillMode,
    current: Option<DrillCase>,
}

impl Default for DrillSelector {
    fn default() -> Self {
        Self {
            mode: DrillMode::SpeedSolve,
            current: None,
        }
    }
}

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

/// Background-builds the 3×3 solver at app startup so the user doesn't
/// hit the table-build pause on first `S` press. With the disk cache
/// populated, this is a fast file-read; on first ever launch (cache
/// miss), the build runs in a worker thread off the render thread, so
/// the window stays responsive throughout.
#[derive(Resource, Default)]
struct SolverBuildState {
    task: Option<Task<Solver3x3>>,
    pub ready: bool,
}

fn start_solver_prebuild(mut state: ResMut<SolverBuildState>) {
    let pool = AsyncComputeTaskPool::get();
    let path = solver_cache_path();
    let task = pool.spawn(async move { Solver3x3::new_with_cache(&path) });
    state.task = Some(task);
    info!("solver: 3×3 prebuild kicked off in the background");
}

fn poll_solver_prebuild(
    mut state: ResMut<SolverBuildState>,
    mut cache: ResMut<SolverCache>,
) {
    if state.ready {
        return;
    }
    let Some(task) = state.task.as_mut() else {
        return;
    };
    if let Some(solver) = block_on(future::poll_once(task)) {
        cache.three = Some(solver);
        state.task = None;
        state.ready = true;
        info!("solver: 3×3 ready");
    }
}

/// Filesystem location for the 3×3 solver's pruning-table cache. Honours
/// `$RUBIKS_CACHE_DIR` (override) → `$XDG_CACHE_HOME/rubiks-trainer` →
/// `$HOME/.cache/rubiks-trainer` → `./.rubiks_cache` (cwd fallback).
/// Filename is versioned in [`cube_solver::three::cache::CACHE_VERSION`]
/// so a stale file from an incompatible build is cheaply rejected on
/// load and rewritten — but the file *name* stays stable across versions
/// because the format itself carries the version field.
fn solver_cache_path() -> PathBuf {
    solver_cache_dir().join("3x3_pruning.bin")
}

fn solver_2x2_cache_path() -> PathBuf {
    solver_cache_dir().join("2x2_distance.bin")
}

fn solver_cache_dir() -> PathBuf {
    if let Some(d) = std::env::var_os("RUBIKS_CACHE_DIR") {
        PathBuf::from(d)
    } else if let Some(d) = std::env::var_os("XDG_CACHE_HOME") {
        PathBuf::from(d).join("rubiks-trainer")
    } else if let Some(h) = std::env::var_os("HOME") {
        PathBuf::from(h).join(".cache").join("rubiks-trainer")
    } else {
        PathBuf::from(".rubiks_cache")
    }
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
                let path = solver_2x2_cache_path();
                info!(
                    "solver: loading/building 2×2 distance table (cache: {})",
                    path.display()
                );
                cache.two = Some(Solver2x2::new_with_cache(&path));
            }
            cache.two.as_ref().unwrap().solve(cube).map_err(|e| e.to_string())
        }
        3 => {
            if cache.three.is_none() {
                // Prebuild hasn't finished yet — bail with a friendly
                // message rather than blocking the render thread.
                warn!(
                    "solver: 3×3 still building tables in the background; try again in a second"
                );
                return;
            }
            cache.three.as_ref().unwrap().solve(cube).map_err(|e| e.to_string())
        }
        4 => {
            if cache.four.is_none() {
                let path = solver_cache_path();
                info!(
                    "solver: loading/building 4×4 (inner 3×3 cache: {})",
                    path.display()
                );
                cache.four = Some(Solver4x4::new_with_cache(&path));
            }
            cache.four.as_ref().unwrap().solve(cube).map_err(|e| e.to_string())
        }
        5 => {
            if cache.five.is_none() {
                let path = solver_cache_path();
                info!(
                    "solver: loading/building 5×5 (inner 3×3 cache: {})",
                    path.display()
                );
                cache.five = Some(Solver5x5::new_with_cache(&path));
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
        state.reset_history();
    }

    // Avoid 'unused' lint when running headless tests.
    let _ = Duration::from_millis(0);
}

/// Android entry point. `cargo-apk` looks for an `android_main` C
/// symbol in the final `cdylib`; the `#[bevy_main]` proc macro emits
/// that symbol on `target_os = "android"` (and is a no-op everywhere
/// else, so the desktop bin is untouched).
#[cfg(target_os = "android")]
#[bevy::prelude::bevy_main]
fn main() {
    run_app();
}
