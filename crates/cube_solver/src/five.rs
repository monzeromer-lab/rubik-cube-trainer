//! 5×5 reduction-based solver. See plan §6.5.
//!
//! ## How 5×5 differs from 4×4
//! - **Fixed center sticker**: every face has a single immutable center
//!   sticker at grid `(2, 2)`. Solved-orientation is unambiguous; no
//!   convention call is needed (unlike 4×4).
//! - **Edge triples**: each visible edge on the cube is three physical
//!   pieces — one "true" edge cubie at the center of the edge, plus two
//!   wing cubies flanking it. All three must be brought together with
//!   matching colors.
//! - **No parity cases**: a 5×5 has odd parity like a 3×3, so the OLL/PLL
//!   parity quirks that plague the 4×4 don't appear. The post-reduction
//!   3×3 view is always reachable.
//!
//! ## What this milestone (M14) ships
//! - The reduction-state predicates [`centers_solved`], [`edges_assembled`],
//!   and [`is_reduced`] over a [`Facelets`] view.
//! - [`project_to_3x3`]: sticker-level projection from a reduced 5×5.
//! - [`Solver5x5`] that solves any **already-reduced** 5×5 by projecting
//!   to a 3×3 facelet view and delegating to [`Solver3x3`]. Outer-layer
//!   3×3 moves carry over verbatim.
//!
//! ## What is M15 polish
//! Full IDA* center-solving and edge-assembly search — same shape as the
//! 4×4 case, with the extra wrinkle of triple assembly.

use cube_core::{Color, Cube, Face, Facelets, MoveSeq};
use thiserror::Error;

use crate::three::{Solve3x3Error, Solver3x3};

#[derive(Debug, Error)]
pub enum Solve5x5Error {
    #[error("cube must be size 5 (got {0})")]
    WrongSize(u8),
    #[error("cube is not reduced (centers solved + edges assembled) — full IDA* reduction is M15 polish")]
    NotReducedYet,
    #[error("3×3 phase failed: {0}")]
    ThreeByThree(#[from] Solve3x3Error),
    #[error("3×3 view of reduced 5×5 was not a reachable 3×3 state — should not happen for 5×5")]
    UnreachableProjection,
}

/// True iff every face's 3×3 center area (rows 1..=3, cols 1..=3 — the
/// nine inner stickers) shows that face's canonical color. The fixed
/// `(2, 2)` sticker pins orientation, so checking against the canonical
/// color is unambiguous on a 5×5.
pub fn centers_solved(facelets: &Facelets) -> bool {
    if facelets.size != 5 {
        return false;
    }
    for face in Face::ALL {
        let want = face_color(face);
        for row in 1..=3u8 {
            for col in 1..=3u8 {
                if facelets.get(face, row, col) != want {
                    return false;
                }
            }
        }
    }
    true
}

/// True iff every visible edge on the cube has its three stickers
/// (left wing, center edge, right wing) showing one color on each of
/// the two faces it touches. Tested per face by checking each of the
/// four edge segments of length 3.
pub fn edges_assembled(facelets: &Facelets) -> bool {
    if facelets.size != 5 {
        return false;
    }
    for face in Face::ALL {
        // Top edge: row 0, cols 1, 2, 3 — three stickers in a row.
        if !triple_match(facelets, face, [(0, 1), (0, 2), (0, 3)]) {
            return false;
        }
        // Bottom edge: row 4, cols 1, 2, 3.
        if !triple_match(facelets, face, [(4, 1), (4, 2), (4, 3)]) {
            return false;
        }
        // Left edge: col 0, rows 1, 2, 3.
        if !triple_match(facelets, face, [(1, 0), (2, 0), (3, 0)]) {
            return false;
        }
        // Right edge: col 4, rows 1, 2, 3.
        if !triple_match(facelets, face, [(1, 4), (2, 4), (3, 4)]) {
            return false;
        }
    }
    true
}

fn triple_match(facelets: &Facelets, face: Face, cells: [(u8, u8); 3]) -> bool {
    let first = facelets.get(face, cells[0].0, cells[0].1);
    cells
        .iter()
        .all(|&(r, c)| facelets.get(face, r, c) == first)
}

/// True iff [`centers_solved`] and [`edges_assembled`].
pub fn is_reduced(facelets: &Facelets) -> bool {
    centers_solved(facelets) && edges_assembled(facelets)
}

/// Project a reduced 5×5 facelet view to a 3×3 facelet view by sampling
/// one sticker per macro-position. Caller must have already verified
/// [`is_reduced`]; otherwise the projection mixes colors from unrelated
/// physical pieces.
pub fn project_to_3x3(facelets: &Facelets) -> Facelets {
    let mut out = Facelets::solved(3);
    for face in Face::ALL {
        for row in 0..3u8 {
            for col in 0..3u8 {
                let (r5, c5) = sample_5x5_for_3x3_cell(row, col);
                out.set(face, row, col, facelets.get(face, r5, c5));
            }
        }
    }
    out
}

/// 5×5 → 3×3 sticker-cell projection. Each 3×3 sticker corresponds to a
/// region of the 5×5 face; we pick one representative.
fn sample_5x5_for_3x3_cell(r3: u8, c3: u8) -> (u8, u8) {
    // Corners (r3, c3) ∈ {0, 2}² → outermost 5×5 corners (r5, c5) ∈ {0, 4}.
    // Edges (one of r3/c3 is 1) → the center sticker of the corresponding
    // 5×5 edge, e.g. (0, 1) → (0, 2) [the "true" edge cubie].
    // Center (1, 1) → fixed center (2, 2) on the 5×5 face.
    let r5 = match r3 { 0 => 0, 1 => 2, 2 => 4, _ => unreachable!() };
    let c5 = match c3 { 0 => 0, 1 => 2, 2 => 4, _ => unreachable!() };
    (r5, c5)
}

fn face_color(face: Face) -> Color {
    match face {
        Face::U => Color::U,
        Face::D => Color::D,
        Face::F => Color::F,
        Face::B => Color::B,
        Face::R => Color::R,
        Face::L => Color::L,
    }
}

/// 5×5 reduction-aware solver. Builds and owns a [`Solver3x3`] internally.
pub struct Solver5x5 {
    pub solver_3x3: Solver3x3,
}

impl Solver5x5 {
    pub fn new() -> Self {
        Self { solver_3x3: Solver3x3::new() }
    }

    /// Mirror of [`Solver3x3::new_with_cache`] — disk-caches the inner
    /// 3×3's pruning tables. The 5×5 itself has no extra heavy tables.
    pub fn new_with_cache(cache_path: &std::path::Path) -> Self {
        Self { solver_3x3: Solver3x3::new_with_cache(cache_path) }
    }

    pub fn solve(&self, cube: &Cube) -> Result<MoveSeq, Solve5x5Error> {
        if cube.size != 5 {
            return Err(Solve5x5Error::WrongSize(cube.size));
        }
        let facelets = Facelets::from_cube(cube);
        if !is_reduced(&facelets) {
            return Err(Solve5x5Error::NotReducedYet);
        }
        let three_view = project_to_3x3(&facelets);
        // 5×5 has no parity quirks — projection is always reachable.
        let three_cube = three_view
            .try_into_cube()
            .map_err(|_| Solve5x5Error::UnreachableProjection)?;
        let three_solution = self.solver_3x3.solve(&three_cube)?;
        // Outer-layer 3×3 moves apply unchanged to the 5×5.
        Ok(three_solution)
    }
}

impl Default for Solver5x5 {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cube_core::{Move, Turn};

    #[test]
    fn solved_5x5_is_reduced() {
        let cube = Cube::solved(5).unwrap();
        let f = Facelets::from_cube(&cube);
        assert!(centers_solved(&f));
        assert!(edges_assembled(&f));
        assert!(is_reduced(&f));
    }

    #[test]
    fn outer_only_scramble_keeps_reduction() {
        // Pure outer-layer turns don't touch inner slices, so centers and
        // edge triples remain assembled.
        let mut cube = Cube::solved(5).unwrap();
        cube.apply_seq(&MoveSeq::parse("R U R' U' F U F' U", 5).unwrap()).unwrap();
        let f = Facelets::from_cube(&cube);
        assert!(centers_solved(&f), "centers disturbed by outer-only scramble");
        assert!(edges_assembled(&f), "edges disassembled by outer-only scramble");
    }

    #[test]
    fn inner_slice_breaks_reduction() {
        // A single 2R (slice 2 from R) on a 5×5 disturbs centers and
        // breaks the edge triples adjacent to that slice.
        let mut cube = Cube::solved(5).unwrap();
        cube.apply(Move {
            face: Face::R, turn: Turn::Cw, depth: 2, wide: false,
        }).unwrap();
        let f = Facelets::from_cube(&cube);
        assert!(!is_reduced(&f), "single 2R should break reduction");
    }

    #[test]
    fn deepest_slice_breaks_reduction() {
        // 3R slices the very middle layer of a 5×5 — same expected effect.
        let mut cube = Cube::solved(5).unwrap();
        cube.apply(Move {
            face: Face::R, turn: Turn::Cw, depth: 3, wide: false,
        }).unwrap();
        let f = Facelets::from_cube(&cube);
        assert!(!is_reduced(&f), "single 3R should break reduction");
    }

    #[test]
    fn project_to_3x3_of_solved_is_solved_3x3() {
        let cube = Cube::solved(5).unwrap();
        let f5 = Facelets::from_cube(&cube);
        let f3 = project_to_3x3(&f5);
        assert_eq!(f3, Facelets::solved(3));
    }

    #[test]
    fn solver_handles_solved_cube() {
        let s = Solver5x5::new();
        let cube = Cube::solved(5).unwrap();
        let seq = s.solve(&cube).unwrap();
        assert!(seq.is_empty(), "solved cube should produce empty solution, got {seq}");
    }

    #[test]
    fn solver_rejects_wrong_size() {
        let s = Solver5x5::new();
        let cube = Cube::solved(3).unwrap();
        assert!(matches!(s.solve(&cube), Err(Solve5x5Error::WrongSize(3))));
    }

    #[test]
    fn solver_rejects_unreduced() {
        // Single 2R breaks reduction — solver must return NotReducedYet.
        let s = Solver5x5::new();
        let mut cube = Cube::solved(5).unwrap();
        cube.apply(Move {
            face: Face::R, turn: Turn::Cw, depth: 2, wide: false,
        }).unwrap();
        assert!(matches!(s.solve(&cube), Err(Solve5x5Error::NotReducedYet)));
    }

    #[test]
    fn solver_solves_outer_only_scrambles() {
        // Single-move scrambles. Two-move and deeper scrambles hit the
        // M3-perf-limited Solver3x3 Phase-2 heuristic and can take
        // minutes; tighter pruning is M15 polish.
        let s = Solver5x5::new();
        for scramble_str in ["R", "U", "F", "R'", "U2"] {
            let scramble = MoveSeq::parse(scramble_str, 5).unwrap();
            let mut cube = Cube::solved(5).unwrap();
            cube.apply_seq(&scramble).unwrap();
            let solution = s.solve(&cube).unwrap_or_else(|e| {
                panic!("solve failed for {scramble_str:?}: {e}");
            });
            cube.apply_seq(&solution).unwrap();
            assert!(
                cube.is_solved(),
                "scramble {scramble_str:?} → solution {solution} did not solve"
            );
        }
    }
}
