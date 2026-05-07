//! Move tables and pruning tables for the two-phase solver.
//!
//! Building everything takes a single multi-second pass on first run. We
//! deliberately keep the tables in memory only — disk-cache wiring lives in
//! `cube_render`/`cube_trainer` later milestones (plan §13 layout).

use super::coords::{
    N_CO, N_CP, N_EO, N_EP, N_UDSLICE, N_UDSLICE_PERM, decode_udslice_mask, encode_co,
    encode_cp, encode_eo, encode_ep, encode_udslice, encode_udslice_perm,
};
use super::moves::{CornerMoveOp, EdgeMoveOp, PHASE2_TO_PHASE1, derive_phase1_ops};
use super::state::{CornerState, EdgeState, State3x3};

pub const N_PHASE1_MOVES: usize = 18;
pub const N_PHASE2_MOVES: usize = 10;

/// Distance sentinel for unfilled pruning entries.
pub const UNVISITED: u8 = u8::MAX;

pub struct MoveTables {
    pub corner_ops: Vec<CornerMoveOp>,
    pub edge_ops: Vec<EdgeMoveOp>,
}

impl MoveTables {
    pub fn build() -> Self {
        let (corner_ops, edge_ops) = derive_phase1_ops();
        Self { corner_ops, edge_ops }
    }
}

pub fn apply_corner_op(s: &CornerState, op: &CornerMoveOp) -> CornerState {
    let mut new_perm = [0u8; 8];
    let mut new_orient = [0u8; 8];
    for slot in 0..8 {
        let src = op.perm[slot] as usize;
        new_perm[slot] = s.perm[src];
        new_orient[slot] = (s.orient[src] + op.orient[slot]) % 3;
    }
    CornerState { perm: new_perm, orient: new_orient }
}

pub fn apply_edge_op(s: &EdgeState, op: &EdgeMoveOp) -> EdgeState {
    let mut new_perm = [0u8; 12];
    let mut new_orient = [0u8; 12];
    for slot in 0..12 {
        let src = op.perm[slot] as usize;
        let cubie = s.perm[src];
        // Type of the cubie that's moving: UD-layer iff home index < 8.
        let delta = if (cubie as usize) < super::state::E_SLICE_FIRST {
            op.delta_ud[slot]
        } else {
            op.delta_es[slot]
        };
        new_perm[slot] = cubie;
        new_orient[slot] = (s.orient[src] + delta) % 2;
    }
    EdgeState { perm: new_perm, orient: new_orient }
}

// ---- Pruning tables ----

pub struct PruningTables {
    /// Phase 1: distance from `(co, udslice)` to `(co=0, udslice=goal)`.
    /// CO and udslice transitions each depend only on their own coordinate
    /// (plus the move), so single-rep BFS in the full state space covers
    /// every cell. Admissible by construction.
    pub p1_co_udslice: Vec<u8>,
    /// Phase 1: distance from `(eo, udslice)` to `(0, goal)`. EO transitions
    /// depend on the udslice mask (E-slice vs UD-layer cubies flip
    /// differently), so we cannot run BFS in (eo) space alone — we BFS in
    /// joint (eo, udslice) coord space using a canonical edge-state
    /// representative for each coord pair. That covers every cell and is
    /// admissible.
    pub p1_eo_udslice: Vec<u8>,
    /// Phase 2: distance from `(cp, udslice_perm)` to `(0, 0)`.
    pub p2_cp_uperm: Vec<u8>,
    /// Phase 2: distance from `(ep, udslice_perm)` to `(0, 0)`.
    pub p2_ep_uperm: Vec<u8>,
    /// Goal udslice value (encoded representation of "E-slice in slots 8..12").
    pub udslice_goal: u16,
}

impl PruningTables {
    pub fn build(tables: &MoveTables) -> Self {
        let udslice_goal = encode_udslice(&EdgeState::solved()) as u16;
        Self {
            // Phase 1 BFS evolves the full corner+edge state and memoizes
            // only the coord-pair distance, because EO transitions depend on
            // the permutation (UD-layer vs E-slice cubie types flip
            // differently). The min-distance over all states sharing a coord
            // pair is a sound admissible heuristic.
            p1_co_udslice: build_phase1_full_pruning(
                tables,
                N_CO * N_UDSLICE,
                |s| {
                    encode_co(&s.corners) as usize * N_UDSLICE
                        + encode_udslice(&s.edges) as usize
                },
            ),
            p1_eo_udslice: build_eo_udslice_pruning(tables),
            p2_cp_uperm: build_phase2_full_pruning(
                tables,
                N_CP * N_UDSLICE_PERM,
                |s| {
                    encode_cp(&s.corners) as usize * N_UDSLICE_PERM
                        + encode_udslice_perm(&s.edges) as usize
                },
            ),
            p2_ep_uperm: build_phase2_full_pruning(
                tables,
                N_EP * N_UDSLICE_PERM,
                |s| {
                    encode_ep(&s.edges) as usize * N_UDSLICE_PERM
                        + encode_udslice_perm(&s.edges) as usize
                },
            ),
            udslice_goal,
        }
    }

    #[inline]
    pub fn p1_co_udslice_at(&self, co: u16, ud: u16) -> u8 {
        self.p1_co_udslice[co as usize * N_UDSLICE + ud as usize]
    }
    #[inline]
    pub fn p1_eo_udslice_at(&self, eo: u16, ud: u16) -> u8 {
        self.p1_eo_udslice[eo as usize * N_UDSLICE + ud as usize]
    }
    #[inline]
    pub fn p2_cp_uperm_at(&self, cp: u32, up: u8) -> u8 {
        self.p2_cp_uperm[cp as usize * N_UDSLICE_PERM + up as usize]
    }
    #[inline]
    pub fn p2_ep_uperm_at(&self, ep: u32, up: u8) -> u8 {
        self.p2_ep_uperm[ep as usize * N_UDSLICE_PERM + up as usize]
    }
}

/// BFS in full state space, memoizing the first-visit distance per coord-pair
/// produced by `coord`. Phase 1 explores all 18 moves.
fn build_phase1_full_pruning(
    tables: &MoveTables,
    table_size: usize,
    coord: impl Fn(&State3x3) -> usize,
) -> Vec<u8> {
    build_full_pruning(tables, table_size, &coord, N_PHASE1_MOVES, |i| i)
}

/// BFS in full state space, Phase 2 explores only the 10 Phase 2 moves.
fn build_phase2_full_pruning(
    tables: &MoveTables,
    table_size: usize,
    coord: impl Fn(&State3x3) -> usize,
) -> Vec<u8> {
    build_full_pruning(tables, table_size, &coord, N_PHASE2_MOVES, |i| {
        PHASE2_TO_PHASE1[i]
    })
}

/// Construct the canonical [`EdgeState`] representative for a `(eo,
/// udslice)` coord pair: UD-layer and E-slice cubies placed in lex order
/// at their respective slot classes, orientations decoded from `eo` with
/// `orient[11]` reconstructed from the parity invariant.
fn canonical_eo_udslice_state(eo: u32, ud: u32) -> EdgeState {
    let mask = decode_udslice_mask(ud);
    let mut perm = [0u8; 12];
    let mut ud_iter = 0u8..8u8;
    let mut es_iter = 8u8..12u8;
    for (slot, &is_es) in mask.iter().enumerate() {
        perm[slot] = if is_es {
            es_iter.next().expect("4 E-slice cubies for 4 mask slots")
        } else {
            ud_iter.next().expect("8 UD cubies for 8 non-mask slots")
        };
    }
    let mut orient = [0u8; 12];
    let mut v = eo;
    for i in (0..11).rev() {
        orient[i] = (v % 2) as u8;
        v /= 2;
    }
    let parity: u32 = orient[..11].iter().map(|&x| x as u32).sum::<u32>() % 2;
    orient[11] = parity as u8;
    EdgeState { perm, orient }
}

/// BFS in `(eo, udslice)` coord space, expanding each visited coord by
/// applying all 18 moves to its canonical edge-state representative. (The
/// closure-style single-rep BFS in full-state space silently terminates
/// early on this projection because the chosen representative diverges
/// from the canonical one at deeper paths — a coord can be unreachable
/// from one rep yet reachable from another. Building over canonical reps
/// guarantees full coverage.)
fn build_eo_udslice_pruning(tables: &MoveTables) -> Vec<u8> {
    let table_size = N_EO * N_UDSLICE;
    let mut table = vec![UNVISITED; table_size];
    let goal = EdgeState::solved();
    let goal_idx = encode_eo(&goal) as usize * N_UDSLICE + encode_udslice(&goal) as usize;
    table[goal_idx] = 0;
    let mut frontier: Vec<usize> = vec![goal_idx];
    let mut next: Vec<usize> = Vec::new();
    let mut dist: u8 = 0;
    while !frontier.is_empty() {
        for &idx in &frontier {
            let eo = (idx / N_UDSLICE) as u32;
            let ud = (idx % N_UDSLICE) as u32;
            let canon = canonical_eo_udslice_state(eo, ud);
            for m in 0..N_PHASE1_MOVES {
                let new_edges = apply_edge_op(&canon, &tables.edge_ops[m]);
                let new_idx = encode_eo(&new_edges) as usize * N_UDSLICE
                    + encode_udslice(&new_edges) as usize;
                if table[new_idx] == UNVISITED {
                    table[new_idx] = dist + 1;
                    next.push(new_idx);
                }
            }
        }
        frontier.clear();
        std::mem::swap(&mut frontier, &mut next);
        dist += 1;
    }
    table
}

fn build_full_pruning(
    tables: &MoveTables,
    table_size: usize,
    coord: &dyn Fn(&State3x3) -> usize,
    n_moves: usize,
    move_to_op_idx: impl Fn(usize) -> usize,
) -> Vec<u8> {
    let mut table = vec![UNVISITED; table_size];
    let goal = State3x3::solved();
    let goal_idx = coord(&goal);
    table[goal_idx] = 0;
    let mut frontier: Vec<State3x3> = vec![goal];
    let mut next: Vec<State3x3> = Vec::new();
    let mut dist: u8 = 0;
    while !frontier.is_empty() {
        for state in frontier.drain(..) {
            for m in 0..n_moves {
                let op = move_to_op_idx(m);
                let mut new_state = state;
                new_state.corners = apply_corner_op(&new_state.corners, &tables.corner_ops[op]);
                new_state.edges = apply_edge_op(&new_state.edges, &tables.edge_ops[op]);
                let idx = coord(&new_state);
                if table[idx] == UNVISITED {
                    table[idx] = dist + 1;
                    next.push(new_state);
                }
            }
        }
        std::mem::swap(&mut frontier, &mut next);
        dist += 1;
    }
    table
}

#[cfg(test)]
mod tests {
    use super::*;

    fn build_tables() -> MoveTables {
        MoveTables::build()
    }


    #[test]
    fn apply_edge_op_r_cw_on_solved_flips_four() {
        let t = build_tables();
        let solved = EdgeState::solved();
        // R-cw is index 6.
        let new = apply_edge_op(&solved, &t.edge_ops[6]);
        let flipped: u32 = new.orient.iter().map(|&x| x as u32).sum();
        assert_eq!(flipped, 4, "R-cw should flip 4 R-face edges; got orient={:?}", new.orient);
    }

    #[test]
    fn apply_edge_op_f_cw_on_solved_flips_two() {
        let t = build_tables();
        let solved = EdgeState::solved();
        // F-cw is index 12.
        let new = apply_edge_op(&solved, &t.edge_ops[12]);
        let flipped: u32 = new.orient.iter().map(|&x| x as u32).sum();
        assert_eq!(flipped, 2, "F-cw should flip 2 UD-layer F-face edges; got orient={:?}", new.orient);
    }

    #[test]
    fn pruning_p1_co_udslice_goal_is_zero() {
        let t = build_tables();
        let p = PruningTables::build(&t);
        assert_eq!(p.p1_co_udslice_at(0, p.udslice_goal), 0);
    }

    #[test]
    fn pruning_p2_cp_uperm_goal_is_zero() {
        let t = build_tables();
        let p = PruningTables::build(&t);
        assert_eq!(p.p2_cp_uperm_at(0, 0), 0);
    }

    #[test]
    fn pruning_p1_co_udslice_full_coverage() {
        // The Phase 1 (co, udslice) BFS should reach every reachable
        // (co, udslice) pair in a small number of moves (≤ 12 standard).
        let t = build_tables();
        let p = PruningTables::build(&t);
        let max = p.p1_co_udslice.iter().filter(|&&d| d != UNVISITED).max().unwrap();
        assert!(*max <= 12, "Phase 1 (co, udslice) max distance > 12: {}", max);
    }

    #[test]
    fn pruning_table_admissible_tables_full() {
        // Every Phase-1 and Phase-2 pruning table must fully cover its
        // coord-pair space. The solver depends on every reachable cube
        // state's projection landing on a finite distance for the IDA*
        // heuristic to remain admissible.
        let t = build_tables();
        let p = PruningTables::build(&t);
        let p1_co_visited = p.p1_co_udslice.iter().filter(|&&d| d != UNVISITED).count();
        let p1_eo_visited = p.p1_eo_udslice.iter().filter(|&&d| d != UNVISITED).count();
        let p2_cp_visited = p.p2_cp_uperm.iter().filter(|&&d| d != UNVISITED).count();
        let p2_ep_visited = p.p2_ep_uperm.iter().filter(|&&d| d != UNVISITED).count();
        assert_eq!(p1_co_visited, p.p1_co_udslice.len(), "p1_co_udslice incomplete");
        assert_eq!(p1_eo_visited, p.p1_eo_udslice.len(), "p1_eo_udslice incomplete");
        assert_eq!(p2_cp_visited, p.p2_cp_uperm.len(), "p2_cp_uperm incomplete");
        assert_eq!(p2_ep_visited, p.p2_ep_uperm.len(), "p2_ep_uperm incomplete");
    }

    #[test]
    fn p1_eo_udslice_covers_every_reachable_state() {
        // Property: for any move sequence applied to solved, the resulting
        // (eo, udslice) lookup hits a visited cell. This is the
        // admissibility condition the heuristic relies on.
        use super::super::coords::{encode_eo, encode_udslice};
        use cube_core::{Cube, random_move_scramble};
        use rand::SeedableRng;
        use rand_chacha::ChaCha8Rng;
        let t = build_tables();
        let p = PruningTables::build(&t);
        let mut rng = ChaCha8Rng::seed_from_u64(0xEEDD_CC11);
        for _ in 0..200 {
            let scramble = random_move_scramble(3, 12, &mut rng);
            let mut cube = Cube::solved(3).unwrap();
            cube.apply_seq(&scramble).unwrap();
            let st = State3x3::from_cube(&cube);
            let eo = encode_eo(&st.edges) as usize;
            let ud = encode_udslice(&st.edges) as usize;
            assert_ne!(
                p.p1_eo_udslice[eo * N_UDSLICE + ud],
                UNVISITED,
                "reachable (eo={eo}, ud={ud}) missing from p1_eo_udslice",
            );
        }
    }
}
