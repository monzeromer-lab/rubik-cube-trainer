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
    AlgEntry {
        id: "oll-33-t",
        display_name: "OLL 33 (T)",
        family: AlgFamily::Oll,
        notation: "R U R' U' R' F R F'",
    },
    AlgEntry {
        id: "oll-37-fish",
        display_name: "OLL 37 (Fish)",
        family: AlgFamily::Oll,
        notation: "F R' F' R U R U' R'",
    },
    AlgEntry {
        id: "oll-38",
        display_name: "OLL 38",
        family: AlgFamily::Oll,
        notation: "R U R' U R U' R' U' R' F R F'",
    },
    AlgEntry {
        id: "oll-44-p",
        display_name: "OLL 44 (P)",
        family: AlgFamily::Oll,
        notation: "F U R U' R' F'",
    },
    AlgEntry {
        id: "oll-45-t",
        display_name: "OLL 45 (T)",
        family: AlgFamily::Oll,
        notation: "F R U R' U' F'",
    },
    AlgEntry {
        id: "oll-48",
        display_name: "OLL 48",
        family: AlgFamily::Oll,
        notation: "F R U R' U' R U R' U' F'",
    },
    AlgEntry {
        id: "oll-51",
        display_name: "OLL 51",
        family: AlgFamily::Oll,
        notation: "F U R U' R' U R U' R' F'",
    },
    AlgEntry {
        id: "oll-56",
        display_name: "OLL 56",
        family: AlgFamily::Oll,
        notation: "R U R' U R U' R' U R U2 R'",
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
    AlgEntry {
        id: "pll-u-b",
        display_name: "U-perm (b)",
        family: AlgFamily::Pll,
        notation: "R2 U R U R' U' R' U' R' U R'",
    },
    AlgEntry {
        id: "pll-ab",
        display_name: "Ab-perm",
        family: AlgFamily::Pll,
        notation: "x R2 D2 R U R' D2 R U' R x'",
    },
    AlgEntry {
        id: "pll-z",
        display_name: "Z-perm",
        family: AlgFamily::Pll,
        notation: "M2 U M2 U M' U2 M2 U2 M'",
    },
    AlgEntry {
        id: "pll-t",
        display_name: "T-perm",
        family: AlgFamily::Pll,
        notation: "R U R' U' R' F R2 U' R' U' R U R' F'",
    },
    AlgEntry {
        id: "pll-ja",
        display_name: "Ja-perm",
        family: AlgFamily::Pll,
        notation: "R' U L' U2 R U' R' U2 R L",
    },
    AlgEntry {
        id: "pll-jb",
        display_name: "Jb-perm",
        family: AlgFamily::Pll,
        notation: "R U R' F' R U R' U' R' F R2 U' R'",
    },
    AlgEntry {
        id: "pll-f",
        display_name: "F-perm",
        family: AlgFamily::Pll,
        notation: "R' U' F' R U R' U' R' F R2 U' R' U' R U R' U R",
    },
    AlgEntry {
        id: "pll-y",
        display_name: "Y-perm",
        family: AlgFamily::Pll,
        notation: "F R U' R' U' R U R' F' R U R' U' R' F R F'",
    },
    AlgEntry {
        id: "pll-v",
        display_name: "V-perm",
        family: AlgFamily::Pll,
        // Speed-solving notation usually drops the closing `y'` since the
        // user doesn't mind the cube ending rotated; the closing `y'` is
        // appended here so the alg leaves the cube in its starting frame
        // (no net AUF), matching the bottom-two-layers test invariant.
        notation: "R' U R' U' y R' F' R2 U' R' U R' F R F y'",
    },
    AlgEntry {
        id: "pll-na",
        display_name: "Na-perm",
        family: AlgFamily::Pll,
        notation: "R U R' U R U R' F' R U R' U' R' F R2 U' R' U2 R U' R'",
    },
    AlgEntry {
        id: "pll-nb",
        display_name: "Nb-perm",
        family: AlgFamily::Pll,
        notation: "R' U R U' R' F' U' F R U R' F R' F' R U' R",
    },
    AlgEntry {
        id: "pll-e",
        display_name: "E-perm",
        family: AlgFamily::Pll,
        notation: "x' L' U L D' L' U' L D L' U' L D' L' U L D x",
    },
    AlgEntry {
        id: "pll-ra",
        display_name: "Ra-perm",
        family: AlgFamily::Pll,
        notation: "R U' R' U' R U R D R' U' R D' R' U2 R'",
    },
    AlgEntry {
        id: "pll-rb",
        display_name: "Rb-perm",
        family: AlgFamily::Pll,
        notation: "R' U2 R U2 R' F R U R' U' R' F' R2",
    },
    AlgEntry {
        id: "pll-ga",
        display_name: "Ga-perm",
        family: AlgFamily::Pll,
        notation: "R2 U R' U R' U' R U' R2 U' D R' U R D'",
    },
    AlgEntry {
        id: "pll-gb",
        display_name: "Gb-perm",
        family: AlgFamily::Pll,
        notation: "R' U' R U D' R2 U R' U R U' R U' R2 D",
    },
    AlgEntry {
        id: "pll-gc",
        display_name: "Gc-perm",
        family: AlgFamily::Pll,
        notation: "R2 U' R U' R U R' U R2 U D' R U' R' D",
    },
    AlgEntry {
        id: "pll-gd",
        display_name: "Gd-perm",
        family: AlgFamily::Pll,
        notation: "R U R' U' D R2 U' R U' R' U R' U R2 D'",
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

    /// OLLs operate only on the top layer — applying any OLL alg to a
    /// solved cube must leave the bottom two layers' stickers identical
    /// to solved (D face all D, side faces' bottom two rows their home
    /// color). Top layer can be in any orientation/permutation.
    #[test]
    fn oll_algorithms_preserve_first_two_layers() {
        use cube_core::{Color, Face, Facelets};
        for entry in STARTER_ALGS.iter().filter(|e| e.family == AlgFamily::Oll) {
            let mut cube = Cube::solved(3).unwrap();
            cube.apply_seq(&entry.parse()).unwrap();
            let f = Facelets::from_cube(&cube);
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
