//! Property tests for the invariants listed in §3.6 of the plan.

use cube_core::{Cube, Face, Facelets, Move, MoveSeq, Turn};
use proptest::prelude::*;
use rand::SeedableRng;
use rand_chacha::ChaCha8Rng;

fn arb_size() -> impl Strategy<Value = u8> {
    prop_oneof![Just(2u8), Just(3u8), Just(4u8), Just(5u8)]
}

fn arb_face() -> impl Strategy<Value = Face> {
    prop_oneof![
        Just(Face::U), Just(Face::D), Just(Face::L),
        Just(Face::R), Just(Face::F), Just(Face::B),
    ]
}

fn arb_turn() -> impl Strategy<Value = Turn> {
    prop_oneof![Just(Turn::Cw), Just(Turn::Ccw), Just(Turn::Half)]
}

fn arb_move(size: u8) -> impl Strategy<Value = Move> {
    let depths = 1u8..=size;
    (arb_face(), arb_turn(), depths, any::<bool>()).prop_map(|(f, t, d, w)| Move {
        face: f, turn: t, depth: d, wide: w,
    })
}

fn arb_seq(size: u8, max_len: usize) -> impl Strategy<Value = MoveSeq> {
    prop::collection::vec(arb_move(size), 0..=max_len).prop_map(MoveSeq::from_vec)
}

proptest! {
    #![proptest_config(ProptestConfig {
        cases: 256,
        .. ProptestConfig::default()
    })]

    #[test]
    fn seq_then_inverse_is_identity(size in arb_size(), seq_seed in any::<u64>()) {
        let mut rng = ChaCha8Rng::seed_from_u64(seq_seed);
        let len = (rng.random::<u32>() % 60) as usize;
        let seq = cube_core::random_move_scramble(size, len, &mut rng);
        let mut cube = Cube::solved(size).unwrap();
        cube.apply_seq(&seq).unwrap();
        cube.apply_seq(&seq.inverse()).unwrap();
        prop_assert!(cube.is_solved());
    }

    #[test]
    fn cubie_count_invariant(size in arb_size(), seq in arb_seq(3, 60)) {
        // Use seq with depth ≤ 3, fits any cube ≥ 3.
        prop_assume!(size >= 3);
        let mut cube = Cube::solved(size).unwrap();
        let count = cube.cubie_count();
        // Filter out moves with depth > size (proptest may generate them).
        for m in seq.0 {
            if m.depth <= size {
                cube.apply(m).unwrap();
            }
        }
        prop_assert_eq!(cube.cubie_count(), count);
    }

    #[test]
    fn cube_to_facelets_round_trip(size in arb_size(), seq_seed in any::<u64>()) {
        let mut rng = ChaCha8Rng::seed_from_u64(seq_seed);
        let len = (rng.random::<u32>() % 40) as usize;
        let seq = cube_core::random_move_scramble(size, len, &mut rng);
        let mut cube = Cube::solved(size).unwrap();
        cube.apply_seq(&seq).unwrap();
        let f = Facelets::from_cube(&cube);
        let back = f.try_into_cube().unwrap();
        // The reconstructed cube must yield the same sticker view.
        prop_assert_eq!(Facelets::from_cube(&back), f);
    }

    #[test]
    fn parser_round_trip(size in arb_size(), seq_seed in any::<u64>()) {
        let mut rng = ChaCha8Rng::seed_from_u64(seq_seed);
        let len = (rng.random::<u32>() % 30) as usize;
        let seq = cube_core::random_move_scramble(size, len, &mut rng);
        let printed = seq.to_string();
        let parsed = MoveSeq::parse(&printed, size).unwrap();
        prop_assert_eq!(parsed, seq);
    }

    #[test]
    fn four_quarter_turns_same_face_is_identity(face in arb_face()) {
        let mut cube = Cube::solved(3).unwrap();
        for _ in 0..4 {
            cube.apply(Move::face(face, Turn::Cw)).unwrap();
        }
        prop_assert!(cube.is_solved());
    }

    #[test]
    fn two_half_turns_same_face_is_identity(face in arb_face()) {
        let mut cube = Cube::solved(3).unwrap();
        let m = Move::face(face, Turn::Half);
        cube.apply(m).unwrap();
        cube.apply(m).unwrap();
        prop_assert!(cube.is_solved());
    }

    #[test]
    fn solved_facelets_consistent_per_face(size in arb_size()) {
        let f = Facelets::solved(size);
        let n = size as usize;
        for face in Face::ALL {
            let first = f.get(face, 0, 0);
            for row in 0..n {
                for col in 0..n {
                    prop_assert_eq!(f.get(face, row as u8, col as u8), first);
                }
            }
        }
    }
}

/// 10,000-move stress test: §1 success criterion ("zero state corruption").
/// Ignored from default `cargo test`; run with `--ignored` for the heavy check.
#[test]
#[ignore]
fn ten_thousand_moves_zero_corruption() {
    let mut rng = ChaCha8Rng::seed_from_u64(0xDEAD_BEEF);
    for size in 2..=5u8 {
        let mut cube = Cube::solved(size).unwrap();
        let count_before = cube.cubie_count();
        let seq = cube_core::random_move_scramble(size, 10_000, &mut rng);
        cube.apply_seq(&seq).unwrap();
        assert_eq!(cube.cubie_count(), count_before);
        // Round-trip via facelets: state remains internally consistent.
        let f = Facelets::from_cube(&cube);
        let back = f.try_into_cube().unwrap();
        assert_eq!(Facelets::from_cube(&back), f);
        // Apply inverse: returns to solved.
        cube.apply_seq(&seq.inverse()).unwrap();
        assert!(cube.is_solved());
    }
}
