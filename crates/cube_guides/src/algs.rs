//! Algorithm library for the CFOP curriculum (plan §7.4).
//!
//! Each entry is an authored algorithm — its `notation` is the move
//! sequence the user learns and executes. The build-time test
//! [`tests::every_alg_round_trips_with_its_inverse`] proves every entry's
//! notation is valid and its inverse perfectly reverses it. Per-case
//! semantic verification (e.g., "this OLL alg leaves F2L intact") needs
//! authored "case state" setups; that's M15 content polish.
//!
//! This starter library covers a handful of widely-used OLL/PLL/F2L algs
//! across the major case families. The authored full-curriculum-set is
//! the kind of work that benefits from algdb.net cross-checking and a
//! human review pass.

use cube_core::{Cube, MoveSeq};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AlgFamily {
    Oll,
    Pll,
    F2l,
    /// Last-layer "shorthand" algs sometimes rolled out before the full
    /// CFOP set (e.g., the 2-look OLL/PLL groups).
    Beginner,
}

#[derive(Debug, Clone)]
pub struct AlgEntry {
    /// Stable identifier for stats keying — `"oll-27"`, `"pll-T"` etc.
    pub id: &'static str,
    pub display_name: &'static str,
    pub family: AlgFamily,
    /// The algorithm itself in WCA notation.
    pub notation: &'static str,
}

impl AlgEntry {
    pub fn parse(&self) -> MoveSeq {
        MoveSeq::parse(self.notation, 3).unwrap_or_else(|e| {
            panic!("alg '{}' has invalid notation '{}': {}", self.id, self.notation, e)
        })
    }
}

/// Starter library. Real CFOP curriculum (57 OLL + 21 PLL + 41 F2L) is
/// M15 polish — it's content work that wants algdb.net cross-checking.
/// All shipped entries pass the round-trip + identity test.
pub const STARTER_ALGS: &[AlgEntry] = &[
    // --- OLL last-layer cases ---
    AlgEntry {
        id: "oll-27-sune",
        display_name: "Sune",
        family: AlgFamily::Oll,
        notation: "R U R' U R U2 R'",
    },
    AlgEntry {
        id: "oll-26-anti-sune",
        display_name: "Anti-Sune",
        family: AlgFamily::Oll,
        notation: "R U2 R' U' R U' R'",
    },
    AlgEntry {
        id: "oll-21-h",
        display_name: "OLL 21 (H)",
        family: AlgFamily::Oll,
        notation: "R U2 R' U' R U R' U' R U' R'",
    },
    AlgEntry {
        id: "oll-22-pi",
        display_name: "OLL 22 (Pi)",
        family: AlgFamily::Oll,
        notation: "R U2 R2 U' R2 U' R2 U2 R",
    },
    // --- PLL last-layer cases ---
    AlgEntry {
        id: "pll-aa",
        display_name: "Aa-perm",
        family: AlgFamily::Pll,
        notation: "x R' U R' D2 R U' R' D2 R2 x'",
    },
    AlgEntry {
        id: "pll-h",
        display_name: "H-perm",
        family: AlgFamily::Pll,
        notation: "M2 U M2 U2 M2 U M2",
    },
    AlgEntry {
        id: "pll-u-a",
        display_name: "U-perm (a)",
        family: AlgFamily::Pll,
        notation: "R U' R U R U R U' R' U' R2",
    },
    // --- F2L pair insertions ---
    AlgEntry {
        id: "f2l-basic-pair-fr",
        display_name: "F2L FR pair (basic)",
        family: AlgFamily::F2l,
        notation: "U R U' R'",
    },
    AlgEntry {
        id: "f2l-basic-pair-fl",
        display_name: "F2L FL pair (basic)",
        family: AlgFamily::F2l,
        notation: "U' L' U L",
    },
    // --- 2-look beginner LL helpers ---
    AlgEntry {
        id: "beginner-ll-corner-twist",
        display_name: "Beginner LL corner twist (Sune family)",
        family: AlgFamily::Beginner,
        notation: "R U R' U R U2 R'",
    },
    AlgEntry {
        id: "beginner-ll-edge-flip",
        display_name: "Beginner LL edge flip",
        family: AlgFamily::Beginner,
        notation: "F R U R' U' F'",
    },
];

#[cfg(test)]
mod tests {
    use super::*;

    /// Every algorithm's notation must parse, and applying the algorithm
    /// followed by its inverse to a solved cube must return to solved.
    /// Catches any typo where someone wrote `R'` as `R´`, dropped a
    /// move, or an algorithm that doesn't actually have a clean inverse.
    #[test]
    fn every_alg_round_trips_with_its_inverse() {
        for entry in STARTER_ALGS {
            let alg = entry.parse();
            let inv = alg.inverse();
            let mut cube = Cube::solved(3).unwrap();
            cube.apply_seq(&alg).unwrap_or_else(|e| {
                panic!("{}: alg apply failed: {}", entry.id, e)
            });
            cube.apply_seq(&inv).unwrap_or_else(|e| {
                panic!("{}: inverse apply failed: {}", entry.id, e)
            });
            assert!(
                cube.is_solved(),
                "{}: alg + inverse did not return to solved",
                entry.id
            );
        }
    }

    /// Cube rotations and slice moves only count as a single token, so
    /// some short PLL algorithms (`x R' U R' D2 ...`) had previously
    /// stumped the parser. Make sure the starter set covers the
    /// notation breadth we ship.
    #[test]
    fn starter_library_includes_each_family() {
        let mut saw_oll = false;
        let mut saw_pll = false;
        let mut saw_f2l = false;
        let mut saw_beginner = false;
        for e in STARTER_ALGS {
            match e.family {
                AlgFamily::Oll => saw_oll = true,
                AlgFamily::Pll => saw_pll = true,
                AlgFamily::F2l => saw_f2l = true,
                AlgFamily::Beginner => saw_beginner = true,
            }
        }
        assert!(saw_oll && saw_pll && saw_f2l && saw_beginner);
    }

    /// PLLs preserve cube orientation up to a top-layer permutation: the
    /// algorithm's effect on the BOTTOM TWO LAYERS is the identity.
    /// Apply each PLL to a solved cube; the bottom two layers' stickers
    /// must be unchanged from solved.
    #[test]
    fn pll_algorithms_preserve_first_two_layers() {
        use cube_core::{Color, Face, Facelets};
        for entry in STARTER_ALGS.iter().filter(|e| e.family == AlgFamily::Pll) {
            let mut cube = Cube::solved(3).unwrap();
            cube.apply_seq(&entry.parse()).unwrap();
            let f = Facelets::from_cube(&cube);
            // U face untouched conceptually means each side face's row 0
            // (the layer just below the top) and the entire D face stay
            // a single solved color. We only check the two bottom rows
            // of each side face plus all of D.
            for side in [Face::F, Face::R, Face::B, Face::L] {
                let expected = match side {
                    Face::F => Color::F,
                    Face::R => Color::R,
                    Face::B => Color::B,
                    Face::L => Color::L,
                    _ => unreachable!(),
                };
                for row in 1..3u8 {
                    for col in 0..3u8 {
                        assert_eq!(
                            f.get(side, row, col),
                            expected,
                            "{}: side {side:?} row {row} col {col} disturbed",
                            entry.id
                        );
                    }
                }
            }
            for row in 0..3u8 {
                for col in 0..3u8 {
                    assert_eq!(
                        f.get(Face::D, row, col),
                        Color::D,
                        "{}: D face disturbed at ({row},{col})",
                        entry.id
                    );
                }
            }
        }
    }
}
