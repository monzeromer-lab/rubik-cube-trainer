//! 3×3 two-phase solver. See plan §6.4.

pub mod coords;
pub mod moves;
pub mod solver;
pub mod state;
pub mod tables;

pub use solver::{Solve3x3Error, Solver3x3};
pub use state::State3x3;
