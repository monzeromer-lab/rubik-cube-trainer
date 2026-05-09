use serde::{Deserialize, Serialize};

use crate::orient::{Orient, generators};

/// One of the six labelled faces of the cube.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Face {
    U,
    D,
    L,
    R,
    F,
    B,
}

impl Face {
    /// Axis index this face's normal lies along: 0=X, 1=Y, 2=Z.
    pub fn axis(self) -> u8 {
        match self {
            Face::R | Face::L => 0,
            Face::U | Face::D => 1,
            Face::F | Face::B => 2,
        }
    }

    /// Sign of the outward-facing normal along [`Face::axis`].
    pub fn axis_sign(self) -> i32 {
        match self {
            Face::R | Face::U | Face::F => 1,
            Face::L | Face::D | Face::B => -1,
        }
    }

    pub const ALL: [Face; 6] = [Face::U, Face::D, Face::L, Face::R, Face::F, Face::B];

    pub fn opposite(self) -> Face {
        match self {
            Face::U => Face::D,
            Face::D => Face::U,
            Face::L => Face::R,
            Face::R => Face::L,
            Face::F => Face::B,
            Face::B => Face::F,
        }
    }

    pub fn symbol(self) -> char {
        match self {
            Face::U => 'U',
            Face::D => 'D',
            Face::L => 'L',
            Face::R => 'R',
            Face::F => 'F',
            Face::B => 'B',
        }
    }
}

/// One quarter, half, or three-quarter turn of a face/slice.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Turn {
    /// 90° clockwise as viewed from outside the named face (WCA convention).
    Cw,
    /// 90° counter-clockwise as viewed from outside.
    Ccw,
    /// 180°.
    Half,
}

impl Turn {
    pub fn inverse(self) -> Self {
        match self {
            Turn::Cw => Turn::Ccw,
            Turn::Ccw => Turn::Cw,
            Turn::Half => Turn::Half,
        }
    }

    /// Number of quarter-turn steps modulo 4 (Cw=1, Half=2, Ccw=3).
    pub fn steps(self) -> u8 {
        match self {
            Turn::Cw => 1,
            Turn::Half => 2,
            Turn::Ccw => 3,
        }
    }

    /// Inverse of [`Turn::steps`]. Returns `None` for 0 (no-move).
    pub fn from_steps(s: u8) -> Option<Self> {
        match s % 4 {
            0 => None,
            1 => Some(Turn::Cw),
            2 => Some(Turn::Half),
            3 => Some(Turn::Ccw),
            _ => unreachable!(),
        }
    }
}

/// A single layer rotation. Cube rotations (`x`/`y`/`z`) and slice moves
/// (`M`/`E`/`S`) are encoded as wide / single-slice variants of this struct
/// (see [`crate::parser`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Move {
    pub face: Face,
    pub turn: Turn,
    /// Layer depth from `face`. 1 = outer layer (e.g., `R`).
    /// On a 5×5, depth=2 with face=R is the slice second-from-right.
    pub depth: u8,
    /// Wide turn: rotates layers `1..=depth` together. `false` rotates only
    /// the single layer at `depth`.
    pub wide: bool,
}

impl Move {
    /// Convenience: single-layer outer turn.
    pub const fn face(face: Face, turn: Turn) -> Self {
        Self { face, turn, depth: 1, wide: false }
    }

    pub fn inverse(self) -> Self {
        Self { turn: self.turn.inverse(), ..self }
    }

    /// The rotation matrix corresponding to this move's turn, expressed as
    /// an [`Orient`] index.
    ///
    /// WCA "CW from outside" means: the observer stands on the +s side of
    /// the face's axis, looks toward the cube, with their head along +Y.
    /// Their right hand then points along `forward × up`. For the +X face,
    /// forward = −X and up = +Y → right = −Z, so a CW turn (top → right)
    /// is +Y → −Z — that is the right-hand rotation around −X (= around
    /// +X by −90°), i.e. our `rx_ccw`. The same flip applies to every
    /// face: `forward = ccw_around_axis` when `outward_sign == +1`.
    ///
    /// Earlier versions of this function had the mapping inverted (using
    /// the right-hand-rule rotation directly), which produced a fully
    /// internally-consistent cube simulator that disagreed with WCA on
    /// the direction of every face. Order-symmetric properties (`X^4 =
    /// id`, `A · A⁻¹ = id`, `sexy^6 = id`) hide the mismatch, but order-2
    /// algorithms like T-perm — whose inverse is themselves — expose it.
    pub fn rotation(self) -> Orient {
        let g = generators();
        let outward_sign = self.face.axis_sign();
        let (cw, ccw, half) = match self.face.axis() {
            0 => (g.rx_cw, g.rx_ccw, g.rx_2),
            1 => (g.ry_cw, g.ry_ccw, g.ry_2),
            2 => (g.rz_cw, g.rz_ccw, g.rz_2),
            _ => unreachable!(),
        };
        let (forward, backward) = if outward_sign == 1 { (ccw, cw) } else { (cw, ccw) };
        match self.turn {
            Turn::Cw => forward,
            Turn::Ccw => backward,
            Turn::Half => half,
        }
    }

    /// Coordinate value along [`Face::axis`] that picks out the affected layer
    /// for the given cube `size` and `depth`. For wide moves the caller must
    /// iterate over all depths `1..=self.depth`.
    pub fn layer_coord(face: Face, size: u8, depth: u8) -> i32 {
        let n = size as i32;
        let d = depth as i32;
        face.axis_sign() * (n + 1 - 2 * d)
    }

    /// Iterator over the axis-coordinates the affected cubies must match.
    pub fn affected_layer_coords(self, size: u8) -> impl Iterator<Item = i32> {
        let depth = self.depth;
        let face = self.face;
        let start = if self.wide { 1 } else { depth };
        (start..=depth).map(move |d| Move::layer_coord(face, size, d))
    }

    /// Whether this move is equivalent to a whole-cube rotation on size `N`.
    pub fn is_whole_cube_rotation(self, size: u8) -> bool {
        self.wide && self.depth == size
    }
}

/// A whole-cube rotation around one of three axes. Held as a separate enum
/// so user-facing notation `x y z` can round-trip cleanly even though they
/// also expand into an equivalent `Move`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum CubeRotation {
    X(Turn),
    Y(Turn),
    Z(Turn),
}

impl CubeRotation {
    /// Lower this cube rotation into its [`Move`] equivalent for cube `size`.
    /// The convention: `x` follows R direction, `y` follows U, `z` follows F.
    pub fn as_move(self, size: u8) -> Move {
        let (face, turn) = match self {
            CubeRotation::X(t) => (Face::R, t),
            CubeRotation::Y(t) => (Face::U, t),
            CubeRotation::Z(t) => (Face::F, t),
        };
        Move { face, turn, depth: size, wide: true }
    }
}

/// Sequence of moves. Notation parsing and pretty-printing live in
/// [`crate::parser`], inverse and cancellation here.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct MoveSeq(pub Vec<Move>);

impl MoveSeq {
    pub fn new() -> Self {
        Self(Vec::new())
    }

    pub fn from_vec(v: Vec<Move>) -> Self {
        Self(v)
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub fn push(&mut self, m: Move) {
        self.0.push(m);
    }

    pub fn extend<I: IntoIterator<Item = Move>>(&mut self, it: I) {
        self.0.extend(it);
    }

    pub fn iter(&self) -> std::slice::Iter<'_, Move> {
        self.0.iter()
    }

    /// Move-wise inverse: reversed order, each turn negated.
    pub fn inverse(&self) -> Self {
        Self(self.0.iter().rev().map(|m| m.inverse()).collect())
    }

    /// Cancel adjacent moves on the same face/depth/wide:
    /// `R R'` → ε, `R R` → `R2`, `R2 R` → `R'`, etc.
    /// Does not commute across non-equivalent layers (a conservative pass).
    pub fn cancel(&self) -> Self {
        let mut out: Vec<Move> = Vec::with_capacity(self.0.len());
        for m in &self.0 {
            if let Some(prev) = out.last()
                && prev.face == m.face
                && prev.depth == m.depth
                && prev.wide == m.wide
            {
                let combined = (prev.turn.steps() + m.turn.steps()) % 4;
                out.pop();
                if let Some(t) = Turn::from_steps(combined) {
                    out.push(Move { turn: t, ..*m });
                }
                continue;
            }
            out.push(*m);
        }
        Self(out)
    }
}

impl IntoIterator for MoveSeq {
    type Item = Move;
    type IntoIter = std::vec::IntoIter<Move>;
    fn into_iter(self) -> Self::IntoIter {
        self.0.into_iter()
    }
}

impl<'a> IntoIterator for &'a MoveSeq {
    type Item = &'a Move;
    type IntoIter = std::slice::Iter<'a, Move>;
    fn into_iter(self) -> Self::IntoIter {
        self.0.iter()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn turn_inverse_round_trip() {
        for t in [Turn::Cw, Turn::Ccw, Turn::Half] {
            assert_eq!(t.inverse().inverse(), t);
        }
    }

    #[test]
    fn turn_steps_round_trip() {
        for t in [Turn::Cw, Turn::Ccw, Turn::Half] {
            assert_eq!(Turn::from_steps(t.steps()), Some(t));
        }
        assert_eq!(Turn::from_steps(0), None);
        assert_eq!(Turn::from_steps(4), None);
    }

    #[test]
    fn move_inverse_combines_to_identity_steps() {
        let m = Move::face(Face::R, Turn::Cw);
        let inv = m.inverse();
        assert_eq!((m.turn.steps() + inv.turn.steps()) % 4, 0);
    }

    #[test]
    fn cancel_removes_inverse_pair() {
        let r = Move::face(Face::R, Turn::Cw);
        let r_inv = Move::face(Face::R, Turn::Ccw);
        let seq = MoveSeq::from_vec(vec![r, r_inv]);
        assert!(seq.cancel().is_empty());
    }

    #[test]
    fn cancel_combines_two_quarters_to_half() {
        let r = Move::face(Face::R, Turn::Cw);
        let seq = MoveSeq::from_vec(vec![r, r]);
        let canceled = seq.cancel();
        assert_eq!(canceled.0, vec![Move::face(Face::R, Turn::Half)]);
    }

    #[test]
    fn cancel_does_not_merge_different_faces() {
        let r = Move::face(Face::R, Turn::Cw);
        let u = Move::face(Face::U, Turn::Cw);
        let seq = MoveSeq::from_vec(vec![r, u, r]);
        assert_eq!(seq.cancel().0, vec![r, u, r]);
    }

    #[test]
    fn r_outer_layer_coord_is_n_minus_1() {
        // For N=3, R depth=1 affects x=2.
        assert_eq!(Move::layer_coord(Face::R, 3, 1), 2);
        assert_eq!(Move::layer_coord(Face::L, 3, 1), -2);
        // Inner middle slice on 3×3: L depth 2 = x = 0.
        assert_eq!(Move::layer_coord(Face::L, 3, 2), 0);
    }
}
