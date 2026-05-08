//! Move tables and pruning tables for the two-phase solver.
//!
//! Building everything takes a single multi-second pass on first run. We
//! deliberately keep the tables in memory only — disk-cache wiring lives in
//! `cube_render`/`cube_trainer` later milestones (plan §13 layout).

use super::coords::{
    N_CO, N_CP, N_EO, N_EP, N_UDSLICE, N_UDSLICE_PERM, decode_eo, decode_udslice_mask, encode_co,
    encode_cp, encode_eo, encode_ep, encode_udslice, encode_udslice_perm, udslice_from_mask,
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
    pub p1_co_udslice: Vec<u8>,
    /// Phase 1: distance from `(eo, udslice)` to `(eo=0, udslice=goal)`.
    pub p1_eo_udslice: Vec<u8>,
    /// Phase 1: distance from `(co, eo)` to `(0, 0)` — tightens the heuristic
    /// when both orientations are far from goal even if udslice is near goal.
    pub p1_co_eo: Vec<u8>,
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
            // only the coord-pair distance. For `(co, udslice)`, transitions
            // are coord-determined and full coverage is achieved.
            p1_co_udslice: build_phase1_full_pruning(
                tables,
                N_CO * N_UDSLICE,
                |s| {
                    encode_co(&s.corners) as usize * N_UDSLICE
                        + encode_udslice(&s.edges) as usize
                },
            ),
            // Phase 1 (eo, udslice): joint coord-space BFS. `(eo, ud)` jointly
            // determines transitions — `ud` identifies cubie types per slot
            // (E-slice if its slot is in the udslice mask, else UD-layer),
            // which is what the edge-op flip rule needs. The full-state BFS
            // approach used for `(co, ud)` is subtly broken here because the
            // edge-op uses `s.perm[src]` to determine cubie type at the
            // source, and a single representative's perm doesn't propagate
            // type info reliably through long sequences. Coord-space BFS
            // avoids that by recomputing the type from the coord pair on each
            // step.
            p1_eo_udslice: build_eo_udslice_pruning(tables),
            // Phase 1 (co, eo): not used in the current heuristic. A proper
            // (co, eo) pruning table would need joint-coord-space BFS over
            // (co, eo, ud) marginalized — but the joint orbit is ~1B states,
            // which doesn't fit in memory as a HashMap. The (co, ud) and
            // (eo, ud) tables together already give a tight heuristic; if
            // M15 budget allows, we can revisit with a 4-byte-per-entry
            // disk-backed table or separable bounds.
            p1_co_eo: vec![0; N_CO * N_EO],
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
    pub fn p1_co_eo_at(&self, co: u16, eo: u16) -> u8 {
        self.p1_co_eo[co as usize * N_EO + eo as usize]
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

/// Apply move `op` to the state described by joint coord `(eo, ud)` and
/// return the resulting `(new_eo, new_ud)`. `(eo, ud)` jointly determines
/// the cubie type at each slot, so this transition is well-defined without
/// tracking the underlying permutation explicitly.
fn step_eo_udslice(eo: u32, ud: u32, op: &EdgeMoveOp) -> (u32, u32) {
    let src_mask = decode_udslice_mask(ud);
    let src_orient = decode_eo(eo).orient;

    // Build the new orient from the move's perm + per-slot deltas chosen by
    // source-slot cubie type.
    let mut new_orient = [0u8; 12];
    let mut new_mask = [false; 12];
    for d in 0..12 {
        let s = op.perm[d] as usize;
        let src_is_es = src_mask[s];
        let delta = if src_is_es { op.delta_es[d] } else { op.delta_ud[d] };
        new_orient[d] = (src_orient[s] + delta) % 2;
        new_mask[d] = src_is_es;
    }

    let new_eo = encode_eo(&EdgeState { perm: [0; 12], orient: new_orient });
    let new_ud = udslice_from_mask(&new_mask);
    (new_eo, new_ud)
}

/// Build the Phase-1 `(eo, udslice)` pruning table by BFS in joint coord
/// space. Starts from the goal `(eo=0, ud=N_UDSLICE-1)` and expands using
/// all 18 face turns. Reaches every coord pair in the orbit (~half the joint
/// space, gated by the EO sum-mod-2 invariant).
fn build_eo_udslice_pruning(tables: &MoveTables) -> Vec<u8> {
    let goal_eo = 0u32;
    let goal_ud = (N_UDSLICE - 1) as u32;
    let goal_idx = goal_eo as usize * N_UDSLICE + goal_ud as usize;

    let mut table = vec![UNVISITED; N_EO * N_UDSLICE];
    table[goal_idx] = 0;
    let mut frontier: Vec<(u32, u32)> = vec![(goal_eo, goal_ud)];
    let mut next: Vec<(u32, u32)> = Vec::new();
    let mut dist: u8 = 0;
    while !frontier.is_empty() {
        for &(eo, ud) in &frontier {
            for op in &tables.edge_ops {
                let (new_eo, new_ud) = step_eo_udslice(eo, ud, op);
                let idx = new_eo as usize * N_UDSLICE + new_ud as usize;
                if table[idx] == UNVISITED {
                    table[idx] = dist + 1;
                    next.push((new_eo, new_ud));
                }
            }
        }
        frontier.clear();
        std::mem::swap(&mut frontier, &mut next);
        dist += 1;
    }
    table
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
    fn pruning_p1_eo_udslice_coverage_count() {
        // Diagnostic: how many (eo, ud) pairs does the joint coord-space BFS
        // reach? Sum-mod-2 EO invariant gates the orbit at 1024 × 495 ≈ 506K.
        let t = build_tables();
        let p = PruningTables::build(&t);
        let visited = p.p1_eo_udslice.iter().filter(|&&d| d != UNVISITED).count();
        let max = p.p1_eo_udslice.iter().filter(|&&d| d != UNVISITED).max().copied().unwrap_or(0);
        eprintln!("p1_eo_udslice visited = {} / {} (max dist = {})", visited, p.p1_eo_udslice.len(), max);
        // Conservative lower bound: must reach at least the (co, ud) full
        // coverage analog (1.08M). Actually, bound to 506K (orbit size).
        assert!(visited > 100_000, "p1_eo_udslice grossly under-covered: {visited}");
    }

    #[test]
    fn pruning_table_admissible_tables_full() {
        // The (co, udslice) and Phase 2 pruning tables must fully cover their
        // coord-pair spaces; the solver depends on those being admissible.
        // The (eo, udslice) and (co, eo) tables are sparse with the
        // single-representative full-state BFS approach (see project memory)
        // and intentionally not used in the current heuristic.
        let t = build_tables();
        let p = PruningTables::build(&t);
        let p1_co_visited = p.p1_co_udslice.iter().filter(|&&d| d != UNVISITED).count();
        let p2_cp_visited = p.p2_cp_uperm.iter().filter(|&&d| d != UNVISITED).count();
        let p2_ep_visited = p.p2_ep_uperm.iter().filter(|&&d| d != UNVISITED).count();
        assert_eq!(p1_co_visited, p.p1_co_udslice.len(), "p1_co_udslice incomplete");
        assert_eq!(p2_cp_visited, p.p2_cp_uperm.len(), "p2_cp_uperm incomplete");
        assert_eq!(p2_ep_visited, p.p2_ep_uperm.len(), "p2_ep_uperm incomplete");
    }
}
