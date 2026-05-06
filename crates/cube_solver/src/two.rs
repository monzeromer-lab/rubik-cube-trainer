//! 2×2 solver.
//!
//! State space: 7! × 3^6 = 3,674,160 reachable states (DBL corner fixed at
//! home as the orientation reference). We build a full distance-from-solved
//! table by BFS in ~1–2 s, then solve any cube in microseconds via gradient
//! descent — every solution is optimal.
//!
//! See plan §6.3.

use cube_core::{Cube, Face, Move, MoveSeq, Orient, Turn, corner_twist};
use glam::IVec3;
use thiserror::Error;

/// Number of encoded states. `perm_idx (0..5040) * 729 + orient_idx (0..729)`.
pub const NUM_2X2_STATES: usize = 5040 * 729;

/// God's number for 2×2 in the half-turn metric is 11. We allow a generous
/// bound when reading distance values back from the table.
pub const GODS_NUMBER_2X2: u8 = 11;

/// 9-move solver alphabet: `R U F` × `Cw Half Ccw`. L/D/B are equivalent up
/// to whole-cube rotation, so we omit them and require DBL to remain at home.
const SOLVER_ALPHABET: [(Face, Turn); 9] = [
    (Face::R, Turn::Cw), (Face::R, Turn::Half), (Face::R, Turn::Ccw),
    (Face::U, Turn::Cw), (Face::U, Turn::Half), (Face::U, Turn::Ccw),
    (Face::F, Turn::Cw), (Face::F, Turn::Half), (Face::F, Turn::Ccw),
];

#[derive(Debug, Error)]
pub enum Solve2x2Error {
    #[error("cube must be size 2 (got {0})")]
    WrongSize(u8),
    #[error("DBL corner is not at home position with identity orientation; canonicalize first")]
    DblNotHome,
    #[error("cube state is not in the solver's reachable orbit")]
    Unreachable,
    #[error("internal: solved state must have orientation index 0")]
    Internal,
}

/// Compact 2×2 state. The DBL corner (`solved_pos = (-1, -1, -1)`) is fixed
/// at home with identity orientation and is not stored — slots 0..7 cover
/// the seven mobile corners in `Cube::cubies` lexicographic order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct State2x2 {
    /// `perm[slot]` = home slot index of the cubie currently at `slot`.
    pub perm: [u8; 7],
    /// `orient[slot]` ∈ {0, 1, 2}: parity-aware corner twist.
    pub orient: [u8; 7],
}

impl State2x2 {
    pub const fn solved() -> Self {
        Self { perm: [0, 1, 2, 3, 4, 5, 6], orient: [0; 7] }
    }

    pub fn is_solved(&self) -> bool {
        *self == Self::solved()
    }

    /// Encode to a u32 index in `0..NUM_2X2_STATES`.
    pub fn index(&self) -> u32 {
        // Lehmer code of perm[0..7].
        let mut perm_idx: u32 = 0;
        for i in 0..7 {
            let mut count: u32 = 0;
            for j in (i + 1)..7 {
                if self.perm[j] < self.perm[i] {
                    count += 1;
                }
            }
            perm_idx = perm_idx * (7 - i as u32) + count;
        }
        // Base-3 of orient[0..6] (orient[6] is fixed by sum-mod-3 invariant).
        let mut orient_idx: u32 = 0;
        for i in 0..6 {
            orient_idx = orient_idx * 3 + self.orient[i] as u32;
        }
        perm_idx * 729 + orient_idx
    }

    /// Decode an index back into a `State2x2`.
    pub fn from_index(idx: u32) -> Self {
        let perm_idx = idx / 729;
        let orient_idx = idx % 729;
        let mut perm = decode_lehmer_7(perm_idx);
        let mut orient = [0u8; 7];
        let mut o = orient_idx;
        for i in (0..6).rev() {
            orient[i] = (o % 3) as u8;
            o /= 3;
        }
        // orient[6] derived from sum-mod-3 invariant.
        let sum: u32 = orient[0..6].iter().map(|x| *x as u32).sum();
        orient[6] = ((3 - (sum % 3)) % 3) as u8;
        // Final touch: ensure perm uses 0..7 indices (decode produces this).
        let _ = &mut perm;
        Self { perm, orient }
    }

    /// Read a `State2x2` from a [`Cube`].
    pub fn from_cube(cube: &Cube) -> Result<Self, Solve2x2Error> {
        if cube.size != 2 {
            return Err(Solve2x2Error::WrongSize(cube.size));
        }
        let dbl = &cube.cubies[0];
        if dbl.solved_pos != IVec3::new(-1, -1, -1)
            || dbl.current_pos != dbl.solved_pos
            || dbl.orientation != Orient::IDENTITY
        {
            return Err(Solve2x2Error::DblNotHome);
        }

        let mut perm = [0u8; 7];
        let mut orient = [0u8; 7];
        for slot in 0..7 {
            let home_pos = cube.cubies[slot + 1].solved_pos;
            let cubie = cube
                .cubies
                .iter()
                .find(|c| c.current_pos == home_pos)
                .ok_or(Solve2x2Error::Unreachable)?;
            let home_idx = cube
                .cubies
                .iter()
                .position(|c| c.solved_pos == cubie.solved_pos)
                .unwrap();
            perm[slot] = (home_idx - 1) as u8;
            orient[slot] = corner_twist(cubie);
        }
        Ok(Self { perm, orient })
    }
}

// `corner_twist` is now shared with cube_core — see `cube_core::corner_twist`.

fn decode_lehmer_7(mut idx: u32) -> [u8; 7] {
    // Standard inverse Lehmer over a remaining-symbol pool.
    let mut pool: Vec<u8> = (0..7).collect();
    let mut perm = [0u8; 7];
    for (i, slot) in perm.iter_mut().enumerate() {
        let base = (7 - i) as u32;
        let factorial: u32 = (1..base).product::<u32>().max(1);
        let pick = (idx / factorial) as usize;
        idx %= factorial;
        *slot = pool.remove(pick);
    }
    perm
}

/// Slot-level move table: `perm[s]` = source slot, `orient[s]` = orientation
/// delta added (mod 3) to the piece moving from source to slot `s`.
#[derive(Debug, Clone, Copy)]
struct MoveOp {
    perm: [u8; 7],
    orient: [u8; 7],
}

impl State2x2 {
    fn apply_op(&self, op: &MoveOp) -> Self {
        let mut new_perm = [0u8; 7];
        let mut new_orient = [0u8; 7];
        for slot in 0..7 {
            let src = op.perm[slot] as usize;
            new_perm[slot] = self.perm[src];
            new_orient[slot] = (self.orient[src] + op.orient[slot]) % 3;
        }
        Self { perm: new_perm, orient: new_orient }
    }
}

fn derive_move_ops() -> Vec<(MoveOp, Move)> {
    SOLVER_ALPHABET
        .iter()
        .map(|&(face, turn)| {
            let m = Move::face(face, turn);
            let mut cube = Cube::solved(2).unwrap();
            cube.apply(m).unwrap();
            let s = State2x2::from_cube(&cube).expect("RUF moves keep DBL at home");
            (MoveOp { perm: s.perm, orient: s.orient }, m)
        })
        .collect()
}

/// Build the full distance-from-solved table by BFS.
fn build_distance_table(move_ops: &[(MoveOp, Move)]) -> Vec<u8> {
    let mut distances = vec![u8::MAX; NUM_2X2_STATES];
    let solved = State2x2::solved();
    distances[solved.index() as usize] = 0;
    let mut frontier: Vec<State2x2> = vec![solved];
    let mut next: Vec<State2x2> = Vec::new();
    let mut dist: u8 = 0;
    while !frontier.is_empty() {
        for state in frontier.drain(..) {
            for (op, _) in move_ops {
                let neighbor = state.apply_op(op);
                let idx = neighbor.index() as usize;
                if distances[idx] == u8::MAX {
                    distances[idx] = dist + 1;
                    next.push(neighbor);
                }
            }
        }
        std::mem::swap(&mut frontier, &mut next);
        dist += 1;
    }
    distances
}

/// 2×2 solver. Holds a full ~3.5 MB distance table.
pub struct Solver2x2 {
    distances: Vec<u8>,
    move_ops: Vec<(MoveOp, Move)>,
}

impl Solver2x2 {
    /// Build the distance table. Takes ~1–2 s on a mid-range CPU.
    pub fn new() -> Self {
        let move_ops = derive_move_ops();
        let distances = build_distance_table(&move_ops);
        debug_assert_eq!(distances[State2x2::solved().index() as usize], 0);
        Self { distances, move_ops }
    }

    /// True iff `state` lies in the orbit reachable from solved using R/U/F.
    pub fn is_reachable(&self, state: &State2x2) -> bool {
        self.distances[state.index() as usize] != u8::MAX
    }

    /// Distance from `state` to solved (the minimum move count).
    pub fn distance(&self, state: &State2x2) -> Option<u8> {
        let d = self.distances[state.index() as usize];
        if d == u8::MAX { None } else { Some(d) }
    }

    /// Solve a cube. Returns the optimal move sequence.
    pub fn solve(&self, cube: &Cube) -> Result<MoveSeq, Solve2x2Error> {
        let state = State2x2::from_cube(cube)?;
        self.solve_state(&state)
    }

    /// Solve from a [`State2x2`] directly.
    pub fn solve_state(&self, start: &State2x2) -> Result<MoveSeq, Solve2x2Error> {
        let mut state = *start;
        let mut dist = self.distance(&state).ok_or(Solve2x2Error::Unreachable)?;
        let mut seq: Vec<Move> = Vec::with_capacity(dist as usize);
        while dist > 0 {
            let mut found = false;
            for (op, m) in &self.move_ops {
                let next = state.apply_op(op);
                let next_dist = self.distances[next.index() as usize];
                if next_dist == dist - 1 {
                    seq.push(*m);
                    state = next;
                    dist = next_dist;
                    found = true;
                    break;
                }
            }
            if !found {
                return Err(Solve2x2Error::Internal);
            }
        }
        Ok(MoveSeq::from_vec(seq))
    }
}

impl Default for Solver2x2 {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn solver() -> Solver2x2 {
        Solver2x2::new()
    }

    #[test]
    fn solved_state_index_is_zero() {
        assert_eq!(State2x2::solved().index(), 0);
    }

    #[test]
    fn index_round_trip() {
        // A handful of perm/orient configurations.
        let states = [
            State2x2::solved(),
            State2x2 { perm: [0, 1, 2, 4, 6, 3, 5], orient: [0, 0, 0, 2, 1, 1, 2] },
            State2x2 { perm: [6, 5, 4, 3, 2, 1, 0], orient: [0, 1, 2, 0, 1, 2, 0] },
        ];
        for s in states {
            let idx = s.index();
            let back = State2x2::from_index(idx);
            assert_eq!(back, s, "round-trip failed for {s:?}");
        }
    }

    #[test]
    fn move_ops_derive_for_all_nine() {
        let ops = derive_move_ops();
        assert_eq!(ops.len(), 9);
    }

    #[test]
    fn r_then_r_inverse_returns_to_solved_via_state() {
        let ops = derive_move_ops();
        let r_cw = &ops[0].0;
        let r_ccw = &ops[2].0;
        let solved = State2x2::solved();
        let after = solved.apply_op(r_cw).apply_op(r_ccw);
        assert_eq!(after, solved);
    }

    #[test]
    fn r_four_times_returns_to_solved_via_state() {
        let ops = derive_move_ops();
        let r_cw = &ops[0].0;
        let mut s = State2x2::solved();
        for _ in 0..4 {
            s = s.apply_op(r_cw);
        }
        assert_eq!(s, State2x2::solved());
    }

    #[test]
    fn corner_twist_sum_invariant_after_one_move() {
        // After any single face turn from solved, total twist sum ≡ 0 (mod 3).
        for &(face, turn) in &SOLVER_ALPHABET {
            let mut cube = Cube::solved(2).unwrap();
            cube.apply(Move::face(face, turn)).unwrap();
            let s = State2x2::from_cube(&cube).unwrap();
            let sum: u32 = s.orient.iter().map(|x| *x as u32).sum();
            assert_eq!(sum % 3, 0, "twist sum invariant violated for {face:?} {turn:?}: {:?}", s.orient);
        }
    }

    #[test]
    fn solver_solves_solved_to_empty() {
        let s = solver();
        let cube = Cube::solved(2).unwrap();
        let seq = s.solve(&cube).unwrap();
        assert!(seq.is_empty());
    }

    #[test]
    fn solver_distance_table_full_orbit_size() {
        let s = solver();
        let reachable = s.distances.iter().filter(|&&d| d != u8::MAX).count();
        // Reachable orbit from solved using R/U/F = 3,674,160.
        assert_eq!(reachable, 3_674_160);
    }

    #[test]
    fn solver_max_distance_is_gods_number() {
        let s = solver();
        let max = s.distances.iter().filter(|&&d| d != u8::MAX).max().unwrap();
        // God's number for 2×2 in HTM is 11.
        assert_eq!(*max, GODS_NUMBER_2X2);
    }

    #[test]
    fn solver_round_trips_short_scrambles() {
        let s = solver();
        // RUF-only scrambles keep DBL at home.
        let scramble_strs = ["R U F", "R U' R'", "R2 F U' F2", "F R U R' U' F'"];
        for scr_s in scramble_strs {
            let scramble = MoveSeq::parse(scr_s, 2).unwrap();
            let mut cube = Cube::solved(2).unwrap();
            cube.apply_seq(&scramble).unwrap();
            let solution = s.solve(&cube).unwrap();
            cube.apply_seq(&solution).unwrap();
            assert!(cube.is_solved(), "scramble {scr_s:?} → solution {solution} did not solve");
        }
    }
}
