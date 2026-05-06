//! End-to-end correctness for the 2×2 solver.
//!
//! Per the §14.4 verification plan, the solver-correctness tests are the
//! mandatory gate before M2 closes. The cheap scramble-and-solve test runs
//! every `cargo test`; the 10K-scramble stress test is `--ignored` to keep
//! CI fast.

use cube_core::{Cube, Face, Move, MoveSeq, Turn};
use cube_solver::Solver2x2;
use rand::SeedableRng;
use rand::seq::IndexedRandom;
use rand::Rng;
use rand_chacha::ChaCha8Rng;

/// 2×2 scrambles use only the R/U/F faces (L/D/B are equivalent up to
/// whole-cube rotation), which keeps the DBL reference corner at home —
/// exactly what `Solver2x2` requires.
fn ruf_scramble<R: Rng + ?Sized>(rng: &mut R, length: usize) -> MoveSeq {
    let faces = [Face::R, Face::U, Face::F];
    let turns = [Turn::Cw, Turn::Half, Turn::Ccw];
    let mut moves: Vec<Move> = Vec::with_capacity(length);
    while moves.len() < length {
        let mut candidates: Vec<Face> = faces.to_vec();
        if let Some(prev) = moves.last() {
            candidates.retain(|f| *f != prev.face);
        }
        let face = *candidates.choose(rng).unwrap();
        let turn = *turns.choose(rng).unwrap();
        moves.push(Move::face(face, turn));
    }
    MoveSeq::from_vec(moves)
}

fn shared_solver() -> &'static Solver2x2 {
    static ONCE: std::sync::OnceLock<Solver2x2> = std::sync::OnceLock::new();
    ONCE.get_or_init(Solver2x2::new)
}

#[test]
fn one_thousand_scrambles_all_solve() {
    let solver = shared_solver();
    let mut rng = ChaCha8Rng::seed_from_u64(0x00C0_FFEE_0202_0202);
    for i in 0..1000 {
        let scramble = ruf_scramble(&mut rng, 14);
        let mut cube = Cube::solved(2).unwrap();
        cube.apply_seq(&scramble).unwrap();
        let solution = solver.solve(&cube).expect("must solve");
        // Every solution must be ≤ god's number = 11.
        assert!(solution.len() <= 11, "iter {i}: solution exceeds god's number ({})", solution.len());
        cube.apply_seq(&solution).unwrap();
        assert!(cube.is_solved(), "iter {i}: scramble {scramble} → {solution} did not solve");
    }
}

#[test]
#[ignore]
fn ten_thousand_scrambles_all_solve_optimally() {
    let solver = shared_solver();
    let mut rng = ChaCha8Rng::seed_from_u64(0xDEAD_BEEF_0202_0202);
    let mut total_moves: usize = 0;
    let mut max_solve: usize = 0;
    for i in 0..10_000 {
        let scramble = ruf_scramble(&mut rng, 25);
        let mut cube = Cube::solved(2).unwrap();
        cube.apply_seq(&scramble).unwrap();
        let solution = solver.solve(&cube).expect("must solve");
        cube.apply_seq(&solution).unwrap();
        assert!(cube.is_solved(), "iter {i}: did not solve");
        total_moves += solution.len();
        max_solve = max_solve.max(solution.len());
    }
    let avg = total_moves as f64 / 10_000.0;
    eprintln!("avg solve length: {avg:.2} moves; max: {max_solve}");
    // Statistical sanity: average optimal 2×2 solve length is ~8.5.
    assert!(avg < 9.5);
    assert!(max_solve <= 11);
}
