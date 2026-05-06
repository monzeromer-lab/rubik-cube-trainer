//! Speedcubing trainer: timer state machine, scramble generator, session
//! statistics with AO5 / AO12 tracking. See plan §8.
//!
//! The pure-logic parts (timer state, AO_n averages, [`SessionStats`])
//! live in the public API and are heavily unit tested. A thin Bevy plugin
//! ([`TrainerPlugin`]) wires them into the app schedule.

use std::time::Duration;

use bevy::prelude::*;
use cube_core::{Cube, MoveSeq};
use cube_render::{CubeState, PendingMoves};
use rand::SeedableRng;
use rand_chacha::ChaCha8Rng;
use serde::{Deserialize, Serialize};

pub mod drill;

/// Standard speedcubing inspection time (15 s, with 8/12 s warnings and
/// optional +2/DNF after 17 s — we keep timing here, leave UI bells for
/// the egui layer).
pub const WCA_INSPECTION_SECS: f32 = 15.0;

/// Outcome flags recorded for a solve.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SolveFlag {
    Ok,
    Plus2,
    Dnf,
}

/// One completed solve.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimedSolve {
    pub scramble: MoveSeq,
    /// Recorded solver moves. Empty if not tracked.
    pub solution: MoveSeq,
    /// Time in milliseconds. For DNF, the partial time is kept for display.
    pub time_ms: u32,
    pub flag: SolveFlag,
}

impl TimedSolve {
    /// Effective time considering the flag. `Plus2` adds 2 s; `Dnf` is
    /// represented as `u32::MAX` so it sorts to the end.
    pub fn effective_time_ms(&self) -> u32 {
        match self.flag {
            SolveFlag::Ok => self.time_ms,
            SolveFlag::Plus2 => self.time_ms.saturating_add(2_000),
            SolveFlag::Dnf => u32::MAX,
        }
    }
}

/// Compute the WCA average-of-N over the most recent solves: discard the
/// fastest and slowest, average the rest. Returns `None` if there aren't
/// enough solves OR if more than one solve in the window is a DNF (a
/// single DNF can still be excluded as the "slowest").
pub fn average_of_n(solves: &[TimedSolve], n: usize) -> Option<u32> {
    if solves.len() < n {
        return None;
    }
    let window = &solves[solves.len() - n..];
    let mut times: Vec<u32> = window.iter().map(|s| s.effective_time_ms()).collect();
    times.sort_unstable();
    // Discard fastest + slowest.
    let mid = &times[1..n - 1];
    let dnf_count = window.iter().filter(|s| matches!(s.flag, SolveFlag::Dnf)).count();
    if dnf_count > 1 {
        return None;
    }
    // If exactly one DNF, it'll have been the "slowest" and dropped.
    let sum: u64 = mid.iter().map(|t| *t as u64).sum();
    Some((sum / mid.len() as u64) as u32)
}

/// Cumulative session stats for a single play session.
#[derive(Resource, Debug, Clone, Default, Serialize, Deserialize)]
pub struct SessionStats {
    pub solves: Vec<TimedSolve>,
}

impl SessionStats {
    pub fn record(&mut self, solve: TimedSolve) {
        self.solves.push(solve);
    }

    /// Best single solve time (ignoring DNFs).
    pub fn best(&self) -> Option<u32> {
        self.solves
            .iter()
            .filter(|s| !matches!(s.flag, SolveFlag::Dnf))
            .map(|s| s.effective_time_ms())
            .min()
    }

    /// Worst single solve time (DNFs count as worse than any time).
    pub fn worst(&self) -> Option<u32> {
        self.solves.iter().map(|s| s.effective_time_ms()).max()
    }

    pub fn ao5(&self) -> Option<u32> {
        average_of_n(&self.solves, 5)
    }

    pub fn ao12(&self) -> Option<u32> {
        average_of_n(&self.solves, 12)
    }

    /// The best AO5 ever recorded in this session (looking at every
    /// rolling 5-solve window).
    pub fn best_ao5(&self) -> Option<u32> {
        if self.solves.len() < 5 {
            return None;
        }
        (0..=self.solves.len() - 5)
            .filter_map(|i| average_of_n(&self.solves[i..i + 5], 5))
            .min()
    }

    pub fn best_ao12(&self) -> Option<u32> {
        if self.solves.len() < 12 {
            return None;
        }
        (0..=self.solves.len() - 12)
            .filter_map(|i| average_of_n(&self.solves[i..i + 12], 12))
            .min()
    }

    pub fn count(&self) -> usize {
        self.solves.len()
    }
}

/// Timer state machine. Strictly monotonic transitions:
/// Idle → Inspecting → Solving → Finished → Idle.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TimerPhase {
    /// Waiting for the user to start (e.g., scramble shown but timer not
    /// yet started).
    Idle,
    /// 15-second WCA inspection countdown is running.
    Inspecting,
    /// Timer is counting up while the user solves.
    Solving,
    /// Solve finished — recorded into stats.
    Finished,
}

#[derive(Resource, Debug, Clone)]
pub struct TimerState {
    pub phase: TimerPhase,
    /// Elapsed time in the current phase.
    pub elapsed: Duration,
    /// Inspection countdown remaining; only meaningful in `Inspecting`.
    pub inspection_remaining: Duration,
    /// The active scramble that this timer's solve is against.
    pub active_scramble: MoveSeq,
}

impl Default for TimerState {
    fn default() -> Self {
        Self {
            phase: TimerPhase::Idle,
            elapsed: Duration::ZERO,
            inspection_remaining: Duration::from_secs_f32(WCA_INSPECTION_SECS),
            active_scramble: MoveSeq::new(),
        }
    }
}

impl TimerState {
    pub fn begin_inspection(&mut self, scramble: MoveSeq) {
        self.phase = TimerPhase::Inspecting;
        self.elapsed = Duration::ZERO;
        self.inspection_remaining = Duration::from_secs_f32(WCA_INSPECTION_SECS);
        self.active_scramble = scramble;
    }

    pub fn begin_solving(&mut self) {
        self.phase = TimerPhase::Solving;
        self.elapsed = Duration::ZERO;
    }

    pub fn finish(&mut self) {
        self.phase = TimerPhase::Finished;
    }

    pub fn reset(&mut self) {
        self.phase = TimerPhase::Idle;
        self.elapsed = Duration::ZERO;
        self.inspection_remaining = Duration::from_secs_f32(WCA_INSPECTION_SECS);
        self.active_scramble = MoveSeq::new();
    }

    /// Advance the timer state by `dt`. Decreases inspection-remaining,
    /// or accumulates solve elapsed, depending on phase. Returns true if
    /// the phase auto-transitioned (e.g., inspection ran out — convert
    /// to `Solving` automatically? plan §8.3 says inspection ends with
    /// any cube move; we leave that decision to the caller and just
    /// track time here.)
    pub fn tick(&mut self, dt: Duration) {
        match self.phase {
            TimerPhase::Inspecting => {
                self.inspection_remaining = self
                    .inspection_remaining
                    .saturating_sub(dt);
                self.elapsed += dt;
            }
            TimerPhase::Solving => {
                self.elapsed += dt;
            }
            TimerPhase::Idle | TimerPhase::Finished => {}
        }
    }
}

/// Resource that owns the deterministic RNG used by the trainer's
/// scramble generator. Inserting your own `TrainerRng` overrides the
/// plugin's default seed.
#[derive(Resource)]
pub struct TrainerRng(pub ChaCha8Rng);

impl Default for TrainerRng {
    fn default() -> Self {
        Self(ChaCha8Rng::seed_from_u64(0xC0FFEE_CAFE_BABE))
    }
}

/// Bevy plugin: registers `TimerState` and `SessionStats`, and runs a
/// `tick_timer` system that advances timer state each frame. The actual
/// **start solve / stop solve** triggers live in the app — typically
/// keyboard- or input-driven — because they tie cube state to timer state.
pub struct TrainerPlugin;

impl Plugin for TrainerPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<TimerState>()
            .init_resource::<SessionStats>()
            .init_resource::<TrainerRng>()
            .init_resource::<drill::PerCaseStats>()
            .add_systems(Update, tick_timer);
    }
}

pub fn tick_timer(time: Res<Time>, mut timer: ResMut<TimerState>) {
    timer.tick(time.delta());
}

/// Helper: enqueue a fresh WCA-style scramble for `cube` and put the
/// timer into the inspection phase. Returns the generated scramble.
pub fn start_solve(
    cube_state: &mut CubeState,
    pending: &mut PendingMoves,
    timer: &mut TimerState,
    rng: &mut TrainerRng,
) -> MoveSeq {
    let size = cube_state.cube.size;
    cube_state.cube = Cube::solved(size).expect("valid size");
    let scramble = cube_core::wca_scramble(size, &mut rng.0);
    pending.0.clear();
    pending.enqueue_all(scramble.iter().copied());
    timer.begin_inspection(scramble.clone());
    scramble
}

#[cfg(test)]
mod tests {
    use super::*;

    fn solve_ms(time_ms: u32) -> TimedSolve {
        TimedSolve {
            scramble: MoveSeq::new(),
            solution: MoveSeq::new(),
            time_ms,
            flag: SolveFlag::Ok,
        }
    }

    fn solve_flag(time_ms: u32, flag: SolveFlag) -> TimedSolve {
        TimedSolve {
            scramble: MoveSeq::new(),
            solution: MoveSeq::new(),
            time_ms,
            flag,
        }
    }

    #[test]
    fn ao5_drops_extremes_and_averages_middle_three() {
        let s = vec![solve_ms(10_000), solve_ms(12_000), solve_ms(14_000), solve_ms(11_000), solve_ms(13_000)];
        // Sorted: 10, 11, 12, 13, 14. Drop 10 and 14, avg of 11+12+13 = 12.
        assert_eq!(average_of_n(&s, 5), Some(12_000));
    }

    #[test]
    fn ao5_treats_plus2_as_added_2000ms() {
        // The +2 affects the effective time used for sorting.
        let s = vec![
            solve_ms(10_000),
            solve_flag(11_000, SolveFlag::Plus2), // effective 13_000
            solve_ms(12_000),
            solve_ms(14_000),
            solve_ms(13_000),
        ];
        // Effective times: 10000, 13000, 12000, 14000, 13000.
        // Sorted: 10000, 12000, 13000, 13000, 14000.
        // Drop 10000 and 14000, avg of 12000+13000+13000 = 12666.
        assert_eq!(average_of_n(&s, 5), Some(12_666));
    }

    #[test]
    fn ao5_one_dnf_is_dropped_as_slowest() {
        let s = vec![
            solve_ms(10_000),
            solve_ms(12_000),
            solve_flag(0, SolveFlag::Dnf),
            solve_ms(11_000),
            solve_ms(13_000),
        ];
        // DNF effective = u32::MAX. Sorted: 10000, 11000, 12000, 13000, MAX.
        // Drop 10000 and MAX. Avg of 11000+12000+13000 = 12000.
        assert_eq!(average_of_n(&s, 5), Some(12_000));
    }

    #[test]
    fn ao5_two_dnfs_returns_none() {
        let s = vec![
            solve_flag(0, SolveFlag::Dnf),
            solve_ms(11_000),
            solve_flag(0, SolveFlag::Dnf),
            solve_ms(12_000),
            solve_ms(13_000),
        ];
        assert_eq!(average_of_n(&s, 5), None);
    }

    #[test]
    fn ao5_returns_none_with_fewer_than_five_solves() {
        let s = vec![solve_ms(10_000), solve_ms(11_000)];
        assert_eq!(average_of_n(&s, 5), None);
    }

    #[test]
    fn session_stats_track_best_and_ao5() {
        let mut stats = SessionStats::default();
        for t in [10_000, 12_000, 14_000, 11_000, 13_000, 9_000] {
            stats.record(solve_ms(t));
        }
        assert_eq!(stats.best(), Some(9_000));
        assert_eq!(stats.count(), 6);
        // Most recent 5 are: 12, 14, 11, 13, 9. Sorted: 9, 11, 12, 13, 14.
        // Drop 9 + 14, avg = 12_000.
        assert_eq!(stats.ao5(), Some(12_000));
        // Best AO5 across all rolling windows of 5.
        // Window 0..5: avg of 11,12,13 = 12_000.
        // Window 1..6: avg of 11,12,13 = 12_000.
        assert_eq!(stats.best_ao5(), Some(12_000));
    }

    #[test]
    fn timer_state_machine_transitions() {
        let mut t = TimerState::default();
        assert_eq!(t.phase, TimerPhase::Idle);
        t.begin_inspection(MoveSeq::new());
        assert_eq!(t.phase, TimerPhase::Inspecting);
        t.tick(Duration::from_secs(5));
        assert_eq!(t.inspection_remaining, Duration::from_secs(10));
        t.begin_solving();
        assert_eq!(t.phase, TimerPhase::Solving);
        t.tick(Duration::from_millis(7_500));
        assert_eq!(t.elapsed, Duration::from_millis(7_500));
        t.finish();
        assert_eq!(t.phase, TimerPhase::Finished);
        t.reset();
        assert_eq!(t.phase, TimerPhase::Idle);
        assert_eq!(t.elapsed, Duration::ZERO);
    }

    #[test]
    fn timer_phases_idle_and_finished_dont_accumulate_elapsed() {
        let mut t = TimerState::default();
        t.tick(Duration::from_secs(10));
        assert_eq!(t.elapsed, Duration::ZERO);
        t.finish();
        let before = t.elapsed;
        t.tick(Duration::from_secs(10));
        assert_eq!(t.elapsed, before);
    }
}
