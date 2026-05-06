//! Progressive learning curriculum (plan §7).
//!
//! A `Lesson` is an ordered list of `Step`s. A `Step` is one of:
//! - `Explain` — text + cubie highlights + camera focus.
//! - `Demonstrate` — animate a setup + canonical algorithm.
//! - `Practice` — setup, then wait for `goal` to evaluate true.
//! - `Drill` — sub-loop over per-case timed solves.
//!
//! The trainer's step engine drives the lesson by advancing on user
//! input or, for `Practice`, when the goal predicate fires `true` against
//! the live `Cube` state.
//!
//! This crate ships the data model + serde + a `GoalPredicate` evaluator
//! that is pure-fn over `cube_core::Cube`. Full lesson content (the 8
//! beginner lessons + CFOP) is content-authoring work for M11/M12 polish.

use cube_core::{Color, Cube, Face, Facelets, MoveSeq};
use serde::{Deserialize, Serialize};

pub mod algs;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Lesson {
    pub id: String,
    pub title: String,
    pub prerequisites: Vec<String>,
    pub estimated_minutes: u32,
    pub steps: Vec<Step>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Step {
    Explain {
        text: String,
        /// Cubie home positions to highlight (as `(x, y, z)` integer
        /// triples). Optional — empty means no highlight.
        highlight: Vec<[i32; 3]>,
        camera_focus: Option<CameraPreset>,
    },
    Demonstrate {
        setup: String,     // notation, parsed at runtime
        algorithm: String, // notation
        narration: Vec<TimedText>,
        speed: AnimSpeed,
    },
    Practice {
        setup: String,           // notation
        goal: GoalPredicate,
        hint: Option<String>,    // notation for a known-good algorithm
    },
    Drill {
        cases: Vec<String>,      // drill case ids resolved by cube_trainer
        target_count: u32,
        max_seconds_per_case: Option<u32>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum CameraPreset {
    /// Default orbit camera, no special focus.
    Default,
    /// Look down at the U face.
    Top,
    /// Look at the F face.
    Front,
    /// Look at the corner between U/R/F.
    UrfCorner,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimedText {
    pub at_seconds: f32,
    pub text: String,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum AnimSpeed {
    Slow,
    Normal,
    Fast,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum GoalPredicate {
    WhiteCrossSolved,
    WhiteFaceSolved,
    F2LPairSolved {
        /// Which F2L slot the corner+edge pair must be solved in.
        slot: F2LSlot,
    },
    YellowCrossOriented,
    /// All top stickers same color (final OLL state).
    OllSolved,
    /// Cube fully solved (sticker view).
    Solved,
    /// Hook for content authors. Resolution happens via a registered
    /// closure in the step engine; unknown ids resolve to `false`.
    Custom(String),
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum F2LSlot {
    Fr,
    Fl,
    Br,
    Bl,
}

impl GoalPredicate {
    /// Evaluate the predicate against a live cube. Centers are assumed
    /// in their solved orientation (we read sticker colors via the
    /// `Facelets` view, which projects through the actual cubie state).
    pub fn evaluate(&self, cube: &Cube) -> bool {
        let f = Facelets::from_cube(cube);
        match self {
            GoalPredicate::Solved => cube.is_solved(),
            GoalPredicate::OllSolved => is_top_face_uniform(&f),
            GoalPredicate::WhiteCrossSolved => is_white_cross_solved(&f),
            GoalPredicate::WhiteFaceSolved => is_white_face_solved(&f, cube.size),
            GoalPredicate::F2LPairSolved { slot } => is_f2l_pair_solved(&f, *slot),
            GoalPredicate::YellowCrossOriented => is_yellow_cross_oriented(&f),
            GoalPredicate::Custom(_) => false,
        }
    }
}

fn is_top_face_uniform(f: &Facelets) -> bool {
    let n = f.size;
    let first = f.get(Face::U, 0, 0);
    (0..n).all(|r| (0..n).all(|c| f.get(Face::U, r, c) == first))
}

/// White cross on top: middle row of U is all white, plus the four
/// edge stickers on F/R/L/B match their face center. (`Color::U` is
/// "white" in the default scheme.)
fn is_white_cross_solved(f: &Facelets) -> bool {
    let n = f.size;
    if n < 3 {
        return false;
    }
    let mid = n / 2;
    // U face: center column + center row are all white.
    let center_col_ok = (0..n).all(|r| f.get(Face::U, r, mid) == Color::U);
    let center_row_ok = (0..n).all(|c| f.get(Face::U, mid, c) == Color::U);
    if !center_col_ok || !center_row_ok {
        return false;
    }
    // Side stickers of the 4 cross edges on the U face.
    f.get(Face::F, 0, mid) == Color::F
        && f.get(Face::R, 0, mid) == Color::R
        && f.get(Face::B, 0, mid) == Color::B
        && f.get(Face::L, 0, mid) == Color::L
}

/// White (U) face fully complete and the four side faces' top row matches
/// their centers — i.e., the entire first layer is solved.
fn is_white_face_solved(f: &Facelets, size: u8) -> bool {
    let n = size;
    let u_uniform = is_top_face_uniform(f);
    let side_top_row_correct = |face: Face, color: Color| -> bool {
        (0..n).all(|c| f.get(face, 0, c) == color)
    };
    u_uniform
        && side_top_row_correct(Face::F, Color::F)
        && side_top_row_correct(Face::R, Color::R)
        && side_top_row_correct(Face::B, Color::B)
        && side_top_row_correct(Face::L, Color::L)
}

/// A specific F2L slot is solved: the corner cubie + the matching edge
/// cubie are both at home with correct orientation. We check via
/// stickers — the corner shows the right three colors and the edge shows
/// its two side-face colors aligned with their centers.
fn is_f2l_pair_solved(f: &Facelets, slot: F2LSlot) -> bool {
    let n = f.size;
    if n < 3 {
        return false;
    }
    let mid = n / 2;
    let last = n - 1;
    match slot {
        F2LSlot::Fr => {
            // Corner DRF: D-face[0,last]=D, F-face[last,last]=F, R-face[last,0]=R.
            let corner = f.get(Face::D, 0, last) == Color::D
                && f.get(Face::F, last, last) == Color::F
                && f.get(Face::R, last, 0) == Color::R;
            // Edge FR: F-face[mid,last]=F, R-face[mid,0]=R.
            let edge = f.get(Face::F, mid, last) == Color::F
                && f.get(Face::R, mid, 0) == Color::R;
            corner && edge
        }
        F2LSlot::Fl => {
            // Corner DLF: D[0,0]=D, F[last,0]=F, L[last,last]=L.
            let corner = f.get(Face::D, 0, 0) == Color::D
                && f.get(Face::F, last, 0) == Color::F
                && f.get(Face::L, last, last) == Color::L;
            let edge = f.get(Face::F, mid, 0) == Color::F
                && f.get(Face::L, mid, last) == Color::L;
            corner && edge
        }
        F2LSlot::Br => {
            // Corner DRB: D[last,last]=D, B[last,0]=B, R[last,last]=R.
            let corner = f.get(Face::D, last, last) == Color::D
                && f.get(Face::B, last, 0) == Color::B
                && f.get(Face::R, last, last) == Color::R;
            let edge = f.get(Face::B, mid, 0) == Color::B
                && f.get(Face::R, mid, last) == Color::R;
            corner && edge
        }
        F2LSlot::Bl => {
            // Corner DLB: D[last,0]=D, B[last,last]=B, L[last,0]=L.
            let corner = f.get(Face::D, last, 0) == Color::D
                && f.get(Face::B, last, last) == Color::B
                && f.get(Face::L, last, 0) == Color::L;
            let edge = f.get(Face::B, mid, last) == Color::B
                && f.get(Face::L, mid, 0) == Color::L;
            corner && edge
        }
    }
}

/// Yellow cross on top: top face shows the cross pattern (+) of D-color
/// stickers. (Note: in our color scheme `D` = yellow.) Corners may be
/// any color.
fn is_yellow_cross_oriented(f: &Facelets) -> bool {
    // After OLL edge orientation, U face has its center plus 4 edges
    // showing U-color (= white in default); but at the "yellow cross"
    // step of beginner LBL, the cube is upside-down (D color on top).
    // We accept either D or U cross — both indicate the four edges are
    // oriented.
    let n = f.size;
    if n < 3 {
        return false;
    }
    let mid = n / 2;
    let cross_color = f.get(Face::U, mid, mid);
    f.get(Face::U, 0, mid) == cross_color
        && f.get(Face::U, mid, 0) == cross_color
        && f.get(Face::U, mid, n - 1) == cross_color
        && f.get(Face::U, n - 1, mid) == cross_color
}

/// Load a lesson from a RON string. Convenience wrapper around `ron::from_str`.
pub fn parse_lesson(ron_text: &str) -> Result<Lesson, ron::de::SpannedError> {
    ron::from_str(ron_text)
}

#[cfg(test)]
mod tests {
    use super::*;
    use cube_core::MoveSeq;

    fn cube_after(moves: &str) -> Cube {
        let mut cube = Cube::solved(3).unwrap();
        cube.apply_seq(&MoveSeq::parse(moves, 3).unwrap()).unwrap();
        cube
    }

    #[test]
    fn solved_predicate_matches_solved_cube() {
        let cube = Cube::solved(3).unwrap();
        assert!(GoalPredicate::Solved.evaluate(&cube));
        assert!(!GoalPredicate::Solved.evaluate(&cube_after("R")));
    }

    #[test]
    fn oll_solved_when_top_uniform() {
        let cube = Cube::solved(3).unwrap();
        assert!(GoalPredicate::OllSolved.evaluate(&cube));
        // R-cw breaks the top face.
        assert!(!GoalPredicate::OllSolved.evaluate(&cube_after("R")));
        // U-cw permutes the top face but keeps every sticker U-colored.
        assert!(GoalPredicate::OllSolved.evaluate(&cube_after("U")));
        // U² same.
        assert!(GoalPredicate::OllSolved.evaluate(&cube_after("U2")));
    }

    #[test]
    fn white_cross_solved_after_setup() {
        let cube = Cube::solved(3).unwrap();
        assert!(GoalPredicate::WhiteCrossSolved.evaluate(&cube));
        // Move that takes a white-cross edge off → cross broken.
        assert!(!GoalPredicate::WhiteCrossSolved.evaluate(&cube_after("F")));
    }

    #[test]
    fn white_face_solved_requires_first_layer() {
        let cube = Cube::solved(3).unwrap();
        assert!(GoalPredicate::WhiteFaceSolved.evaluate(&cube));
        // R turn breaks the first layer's right column.
        assert!(!GoalPredicate::WhiteFaceSolved.evaluate(&cube_after("R")));
    }

    #[test]
    fn f2l_fr_solved_for_solved_cube() {
        let cube = Cube::solved(3).unwrap();
        for slot in [F2LSlot::Fr, F2LSlot::Fl, F2LSlot::Br, F2LSlot::Bl] {
            assert!(GoalPredicate::F2LPairSolved { slot }.evaluate(&cube));
        }
    }

    #[test]
    fn yellow_cross_oriented_on_solved_cube() {
        let cube = Cube::solved(3).unwrap();
        // Solved cube trivially has the cross pattern (whole face uniform).
        assert!(GoalPredicate::YellowCrossOriented.evaluate(&cube));
    }

    #[test]
    fn parse_shipped_white_cross_asset() {
        let ron_text = include_str!("../../../assets/guides/beginner_white_cross.ron");
        let lesson = parse_lesson(ron_text).expect("shipped asset must parse");
        assert_eq!(lesson.id, "beginner-1-white-cross");
        assert!(!lesson.steps.is_empty());
        // Every Practice step's setup notation must parse, and at least
        // one Practice step uses WhiteCrossSolved as goal.
        let mut saw_white_cross = false;
        for step in &lesson.steps {
            if let Step::Practice { setup, goal, .. } = step {
                MoveSeq::parse(setup, 3).expect("practice setup must be valid notation");
                if matches!(goal, GoalPredicate::WhiteCrossSolved) {
                    saw_white_cross = true;
                }
            }
            if let Step::Demonstrate { setup, algorithm, .. } = step {
                MoveSeq::parse(setup, 3).expect("demo setup must parse");
                MoveSeq::parse(algorithm, 3).expect("demo algorithm must parse");
            }
        }
        assert!(saw_white_cross, "expected at least one WhiteCrossSolved practice step");
    }

    #[test]
    fn parse_minimal_lesson_round_trips() {
        let ron_text = r#"
            (
                id: "test-1",
                title: "Test",
                prerequisites: [],
                estimated_minutes: 5,
                steps: [
                    Explain(
                        text: "Hello.",
                        highlight: [],
                        camera_focus: None,
                    ),
                    Practice(
                        setup: "R U R'",
                        goal: Solved,
                        hint: Some("R U' R'"),
                    ),
                ],
            )
        "#;
        let lesson = parse_lesson(ron_text).unwrap();
        assert_eq!(lesson.id, "test-1");
        assert_eq!(lesson.steps.len(), 2);
    }
}
