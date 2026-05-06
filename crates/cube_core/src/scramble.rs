//! Scramble generation. M1 ships a random-move generator obeying the
//! WCA "no consecutive same-face / no same-axis sandwich" rule. The
//! random-state generator for 2×2 and 3×3 from §8.2 of the plan needs
//! the solver and is wired in from M2/M3.

use rand::Rng;
use rand::seq::IndexedRandom;

use crate::moves::{Face, Move, MoveSeq, Turn};

/// Default WCA-style scramble length per cube size. The 2×2/3×3 numbers are
/// provisional pending the solver-driven random-state generator.
pub fn default_length(size: u8) -> usize {
    match size {
        2 => 11,
        3 => 25,
        4 => 40,
        5 => 60,
        _ => 0,
    }
}

/// Generate a WCA-style scramble appropriate for the cube size.
pub fn wca_scramble<R: Rng + ?Sized>(size: u8, rng: &mut R) -> MoveSeq {
    random_move_scramble(size, default_length(size), rng)
}

/// Generate a random-move scramble of the given length. Avoids two
/// consecutive moves on the same face, and the "X Y X" same-axis sandwich.
pub fn random_move_scramble<R: Rng + ?Sized>(size: u8, length: usize, rng: &mut R) -> MoveSeq {
    let alphabet = scramble_alphabet(size);
    let turns = [Turn::Cw, Turn::Ccw, Turn::Half];
    let mut moves: Vec<Move> = Vec::with_capacity(length);

    while moves.len() < length {
        let mut candidates = alphabet.clone();
        // Disallow same-face-as-previous.
        if let Some(prev) = moves.last() {
            candidates.retain(|m| m.face != prev.face);
        }
        // Disallow same-axis sandwich (e.g., R L R).
        if moves.len() >= 2 {
            let prev = moves[moves.len() - 1];
            let prev2 = moves[moves.len() - 2];
            if prev.face.axis() == prev2.face.axis() {
                candidates.retain(|m| m.face.axis() != prev.face.axis());
            }
        }
        let chosen = candidates.choose(rng).expect("alphabet always has options");
        let turn = *turns.choose(rng).unwrap();
        moves.push(Move { turn, ..*chosen });
    }
    MoveSeq(moves)
}

/// Distinct (face, depth, wide) move templates used in scrambles for `size`.
fn scramble_alphabet(size: u8) -> Vec<Move> {
    let mut out: Vec<Move> = Face::ALL
        .iter()
        .map(|&f| Move::face(f, Turn::Cw))
        .collect();
    if size >= 4 {
        for f in Face::ALL {
            out.push(Move { face: f, turn: Turn::Cw, depth: 2, wide: true });
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cube::Cube;
    use rand::SeedableRng;
    use rand_chacha::ChaCha8Rng;

    fn rng() -> ChaCha8Rng {
        ChaCha8Rng::seed_from_u64(0xC0FFEE)
    }

    #[test]
    fn scramble_has_expected_length() {
        let mut r = rng();
        for size in 2..=5u8 {
            let seq = wca_scramble(size, &mut r);
            assert_eq!(seq.len(), default_length(size));
        }
    }

    #[test]
    fn scramble_no_consecutive_same_face() {
        let mut r = rng();
        let seq = wca_scramble(3, &mut r);
        for w in seq.0.windows(2) {
            assert_ne!(w[0].face, w[1].face);
        }
    }

    #[test]
    fn scramble_no_axis_sandwich() {
        let mut r = rng();
        let seq = wca_scramble(3, &mut r);
        for w in seq.0.windows(3) {
            if w[0].face.axis() == w[1].face.axis() {
                assert_ne!(w[2].face.axis(), w[0].face.axis(),
                    "axis sandwich at {:?} {:?} {:?}", w[0], w[1], w[2]);
            }
        }
    }

    #[test]
    fn scramble_applies_cleanly_to_every_size() {
        let mut r = rng();
        for size in 2..=5u8 {
            let mut cube = Cube::solved(size).unwrap();
            let seq = wca_scramble(size, &mut r);
            cube.apply_seq(&seq).unwrap();
            // Cubie count invariant.
            assert_eq!(cube.cubie_count(), Cube::solved(size).unwrap().cubie_count());
        }
    }

    #[test]
    fn scramble_inverse_returns_to_solved() {
        let mut r = rng();
        for size in 2..=5u8 {
            let mut cube = Cube::solved(size).unwrap();
            let seq = wca_scramble(size, &mut r);
            cube.apply_seq(&seq).unwrap();
            cube.apply_seq(&seq.inverse()).unwrap();
            assert!(cube.is_solved());
        }
    }
}
