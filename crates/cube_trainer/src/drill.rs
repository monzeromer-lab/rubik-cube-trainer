//! Drill modes (plan §8.1) and per-case statistics (§8.4–§8.5).
//!
//! A drill case is a named pre-solve state plus an optional canonical
//! algorithm. The trainer applies the case's `setup` move sequence to a
//! solved cube — that puts the cube in the case's recognition state.
//! The user then executes the algorithm; when the cube returns to solved
//! the per-case timer fires and the result is recorded into
//! [`PerCaseStats`].
//!
//! Full case libraries (57 OLL, 21 PLL, 41 F2L) are M11/M12 content work.
//! This module ships the data model plus a small starter set so the
//! flow is testable and demoable today.

use std::collections::HashMap;

use bevy::prelude::*;
use cube_core::{Move, MoveSeq};
use rand::Rng;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum DrillMode {
    SpeedSolve,
    Oll,
    Pll,
    F2l,
    /// Combined OLL + PLL after F2L is complete.
    LastLayer,
    Cross,
}

impl DrillMode {
    pub fn label(&self) -> &'static str {
        match self {
            DrillMode::SpeedSolve => "speed-solve",
            DrillMode::Oll => "OLL",
            DrillMode::Pll => "PLL",
            DrillMode::F2l => "F2L",
            DrillMode::LastLayer => "last-layer",
            DrillMode::Cross => "cross",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DrillCase {
    /// Unique stable identifier, e.g., `"oll-21-cross"` or `"pll-Aa"`.
    /// Use this as the key in [`PerCaseStats`].
    pub id: String,
    pub mode: DrillMode,
    /// Friendly name for display in the heatmap.
    pub display_name: String,
    /// Move sequence applied from solved to put the cube in this case's
    /// recognition state. The user's task is to recognise + solve it.
    pub setup: MoveSeq,
    /// One known-good algorithm. Stored as text in case the user wants a
    /// hint; not required for the drill loop.
    pub canonical_algorithm: Option<String>,
}

/// Per-case statistics: every solve attempt's wall-clock time, keyed by
/// case id.
#[derive(Resource, Debug, Clone, Default, Serialize, Deserialize)]
pub struct PerCaseStats {
    pub by_case: HashMap<String, Vec<u32>>,
}

impl PerCaseStats {
    pub fn record(&mut self, case_id: &str, time_ms: u32) {
        self.by_case
            .entry(case_id.to_string())
            .or_default()
            .push(time_ms);
    }

    pub fn count(&self, case_id: &str) -> usize {
        self.by_case.get(case_id).map(|v| v.len()).unwrap_or(0)
    }

    pub fn average(&self, case_id: &str) -> Option<u32> {
        let times = self.by_case.get(case_id)?;
        if times.is_empty() {
            return None;
        }
        let sum: u64 = times.iter().map(|t| *t as u64).sum();
        Some((sum / times.len() as u64) as u32)
    }

    pub fn best(&self, case_id: &str) -> Option<u32> {
        self.by_case.get(case_id)?.iter().copied().min()
    }

    pub fn worst(&self, case_id: &str) -> Option<u32> {
        self.by_case.get(case_id)?.iter().copied().max()
    }

    /// Order cases by "weakness": untried cases first (highest drill
    /// priority), then cases whose average is slowest. The trainer uses
    /// this to bias drill picks toward where you're slow — the heatmap.
    pub fn weakness_ranking<'a>(&self, candidates: &'a [DrillCase]) -> Vec<&'a DrillCase> {
        let mut out: Vec<&'a DrillCase> = candidates.iter().collect();
        out.sort_by_key(|c| {
            // Stable, total order: untried (None) sorts before tried.
            // Among tried, slower average sorts first.
            match self.average(&c.id) {
                None => (0u8, 0u32),
                Some(avg) => (1u8, u32::MAX - avg),
            }
        });
        out
    }
}

/// Pick a drill case with bias toward weak ones. Untried cases are picked
/// first; among tried cases, slower averages are weighted more heavily.
/// Returns `None` if `candidates` is empty.
pub fn pick_drill_case<'a, R: Rng + ?Sized>(
    candidates: &'a [DrillCase],
    stats: &PerCaseStats,
    rng: &mut R,
) -> Option<&'a DrillCase> {
    if candidates.is_empty() {
        return None;
    }
    // Prefer untried cases — drill them first.
    let untried: Vec<&DrillCase> = candidates
        .iter()
        .filter(|c| stats.count(&c.id) == 0)
        .collect();
    if !untried.is_empty() {
        let idx = rng.random_range(0..untried.len());
        return Some(untried[idx]);
    }
    // Weight by `average_ms`: slower → more weight. Use a square so the
    // slowest case is meaningfully more likely without being deterministic.
    let weights: Vec<u64> = candidates
        .iter()
        .map(|c| {
            let avg = stats.average(&c.id).unwrap_or(1);
            (avg as u64).saturating_mul(avg as u64)
        })
        .collect();
    let total: u64 = weights.iter().sum();
    if total == 0 {
        // All averages were zero — fall back to uniform.
        let idx = rng.random_range(0..candidates.len());
        return Some(&candidates[idx]);
    }
    let mut roll = rng.random_range(0..total);
    for (c, w) in candidates.iter().zip(weights.iter()) {
        if roll < *w {
            return Some(c);
        }
        roll -= w;
    }
    // Fallback shouldn't happen given total > 0.
    candidates.last()
}

/// A small starter library of OLL cases. Real M11 work fills out all 57.
/// Each setup is the inverse of the case's algorithm — applied to a
/// post-F2L state, the cube ends up in the recognition pattern.
pub fn starter_oll_cases() -> Vec<DrillCase> {
    vec![
        DrillCase {
            id: "oll-21-h".into(),
            mode: DrillMode::Oll,
            display_name: "OLL 21 (H)".into(),
            setup: MoveSeq::parse(
                "R U R' U R U' R' U R U2 R'",
                3,
            )
            .expect("hard-coded setup parses"),
            canonical_algorithm: Some("R U2 R' U' R U R' U' R U' R'".into()),
        },
        DrillCase {
            id: "oll-22-pi".into(),
            mode: DrillMode::Oll,
            display_name: "OLL 22 (Pi)".into(),
            setup: MoveSeq::parse(
                "R' F R U R' U' F' U R",
                3,
            )
            .expect("hard-coded setup parses"),
            canonical_algorithm: Some("R U2 R2 U' R2 U' R2 U2 R".into()),
        },
        DrillCase {
            id: "oll-27-sune".into(),
            mode: DrillMode::Oll,
            display_name: "OLL 27 (Sune)".into(),
            setup: MoveSeq::parse("R' U' R U' R' U2 R", 3)
                .expect("hard-coded setup parses"),
            canonical_algorithm: Some("R U R' U R U2 R'".into()),
        },
        DrillCase {
            id: "oll-26-anti-sune".into(),
            mode: DrillMode::Oll,
            display_name: "OLL 26 (Anti-Sune)".into(),
            setup: MoveSeq::parse("R U2 R' U' R U' R'", 3)
                .expect("hard-coded setup parses"),
            canonical_algorithm: Some("R U2 R' U' R U' R'".into()),
        },
        DrillCase {
            id: "oll-23-h-cross".into(),
            mode: DrillMode::Oll,
            display_name: "OLL 23 (H-cross)".into(),
            setup: MoveSeq::parse(
                "R2 D' R U2 R' D R U2 R",
                3,
            )
            .expect("hard-coded setup parses"),
            canonical_algorithm: Some("R2 D R' U2 R D' R' U2 R'".into()),
        },
    ]
}

/// Variant of [`crate::start_solve`] for a specific drill case. Resets
/// the cube, applies the case's setup, and starts inspection.
pub fn start_drill(
    case: &DrillCase,
    cube_state: &mut crate::CubeState,
    pending: &mut crate::PendingMoves,
    timer: &mut crate::TimerState,
) -> MoveSeq {
    let size = cube_state.cube.size;
    cube_state.cube = cube_core::Cube::solved(size).expect("valid size");
    cube_state.reset_history();
    pending.0.clear();
    let setup_moves: Vec<Move> = case.setup.0.clone();
    pending.enqueue_all(setup_moves.iter().copied());
    timer.begin_inspection(case.setup.clone());
    case.setup.clone()
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::SeedableRng;
    use rand_chacha::ChaCha8Rng;

    #[test]
    fn per_case_stats_record_and_query() {
        let mut s = PerCaseStats::default();
        s.record("oll-27-sune", 4_000);
        s.record("oll-27-sune", 3_500);
        s.record("oll-27-sune", 4_500);
        s.record("oll-21-h", 6_000);
        assert_eq!(s.count("oll-27-sune"), 3);
        assert_eq!(s.average("oll-27-sune"), Some(4_000));
        assert_eq!(s.best("oll-27-sune"), Some(3_500));
        assert_eq!(s.worst("oll-27-sune"), Some(4_500));
        assert_eq!(s.average("oll-21-h"), Some(6_000));
        assert_eq!(s.average("oll-22-pi"), None);
    }

    #[test]
    fn weakness_ranking_puts_untried_first_then_slowest() {
        let cases = starter_oll_cases();
        let mut s = PerCaseStats::default();
        // Mark sune fast, h-perm slow; leave others untried.
        s.record("oll-27-sune", 2_000);
        s.record("oll-21-h", 8_000);
        let ranked = s.weakness_ranking(&cases);
        let ranked_ids: Vec<&str> = ranked.iter().map(|c| c.id.as_str()).collect();
        // Untried (3 of them) come first in some order, then h (slow),
        // then sune (fast).
        assert!(
            ranked_ids[0..3]
                .iter()
                .all(|id| !["oll-27-sune", "oll-21-h"].contains(id)),
            "untried not first: {ranked_ids:?}",
        );
        // h-perm before sune among the tried cases.
        let h_pos = ranked_ids.iter().position(|s| *s == "oll-21-h").unwrap();
        let sune_pos = ranked_ids.iter().position(|s| *s == "oll-27-sune").unwrap();
        assert!(h_pos < sune_pos);
    }

    #[test]
    fn pick_drill_case_returns_untried_when_available() {
        let cases = starter_oll_cases();
        let stats = PerCaseStats::default();
        let mut rng = ChaCha8Rng::seed_from_u64(0xC0FFEE);
        let pick = pick_drill_case(&cases, &stats, &mut rng).expect("non-empty");
        // With nothing tracked yet every case is untried, so any of them
        // is a valid pick.
        assert!(cases.iter().any(|c| c.id == pick.id));
    }

    #[test]
    fn pick_drill_case_biases_toward_slow_cases_once_all_tried() {
        let cases = starter_oll_cases();
        let mut stats = PerCaseStats::default();
        // Record at least one solve for every case so none are "untried".
        // Make the first one dramatically slower than the rest.
        for c in &cases {
            stats.record(&c.id, 1_000);
        }
        stats.record(&cases[0].id, 50_000); // boosts its average.
        let mut rng = ChaCha8Rng::seed_from_u64(0xDEADBEEF);
        let mut hits = 0;
        let trials = 1000;
        for _ in 0..trials {
            let p = pick_drill_case(&cases, &stats, &mut rng).unwrap();
            if p.id == cases[0].id {
                hits += 1;
            }
        }
        // Uniform random would give 1/5 = 20%; weighted-by-average² gives
        // ≫50% to the slow case. Allow generous slack.
        assert!(hits > trials * 4 / 10, "weighting too weak: {hits}/{trials}");
    }

    #[test]
    fn starter_oll_setups_all_parse() {
        // The library is hard-coded; failing to parse on construction is
        // a typo bug. Call once to validate.
        let cases = starter_oll_cases();
        assert!(!cases.is_empty());
        for c in &cases {
            assert!(!c.setup.is_empty(), "{} has empty setup", c.id);
        }
    }
}
