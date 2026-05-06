//! Custom Rubik's cube solvers.
//!
//! - [`ida`] — generic iterative-deepening A* search.
//! - [`two`] — 2×2 with full distance table; every solution is optimal.
//! - [`three`] — 3×3 two-phase Kociemba-style solver.
//! - [`four`] — 4×4 reduction-based solver, parity-aware for already-
//!   reduced inputs (full IDA* reduction is M15 polish).
//! - 5×5 reduction-based solver lives in M14.

pub mod four;
pub mod ida;
pub mod three;
pub mod two;

pub use four::{
    OLL_PARITY_ALG, PLL_PARITY_ALG, Solve4x4Error, Solver4x4, centers_solved, edges_paired,
    is_reduced, project_to_3x3,
};
pub use ida::{SearchProblem, ida_star};
pub use three::{Solve3x3Error, Solver3x3, State3x3};
pub use two::{NUM_2X2_STATES, Solve2x2Error, Solver2x2, State2x2};
