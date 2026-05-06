//! Generates and prints a scramble + the resulting facelet view for each
//! cube size. Useful for eyeballing the domain model before any UI exists.
//!
//! Run with: `cargo run -p cube_core --example scramble_demo`

use cube_core::{Cube, Facelets, Face, MoveSeq, wca_scramble};
use rand::SeedableRng;
use rand_chacha::ChaCha8Rng;

fn main() {
    let mut rng = ChaCha8Rng::seed_from_u64(seed_from_args());
    for size in 2..=5u8 {
        println!("=== {size}×{size} ===");
        let scramble = wca_scramble(size, &mut rng);
        println!("Scramble ({} moves): {}", scramble.len(), scramble);
        let mut cube = Cube::solved(size).unwrap();
        cube.apply_seq(&scramble).unwrap();
        let facelets = Facelets::from_cube(&cube);
        print_facelets(&facelets);
        // Inverse round-trip sanity check.
        let mut roundtrip = cube.clone();
        roundtrip.apply_seq(&scramble.inverse()).unwrap();
        assert!(roundtrip.is_solved(), "scramble inverse did not solve {size}×{size}");
        println!();
    }

    // Demo the parser+inverse on a famous identity.
    let sexy = MoveSeq::parse("R U R' U'", 3).unwrap();
    let mut cube = Cube::solved(3).unwrap();
    for _ in 0..6 {
        cube.apply_seq(&sexy).unwrap();
    }
    println!("(R U R' U')⁶ on 3×3 → solved: {}", cube.is_solved());
}

fn print_facelets(f: &Facelets) {
    let n = f.size as usize;
    let pad = " ".repeat(n + 1);
    // U face on top.
    for row in 0..n {
        print!("{pad}");
        for col in 0..n {
            print!("{}", f.get(Face::U, row as u8, col as u8).symbol());
        }
        println!();
    }
    // L F R B side bands.
    for row in 0..n {
        for face in [Face::L, Face::F, Face::R, Face::B] {
            for col in 0..n {
                print!("{}", f.get(face, row as u8, col as u8).symbol());
            }
            print!(" ");
        }
        println!();
    }
    // D face below.
    for row in 0..n {
        print!("{pad}");
        for col in 0..n {
            print!("{}", f.get(Face::D, row as u8, col as u8).symbol());
        }
        println!();
    }
}

fn seed_from_args() -> u64 {
    std::env::args()
        .nth(1)
        .and_then(|s| s.parse::<u64>().ok())
        .unwrap_or(0x00C0_FFEE_DEAD_BEEF)
}
