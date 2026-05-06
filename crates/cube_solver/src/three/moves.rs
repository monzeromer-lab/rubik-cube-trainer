//! Move alphabets for the two-phase solver and their slot-level effects on
//! corners and edges. Move ops are derived once from `cube_core::Cube`
//! simulation.

use cube_core::{Cube, Face, Move, Turn};
use glam::IVec3;

use super::state::{EDGE_HOMES, State3x3};

/// All 18 face turns. Order matters — the move tables and pruning tables
/// index into this array.
pub const PHASE1_MOVES: [(Face, Turn); 18] = [
    (Face::U, Turn::Cw),  (Face::U, Turn::Half), (Face::U, Turn::Ccw),
    (Face::D, Turn::Cw),  (Face::D, Turn::Half), (Face::D, Turn::Ccw),
    (Face::R, Turn::Cw),  (Face::R, Turn::Half), (Face::R, Turn::Ccw),
    (Face::L, Turn::Cw),  (Face::L, Turn::Half), (Face::L, Turn::Ccw),
    (Face::F, Turn::Cw),  (Face::F, Turn::Half), (Face::F, Turn::Ccw),
    (Face::B, Turn::Cw),  (Face::B, Turn::Half), (Face::B, Turn::Ccw),
];

/// Phase 2 indices into `PHASE1_MOVES`: U, U², U', D, D², D', R², L², F², B².
pub const PHASE2_TO_PHASE1: [usize; 10] = [0, 1, 2, 3, 4, 5, 7, 10, 13, 16];

/// `phase2_to_move(i)` is the i-th Phase 2 move as a `(Face, Turn)`.
pub fn phase2_to_move(idx: usize) -> (Face, Turn) {
    PHASE1_MOVES[PHASE2_TO_PHASE1[idx]]
}

/// Slot-level effect of a single move on the 8 corners.
#[derive(Debug, Clone, Copy)]
pub struct CornerMoveOp {
    /// `perm[s]` = source slot the cubie at `s` came from.
    pub perm: [u8; 8],
    /// `orient[s]` = orientation delta added to the moving cubie (mod 3).
    pub orient: [u8; 8],
}

#[derive(Debug, Clone, Copy)]
pub struct EdgeMoveOp {
    pub perm: [u8; 12],
    /// EO flip delta for an UD-layer cubie moving through this slot.
    /// (E-slice and UD-layer cubies experience different flip patterns
    /// because their primary stickers point along different axes.)
    pub delta_ud: [u8; 12],
    /// EO flip delta for an E-slice cubie moving through this slot.
    pub delta_es: [u8; 12],
}

/// Build per-move corner and edge ops by applying each Phase 1 move to a
/// solved cube and reading off the resulting `State3x3`. For edges the EO
/// deltas depend on cubie type (UD-layer vs E-slice), so we derive them
/// analytically from the move's rotation rather than per-slot from a single
/// solved-state simulation.
pub fn derive_phase1_ops() -> (Vec<CornerMoveOp>, Vec<EdgeMoveOp>) {
    let mut corner_ops = Vec::with_capacity(PHASE1_MOVES.len());
    let mut edge_ops = Vec::with_capacity(PHASE1_MOVES.len());
    for &(face, turn) in &PHASE1_MOVES {
        let m = Move::face(face, turn);
        let mut cube = Cube::solved(3).unwrap();
        cube.apply(m).unwrap();
        let s = State3x3::from_cube(&cube);
        corner_ops.push(CornerMoveOp { perm: s.corners.perm, orient: s.corners.orient });
        edge_ops.push(derive_edge_op(m, &s));
    }
    (corner_ops, edge_ops)
}

fn derive_edge_op(m: Move, after_solved: &State3x3) -> EdgeMoveOp {
    let rotation = m.rotation();
    // Does this move's rotation move the Y axis to a non-Y axis?
    let flips_y = rotation.apply(IVec3::Y).y == 0;
    // Same question for the Z axis.
    let flips_z = rotation.apply(IVec3::Z).z == 0;

    let mut delta_ud = [0u8; 12];
    let mut delta_es = [0u8; 12];
    for slot in 0..12 {
        let in_cycle = after_solved.edges.perm[slot] != slot as u8;
        if in_cycle {
            // UD-layer primary is Y; flips iff rotation moves Y axis.
            delta_ud[slot] = u8::from(flips_y);
            // E-slice primary is Z; flips iff rotation moves Z axis.
            delta_es[slot] = u8::from(flips_z);
        }
    }
    let _ = EDGE_HOMES; // kept for symmetry with corner derivation
    EdgeMoveOp {
        perm: after_solved.edges.perm,
        delta_ud,
        delta_es,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn corner_homes_match_cube_core() {
        use super::super::state::{CORNER_HOMES, EDGE_HOMES};
        let cube = Cube::solved(3).unwrap();
        for h in CORNER_HOMES {
            assert!(cube.cubies.iter().any(|c| c.solved_pos == h));
        }
        for h in EDGE_HOMES {
            assert!(cube.cubies.iter().any(|c| c.solved_pos == h));
        }
    }

    #[test]
    fn derive_returns_18_ops_each() {
        let (c, e) = derive_phase1_ops();
        assert_eq!(c.len(), 18);
        assert_eq!(e.len(), 18);
    }

    #[test]
    fn u_move_orientation_deltas_zero() {
        // U-cw shouldn't twist any corner; for edges, U-cw flips E-slice
        // cubies (because the rotation moves Z axis to X axis), but the
        // 4 cubies in U's cycle are all UD-layer, so no E-slice flips.
        let (corner_ops, edge_ops) = derive_phase1_ops();
        // U-cw is index 0.
        assert!(corner_ops[0].orient.iter().all(|&o| o == 0),
            "U twisted a corner: {:?}", corner_ops[0].orient);
        // UD-layer cubies don't flip under U (rotation around Y preserves Y).
        assert!(edge_ops[0].delta_ud.iter().all(|&o| o == 0));
    }

    #[test]
    fn r_squared_orientation_deltas_zero() {
        // R² is a Phase 2 move; preserves orientations of all cubie types.
        let (corner_ops, edge_ops) = derive_phase1_ops();
        // R² is index 7.
        assert!(corner_ops[7].orient.iter().all(|&o| o == 0));
        assert!(edge_ops[7].delta_ud.iter().all(|&o| o == 0));
        assert!(edge_ops[7].delta_es.iter().all(|&o| o == 0));
    }

    #[test]
    fn phase2_indices_map_to_correct_moves() {
        let expected: [(Face, Turn); 10] = [
            (Face::U, Turn::Cw), (Face::U, Turn::Half), (Face::U, Turn::Ccw),
            (Face::D, Turn::Cw), (Face::D, Turn::Half), (Face::D, Turn::Ccw),
            (Face::R, Turn::Half), (Face::L, Turn::Half),
            (Face::F, Turn::Half), (Face::B, Turn::Half),
        ];
        for (i, exp) in expected.iter().enumerate() {
            assert_eq!(phase2_to_move(i), *exp);
        }
    }
}
