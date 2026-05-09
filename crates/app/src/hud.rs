//! On-screen HUD: timer, scramble, session stats, and a control cheatsheet.
//!
//! Everything the player needs to see while training is rendered here as
//! Bevy UI text. Backend state lives in [`cube_trainer::TimerState`],
//! [`cube_trainer::SessionStats`], and [`cube_render::CubeRenderConfig`]
//! — this module just observes those resources and refreshes labels.

use std::time::Duration;

use bevy::prelude::*;
use cube_render::{CubeRenderConfig, PendingMoves};
use cube_trainer::{SessionStats, TimerPhase, TimerState};

pub struct HudPlugin;

impl Plugin for HudPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, spawn_hud).add_systems(
            Update,
            (
                update_timer,
                update_phase_size,
                update_scramble,
                update_stats,
                update_solver_hint,
            ),
        );
    }
}

#[derive(Component)]
struct HudTimerText;

#[derive(Component)]
struct HudPhaseSizeText;

#[derive(Component)]
struct HudScrambleText;

#[derive(Component)]
struct HudStatsText;

#[derive(Component)]
struct HudSolverHintText;

const PANEL_BG: Color = Color::srgba(0.0, 0.0, 0.0, 0.55);
const ACCENT: Color = Color::srgb(0.9, 0.85, 0.4);
const DIM: Color = Color::srgb(0.75, 0.75, 0.78);

fn spawn_hud(mut commands: Commands) {
    // Top-left: cube size + timer phase indicator.
    commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                top: Val::Px(12.0),
                left: Val::Px(12.0),
                padding: UiRect::all(Val::Px(10.0)),
                ..default()
            },
            BackgroundColor(PANEL_BG),
        ))
        .with_children(|p| {
            p.spawn((
                Text::new("Cube 3×3 — Idle"),
                TextFont { font_size: 16.0, ..default() },
                TextColor(DIM),
                HudPhaseSizeText,
            ));
        });

    // Top-center: the timer. The headline element of the trainer.
    commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                top: Val::Px(12.0),
                left: Val::Percent(50.0),
                margin: UiRect {
                    left: Val::Px(-110.0),
                    ..default()
                },
                width: Val::Px(220.0),
                padding: UiRect::all(Val::Px(10.0)),
                justify_content: JustifyContent::Center,
                ..default()
            },
            BackgroundColor(PANEL_BG),
        ))
        .with_children(|p| {
            p.spawn((
                Text::new("READY"),
                TextFont { font_size: 36.0, ..default() },
                TextColor(ACCENT),
                HudTimerText,
            ));
        });

    // Top-right: session stats.
    commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                top: Val::Px(12.0),
                right: Val::Px(12.0),
                padding: UiRect::all(Val::Px(10.0)),
                ..default()
            },
            BackgroundColor(PANEL_BG),
        ))
        .with_children(|p| {
            p.spawn((
                Text::new("Solves 0   Best —   AO5 —   AO12 —"),
                TextFont { font_size: 16.0, ..default() },
                TextColor(DIM),
                HudStatsText,
            ));
        });

    // Below the timer: the active scramble (only while one is queued / running).
    commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                top: Val::Px(86.0),
                left: Val::Percent(50.0),
                margin: UiRect {
                    left: Val::Px(-260.0),
                    ..default()
                },
                width: Val::Px(520.0),
                padding: UiRect::all(Val::Px(8.0)),
                justify_content: JustifyContent::Center,
                ..default()
            },
            BackgroundColor(PANEL_BG),
        ))
        .with_children(|p| {
            p.spawn((
                Text::new(""),
                TextFont { font_size: 18.0, ..default() },
                TextColor(Color::srgb(0.95, 0.95, 0.98)),
                HudScrambleText,
            ));
        });

    // Bottom-left: controls cheatsheet. Static text — no update system.
    commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                bottom: Val::Px(12.0),
                left: Val::Px(12.0),
                padding: UiRect::all(Val::Px(10.0)),
                flex_direction: FlexDirection::Column,
                row_gap: Val::Px(2.0),
                ..default()
            },
            BackgroundColor(PANEL_BG),
        ))
        .with_children(|p| {
            for line in CONTROLS_HELP {
                p.spawn((
                    Text::new(*line),
                    TextFont { font_size: 13.0, ..default() },
                    TextColor(DIM),
                ));
            }
        });

    // Bottom-right: transient solver/queue hint (e.g. "Solving…", "Queue: 14").
    commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                bottom: Val::Px(12.0),
                right: Val::Px(12.0),
                padding: UiRect::all(Val::Px(10.0)),
                ..default()
            },
            BackgroundColor(PANEL_BG),
        ))
        .with_children(|p| {
            p.spawn((
                Text::new(""),
                TextFont { font_size: 14.0, ..default() },
                TextColor(DIM),
                HudSolverHintText,
            ));
        });
}

const CONTROLS_HELP: &[&str] = &[
    "T  scramble + inspect       Enter  begin solve       Esc  abandon",
    "U/D/L/R/F/B  face turn    Shift+letter  inverse    Alt+letter  half",
    "Space  random scramble    S  auto-solve            Backspace  reset",
    "2/3/4/5  cube size         Left-drag  turn face    Right-drag  orbit",
];

fn format_ms(ms: u32) -> String {
    let total_ms = ms as u64;
    let m = total_ms / 60_000;
    let s = (total_ms % 60_000) / 1000;
    let frac = total_ms % 1000;
    if m > 0 {
        format!("{m}:{s:02}.{frac:03}")
    } else {
        format!("{s}.{frac:03}")
    }
}

fn duration_ms(d: Duration) -> u32 {
    d.as_millis().min(u32::MAX as u128) as u32
}

fn update_timer(
    timer: Res<TimerState>,
    mut q: Query<(&mut Text, &mut TextColor), With<HudTimerText>>,
) {
    let Ok((mut text, mut color)) = q.single_mut() else { return };
    let (label, c) = match timer.phase {
        TimerPhase::Idle => {
            if timer.active_scramble.is_empty() {
                ("READY".to_string(), ACCENT)
            } else {
                ("SCRAMBLED".to_string(), ACCENT)
            }
        }
        TimerPhase::Inspecting => {
            let secs = timer.inspection_remaining.as_secs_f32().max(0.0);
            // Color shift as inspection runs out: yellow → orange → red.
            let warn = if secs <= 3.0 {
                Color::srgb(1.0, 0.4, 0.4)
            } else if secs <= 8.0 {
                Color::srgb(1.0, 0.7, 0.3)
            } else {
                ACCENT
            };
            (format!("INSPECT {secs:>4.1}"), warn)
        }
        TimerPhase::Solving => (
            format_ms(duration_ms(timer.elapsed)),
            Color::srgb(0.6, 1.0, 0.7),
        ),
        TimerPhase::Finished => (
            format_ms(duration_ms(timer.elapsed)),
            Color::srgb(0.7, 0.85, 1.0),
        ),
    };
    if text.0 != label {
        text.0 = label;
    }
    if color.0 != c {
        color.0 = c;
    }
}

fn update_phase_size(
    timer: Res<TimerState>,
    config: Res<CubeRenderConfig>,
    mut q: Query<&mut Text, With<HudPhaseSizeText>>,
) {
    let Ok(mut text) = q.single_mut() else { return };
    let phase = match timer.phase {
        TimerPhase::Idle => "Idle",
        TimerPhase::Inspecting => "Inspecting",
        TimerPhase::Solving => "Solving",
        TimerPhase::Finished => "Finished",
    };
    let s = config.size;
    let label = format!("Cube {s}×{s} — {phase}");
    if text.0 != label {
        text.0 = label;
    }
}

fn update_scramble(
    timer: Res<TimerState>,
    mut q: Query<&mut Text, With<HudScrambleText>>,
) {
    let Ok(mut text) = q.single_mut() else { return };
    // Only show during inspection / solve. Once finished, hide it so the
    // timer reading is uncluttered.
    let label = match timer.phase {
        TimerPhase::Inspecting | TimerPhase::Solving => {
            if timer.active_scramble.is_empty() {
                String::new()
            } else {
                timer.active_scramble.to_string()
            }
        }
        _ => String::new(),
    };
    if text.0 != label {
        text.0 = label;
    }
}

fn update_stats(
    stats: Res<SessionStats>,
    mut q: Query<&mut Text, With<HudStatsText>>,
) {
    let Ok(mut text) = q.single_mut() else { return };
    let fmt = |o: Option<u32>| match o {
        Some(ms) => format_ms(ms),
        None => "—".to_string(),
    };
    let label = format!(
        "Solves {}   Best {}   AO5 {}   AO12 {}",
        stats.count(),
        fmt(stats.best()),
        fmt(stats.ao5()),
        fmt(stats.ao12())
    );
    if text.0 != label {
        text.0 = label;
    }
}

fn update_solver_hint(
    pending: Res<PendingMoves>,
    mut q: Query<&mut Text, With<HudSolverHintText>>,
) {
    let Ok(mut text) = q.single_mut() else { return };
    let queued = pending.0.len();
    let label = if queued > 0 {
        format!("Queue: {queued} move{}", if queued == 1 { "" } else { "s" })
    } else {
        String::new()
    };
    if text.0 != label {
        text.0 = label;
    }
}
