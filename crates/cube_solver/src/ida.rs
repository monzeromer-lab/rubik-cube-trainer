//! Iterative-deepening A* search.
//!
//! IDA* is the workhorse of every solver phase: 2×2 (single phase), 3×3
//! two-phase (one IDA* per phase), 4×4/5×5 reduction (one per reduction
//! step). Linear memory, optimal when the heuristic is admissible.
//!
//! See plan §6.2.

use std::hash::Hash;

/// A search problem the IDA* engine can solve.
pub trait SearchProblem {
    /// Compactly encoded state. Must be cheaply cloneable.
    type State: Clone + Eq + Hash;
    /// A move in the problem's move set.
    type Move: Copy;

    fn is_goal(&self, state: &Self::State) -> bool;

    /// Admissible lower bound on the number of moves to reach goal.
    fn heuristic(&self, state: &Self::State) -> u8;

    /// All `(move, next_state)` neighbors of `state`.
    fn neighbors(&self, state: &Self::State)
        -> Box<dyn Iterator<Item = (Self::Move, Self::State)> + '_>;

    /// Skip moves that are trivially redundant after `last`. The default
    /// allows everything; override for face-move-style problems where
    /// e.g. `R R'` should never be searched.
    fn prune_redundant(&self, _last: Option<Self::Move>, _next: Self::Move) -> bool {
        false
    }
}

/// Run IDA* up to `max_depth`. Returns `None` if no solution within bound.
pub fn ida_star<P: SearchProblem>(
    problem: &P,
    start: P::State,
    max_depth: u8,
) -> Option<Vec<P::Move>> {
    let mut bound = problem.heuristic(&start);
    if bound > max_depth {
        return None;
    }
    loop {
        let mut path: Vec<P::Move> = Vec::new();
        match dfs(problem, &start, &mut path, 0, bound, None) {
            DfsOutcome::Found => return Some(path),
            DfsOutcome::NewBound(b) => {
                if b > max_depth {
                    return None;
                }
                bound = b;
            }
            DfsOutcome::Exhausted => return None,
        }
    }
}

enum DfsOutcome {
    Found,
    NewBound(u8),
    Exhausted,
}

fn dfs<P: SearchProblem>(
    problem: &P,
    state: &P::State,
    path: &mut Vec<P::Move>,
    g: u8,
    bound: u8,
    last_move: Option<P::Move>,
) -> DfsOutcome {
    let h = problem.heuristic(state);
    let f = g.saturating_add(h);
    if f > bound {
        return DfsOutcome::NewBound(f);
    }
    if problem.is_goal(state) {
        return DfsOutcome::Found;
    }
    let mut min_next_bound: u8 = u8::MAX;
    for (m, next) in problem.neighbors(state) {
        if problem.prune_redundant(last_move, m) {
            continue;
        }
        path.push(m);
        match dfs(problem, &next, path, g + 1, bound, Some(m)) {
            DfsOutcome::Found => return DfsOutcome::Found,
            DfsOutcome::NewBound(b) => {
                if b < min_next_bound {
                    min_next_bound = b;
                }
            }
            DfsOutcome::Exhausted => {}
        }
        path.pop();
    }
    if min_next_bound == u8::MAX {
        DfsOutcome::Exhausted
    } else {
        DfsOutcome::NewBound(min_next_bound)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A trivial search: walk on integers, goal is to reach `target`,
    /// moves are +1 / -1 / +5 / -5. Heuristic = |state - target| / 5
    /// (admissible since +5 is the largest step).
    struct WalkProblem {
        target: i32,
    }

    impl SearchProblem for WalkProblem {
        type State = i32;
        type Move = i32;

        fn is_goal(&self, state: &i32) -> bool {
            *state == self.target
        }

        fn heuristic(&self, state: &i32) -> u8 {
            ((self.target - state).unsigned_abs() / 5).min(u8::MAX as u32) as u8
        }

        fn neighbors(&self, state: &i32) -> Box<dyn Iterator<Item = (i32, i32)> + '_> {
            let s = *state;
            Box::new([1, -1, 5, -5].into_iter().map(move |d| (d, s + d)))
        }
    }

    #[test]
    fn walks_to_target() {
        let p = WalkProblem { target: 23 };
        let path = ida_star(&p, 0, 30).expect("must find a path");
        assert_eq!(path.iter().sum::<i32>(), 23);
        // Optimal: 23 = 4*5 + 3*1 = 7 moves.
        assert_eq!(path.len(), 7);
    }

    #[test]
    fn already_at_goal() {
        let p = WalkProblem { target: 0 };
        let path = ida_star(&p, 0, 5).unwrap();
        assert!(path.is_empty());
    }

    #[test]
    fn returns_none_when_depth_exceeded() {
        let p = WalkProblem { target: 1000 };
        assert!(ida_star(&p, 0, 5).is_none());
    }
}
