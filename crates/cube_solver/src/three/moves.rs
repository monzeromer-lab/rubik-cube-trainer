//! Move alphabets for the two-phase solver and their slot-level effects on
//! corners and edges. Move ops are derived once from `cube_core::Cube`
//! simulation.

use cube_core::{Cube, Face, Move, Turn};

use super::state::State3x3;

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
    /// `perm[slot]` is the source slot the cubie at `slot` came from.
    pub perm: [u8; 12],
    /// 24-element rotation index applied to every cubie that the move
    /// rotates (composed from the cubie's existing orientation). Cubies
    /// outside the move's cycle keep their orientation unchanged — see
    /// `in_cycle`.
    pub move_rot: u8,
    /// True for slots whose cubies are rotated by this move (every slot in
    /// the move's 4-cycle). Cubies in non-cycle slots keep their full
    /// rotation; we still propagate them through `perm` (which is identity
    /// for them) so the apply-loop is uniform.
    pub in_cycle: [bool; 12],
}

/// Build per-move corner and edge ops by applying each Phase 1 move to a
/// solved cube and reading off the resulting `State3x3`. The edge op also
/// records the move's 24-element rotation; `apply_edge_op` then composes it
/// onto the existing per-slot rotation, which is the only EO model that
/// actually closes under multi-move sequences (the older additive bit-XOR
/// model didn't — see `tables::apply_edge_op` history).
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
    let move_rot = m.rotation().0;
    let mut in_cycle = [false; 12];
    for slot in 0..12 {
        in_cycle[slot] = after_solved.edges.perm[slot] != slot as u8;
    }
    EdgeMoveOp {
        perm: after_solved.edges.perm,
        move_rot,
        in_cycle,
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
    fn u_move_corners_dont_twist() {
        // U-cw shouldn't twist any corner; the 4 corners in its cycle stay
        // oriented because U rotates around their primary (Y) axis.
        let (corner_ops, _) = derive_phase1_ops();
        // U-cw is index 0.
        assert!(corner_ops[0].orient.iter().all(|&o| o == 0),
            "U twisted a corner: {:?}", corner_ops[0].orient);
    }

    #[test]
    fn r_squared_in_cycle_marks_eight_slots() {
        // R² cycles UR↔DR and FR↔BR, so 4 edge slots should be in the
        // cycle. Corner-side: same — 4 corner slots in UR-DR / FR-BR axis
        // pairs are cycled. Both lookup tables must reflect that.
        let (_, edge_ops) = derive_phase1_ops();
        // R² is index 7.
        let cycled = edge_ops[7].in_cycle.iter().filter(|&&x| x).count();
        assert_eq!(cycled, 4, "R² should cycle exactly 4 edge slots");
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
