//! End-to-end correctness for the 3×3 two-phase solver.
//!
//! Per §14.4 of the plan: "100,000 random scrambles, every one solves" is
//! the eventual gate. With the M3 (co, udslice)-only Phase-1 heuristic and
//! loose Phase-2 pruning, solve time grows quickly with scramble length —
//! deep random-scramble verification is M15 polish. What we pin down here
//! is the strongest property the solver must hold today: across every
//! single-move scramble of every face, the solution visually solves.

use cube_core::{Cube, Face, Facelets, Move, MoveSeq, Turn};
use cube_solver::Solver3x3;

fn shared_solver() -> &'static Solver3x3 {
    static ONCE: std::sync::OnceLock<Solver3x3> = std::sync::OnceLock::new();
    ONCE.get_or_init(Solver3x3::new)
}

#[test]
fn every_single_move_scramble_solves_visually() {
    let solver = shared_solver();
    for face in Face::ALL {
        for turn in [Turn::Cw, Turn::Half, Turn::Ccw] {
            let scramble = MoveSeq::from_vec(vec![Move::face(face, turn)]);
            let mut cube = Cube::solved(3).unwrap();
            cube.apply_seq(&scramble).unwrap();
            let solution = solver.solve(&cube).unwrap_or_else(|e| {
                panic!("scramble {scramble} → {e}");
            });
            cube.apply_seq(&solution).unwrap();
            let f = Facelets::from_cube(&cube);
            assert_eq!(f, Facelets::solved(3),
                "scramble {scramble} → solution {solution} did not visually solve");
        }
    }
}
