//! End-to-end correctness for the 3×3 two-phase solver.
//!
//! Per §14.4 of the plan: "100,000 random scrambles, every one solves" is
//! the eventual gate. We ratchet toward that as the solver matures; today
//! we cover every single-face turn (mandatory) and a sample of random
//! multi-move scrambles (regression for the rotation-tracking EO fix).

use cube_core::{Cube, Face, Facelets, Move, MoveSeq, Turn, random_move_scramble};
use cube_solver::Solver3x3;
use rand::SeedableRng;
use rand_chacha::ChaCha8Rng;

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

/// Random multi-move scrambles. Pins down the rotation-tracking EO model:
/// the previous additive bit-XOR delta model produced odd EO parity on
/// ~45% of length-8 scrambles, which made `is_in_g1` lie and the solver
/// either hang or return non-solving sequences.
#[test]
fn random_multi_move_scrambles_solve() {
    let solver = shared_solver();
    let mut rng = ChaCha8Rng::seed_from_u64(0x515f3);
    for trial in 0..30 {
        let scramble = random_move_scramble(3, 12, &mut rng);
        let mut cube = Cube::solved(3).unwrap();
        cube.apply_seq(&scramble).unwrap();
        let solution = solver.solve(&cube).unwrap_or_else(|e| {
            panic!("trial {trial} scramble {scramble} → {e}");
        });
        cube.apply_seq(&solution).unwrap();
        let f = Facelets::from_cube(&cube);
        assert_eq!(f, Facelets::solved(3),
            "trial {trial} scramble {scramble} → solution {solution} did not visually solve");
    }
}
