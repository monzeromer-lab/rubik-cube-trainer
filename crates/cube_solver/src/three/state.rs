//! 3×3 cubie-level state, decoupled from the heavyweight `cube_core::Cube`.
//!
//! The two-phase solver operates on a compact representation of corners and
//! edges with parity-aware orientation values. This module extracts that
//! representation from a `cube_core::Cube` and provides goal predicates.

use cube_core::{Cube, Cubie, corner_twist as cube_core_corner_twist, edge_orient as cube_core_edge_orient};
use glam::IVec3;

/// 8 corner home positions in Singmaster/Kociemba order: URF, UFL, ULB,
/// UBR, DFR, DLF, DBL, DRB. The integer-lattice positions for a 3×3
/// (`size = 3`, coords ∈ {-2, 0, 2}) are at the corners with all coords ±2.
pub const CORNER_HOMES: [IVec3; 8] = [
    IVec3::new(2, 2, 2),    // 0: URF
    IVec3::new(-2, 2, 2),   // 1: UFL
    IVec3::new(-2, 2, -2),  // 2: ULB
    IVec3::new(2, 2, -2),   // 3: UBR
    IVec3::new(2, -2, 2),   // 4: DFR
    IVec3::new(-2, -2, 2),  // 5: DLF
    IVec3::new(-2, -2, -2), // 6: DBL
    IVec3::new(2, -2, -2),  // 7: DRB
];

/// 12 edge home positions in Kociemba order: UR, UF, UL, UB (top layer),
/// DR, DF, DL, DB (bottom layer), then the four E-slice edges
/// FR, FL, BL, BR. The two-phase algorithm requires this exact ordering
/// because the udslice coordinate is defined over indices 8..12.
pub const EDGE_HOMES: [IVec3; 12] = [
    IVec3::new(2, 2, 0),    // 0: UR
    IVec3::new(0, 2, 2),    // 1: UF
    IVec3::new(-2, 2, 0),   // 2: UL
    IVec3::new(0, 2, -2),   // 3: UB
    IVec3::new(2, -2, 0),   // 4: DR
    IVec3::new(0, -2, 2),   // 5: DF
    IVec3::new(-2, -2, 0),  // 6: DL
    IVec3::new(0, -2, -2),  // 7: DB
    IVec3::new(2, 0, 2),    // 8: FR
    IVec3::new(-2, 0, 2),   // 9: FL
    IVec3::new(-2, 0, -2),  // 10: BL
    IVec3::new(2, 0, -2),   // 11: BR
];

/// A corner is "in the E-slice" iff its slot index is < 0 (not applicable —
/// corners aren't E-slice). For edges, slot indices 8..12 are E-slice slots.
pub const E_SLICE_FIRST: usize = 8;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct CornerState {
    /// `perm[slot]` is the home index of the corner currently at `slot`.
    pub perm: [u8; 8],
    /// Parity-aware corner twist value, 0..3.
    pub orient: [u8; 8],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct EdgeState {
    /// `perm[slot]` is the home index of the edge currently at `slot`.
    pub perm: [u8; 12],
    /// Edge orientation, 0..2 (0 = good, 1 = flipped).
    pub orient: [u8; 12],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct State3x3 {
    pub corners: CornerState,
    pub edges: EdgeState,
}

impl CornerState {
    pub const fn solved() -> Self {
        Self { perm: [0, 1, 2, 3, 4, 5, 6, 7], orient: [0; 8] }
    }
}

impl EdgeState {
    pub const fn solved() -> Self {
        Self {
            perm: [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11],
            orient: [0; 12],
        }
    }
}

impl State3x3 {
    pub const fn solved() -> Self {
        Self {
            corners: CornerState::solved(),
            edges: EdgeState::solved(),
        }
    }

    pub fn is_solved(&self) -> bool {
        *self == Self::solved()
    }

    /// True iff the state is in `G1 = ⟨U, D, R², L², F², B²⟩`: all corners
    /// oriented, all edges oriented, all E-slice edges in E-slice slots.
    pub fn is_in_g1(&self) -> bool {
        if self.corners.orient.iter().any(|&o| o != 0) {
            return false;
        }
        if self.edges.orient.iter().any(|&o| o != 0) {
            return false;
        }
        // E-slice edges (home indices 8..12) must be in slots 8..12.
        for slot in 0..E_SLICE_FIRST {
            if self.edges.perm[slot] as usize >= E_SLICE_FIRST {
                return false;
            }
        }
        true
    }

    /// Read a `State3x3` out of a 3×3 [`Cube`]. Panics if `cube.size != 3`.
    pub fn from_cube(cube: &Cube) -> Self {
        assert_eq!(cube.size, 3, "State3x3 only supports 3×3 cubes");

        let mut corners = CornerState::solved();
        for slot in 0..8 {
            let home_pos = CORNER_HOMES[slot];
            let cubie = find_cubie_at(cube, home_pos);
            let home_idx = position_index(&CORNER_HOMES, cubie.solved_pos);
            corners.perm[slot] = home_idx as u8;
            corners.orient[slot] = corner_twist(cubie, home_pos);
        }

        let mut edges = EdgeState::solved();
        for slot in 0..12 {
            let home_pos = EDGE_HOMES[slot];
            let cubie = find_cubie_at(cube, home_pos);
            let home_idx = position_index(&EDGE_HOMES, cubie.solved_pos);
            edges.perm[slot] = home_idx as u8;
            edges.orient[slot] = edge_orient(cubie);
        }

        Self { corners, edges }
    }
}

fn find_cubie_at(cube: &Cube, pos: IVec3) -> &Cubie {
    cube.cubies
        .iter()
        .find(|c| c.current_pos == pos)
        .expect("position must be occupied on a 3×3 in valid state")
}

fn position_index(table: &[IVec3], pos: IVec3) -> usize {
    table.iter().position(|p| *p == pos).expect("position must be in table")
}

// `corner_twist` and `edge_orient` are now shared with cube_core — see
// `cube_core::corner_twist` and `cube_core::edge_orient`.
pub(crate) fn corner_twist(cubie: &Cubie, _current_pos: IVec3) -> u8 {
    cube_core_corner_twist(cubie)
}

pub(crate) fn edge_orient(cubie: &Cubie) -> u8 {
    cube_core_edge_orient(cubie)
}

#[cfg(test)]
mod tests {
    use super::*;
    use cube_core::{Face, Move, MoveSeq, Turn};

    #[test]
    fn solved_state_from_solved_cube() {
        let cube = Cube::solved(3).unwrap();
        let s = State3x3::from_cube(&cube);
        assert_eq!(s, State3x3::solved());
        assert!(s.is_solved());
        assert!(s.is_in_g1());
    }

    #[test]
    fn solved_corner_orient_sum_is_zero() {
        let cube = Cube::solved(3).unwrap();
        let s = State3x3::from_cube(&cube);
        let sum: u32 = s.corners.orient.iter().map(|x| *x as u32).sum();
        assert_eq!(sum % 3, 0);
    }

    #[test]
    fn corner_orient_invariant_after_face_turn() {
        for face in Face::ALL {
            let mut cube = Cube::solved(3).unwrap();
            cube.apply(Move::face(face, Turn::Cw)).unwrap();
            let s = State3x3::from_cube(&cube);
            let sum: u32 = s.corners.orient.iter().map(|x| *x as u32).sum();
            assert_eq!(sum % 3, 0, "corner orient sum invariant broken by {face:?} Cw");
        }
    }

    #[test]
    fn edge_orient_invariant_under_phase2_moves() {
        // Phase 2: U, D, R², L², F², B² preserve EO=0.
        let phase2 = ["U", "D", "R2", "L2", "F2", "B2", "U'", "D'"];
        for s in phase2 {
            let mut cube = Cube::solved(3).unwrap();
            let seq = MoveSeq::parse(s, 3).unwrap();
            cube.apply_seq(&seq).unwrap();
            let st = State3x3::from_cube(&cube);
            assert!(st.edges.orient.iter().all(|&o| o == 0),
                "EO not preserved by Phase 2 move {s}: {:?}", st.edges.orient);
        }
    }

    #[test]
    fn r_quarter_turn_changes_eo() {
        let mut cube = Cube::solved(3).unwrap();
        cube.apply(Move::face(Face::R, Turn::Cw)).unwrap();
        let s = State3x3::from_cube(&cube);
        // R-cw flips 4 R-face edges (UR, DR, FR, BR).
        let flipped: u32 = s.edges.orient.iter().map(|x| *x as u32).sum();
        assert_eq!(flipped, 4);
        // EO sum is preserved mod 2.
        assert_eq!(flipped % 2, 0);
    }

    #[test]
    fn f_quarter_turn_flips_two_edges() {
        // F flips UF and DF; FR/FL stay oriented because F rotates around
        // their primary (Z) axis.
        let mut cube = Cube::solved(3).unwrap();
        cube.apply(Move::face(Face::F, Turn::Cw)).unwrap();
        let s = State3x3::from_cube(&cube);
        let flipped: u32 = s.edges.orient.iter().map(|x| *x as u32).sum();
        assert_eq!(flipped, 2);
    }

    #[test]
    fn r_squared_preserves_eo() {
        let mut cube = Cube::solved(3).unwrap();
        cube.apply(Move::face(Face::R, Turn::Half)).unwrap();
        let s = State3x3::from_cube(&cube);
        assert!(s.edges.orient.iter().all(|&o| o == 0));
    }

    #[test]
    fn r_squared_keeps_in_g1() {
        // R² is a Phase 2 move; staying in G1 from solved.
        let mut cube = Cube::solved(3).unwrap();
        cube.apply(Move::face(Face::R, Turn::Half)).unwrap();
        let s = State3x3::from_cube(&cube);
        assert!(s.is_in_g1());
    }

    #[test]
    fn r_quarter_leaves_g1() {
        let mut cube = Cube::solved(3).unwrap();
        cube.apply(Move::face(Face::R, Turn::Cw)).unwrap();
        let s = State3x3::from_cube(&cube);
        assert!(!s.is_in_g1());
    }
}
