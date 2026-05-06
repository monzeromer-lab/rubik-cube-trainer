//! Cubie-level cube state. Source of truth for the simulation.

use glam::IVec3;
use serde::{Deserialize, Serialize};

use crate::error::CubeError;
use crate::moves::{Move, MoveSeq};
use crate::orient::Orient;

/// Smallest supported cube size.
pub const MIN_SIZE: u8 = 2;
/// Largest supported cube size.
pub const MAX_SIZE: u8 = 5;

/// A single physical sub-cube. Coordinates live on the integer lattice
/// `-(N-1)..=(N-1)` stepping by 2.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Cubie {
    /// Home position; identifies the piece across moves.
    pub solved_pos: IVec3,
    /// Where the piece currently sits.
    pub current_pos: IVec3,
    /// Rotation applied to the piece relative to its home orientation.
    pub orientation: Orient,
}

impl Cubie {
    pub fn solved_at(pos: IVec3) -> Self {
        Self {
            solved_pos: pos,
            current_pos: pos,
            orientation: Orient::IDENTITY,
        }
    }

    /// True iff the piece sits at home with no rotation applied.
    pub fn is_at_home(&self) -> bool {
        self.solved_pos == self.current_pos && self.orientation == Orient::IDENTITY
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Cube {
    pub size: u8,
    /// Visible pieces only — no invisible interior cubies. Sorted by
    /// `solved_pos` (lex order on (x, y, z)) for stable iteration.
    pub cubies: Vec<Cubie>,
}

impl Cube {
    /// Build a freshly solved cube of the given size.
    pub fn solved(size: u8) -> Result<Self, CubeError> {
        if !(MIN_SIZE..=MAX_SIZE).contains(&size) {
            return Err(CubeError::InvalidSize(size));
        }
        let positions = visible_positions(size);
        let cubies = positions.into_iter().map(Cubie::solved_at).collect();
        Ok(Self { size, cubies })
    }

    /// Apply a move in place.
    pub fn apply(&mut self, m: Move) -> Result<(), CubeError> {
        if m.depth == 0 {
            return Err(CubeError::ZeroDepth);
        }
        if m.depth > self.size {
            return Err(CubeError::DepthOutOfRange { depth: m.depth, size: self.size });
        }
        let axis = m.face.axis() as usize;
        let coords: Vec<i32> = m.affected_layer_coords(self.size).collect();
        let rotation = m.rotation();
        for cubie in self.cubies.iter_mut() {
            let v = component(cubie.current_pos, axis);
            if coords.contains(&v) {
                cubie.current_pos = rotation.apply(cubie.current_pos);
                cubie.orientation = rotation.compose(cubie.orientation);
            }
        }
        Ok(())
    }

    /// Apply a sequence of moves in order.
    pub fn apply_seq(&mut self, seq: &MoveSeq) -> Result<(), CubeError> {
        for &m in &seq.0 {
            self.apply(m)?;
        }
        Ok(())
    }

    /// Apply a single move and return the resulting cube.
    pub fn after(&self, m: Move) -> Result<Self, CubeError> {
        let mut out = self.clone();
        out.apply(m)?;
        Ok(out)
    }

    /// Visually solved: every face shows a single color. Indistinguishable
    /// pieces (e.g., 4×4 wing-edge twins, big-cube centers) and unobservable
    /// center-cubie rotations on 3×3 are all OK — what matters is the sticker
    /// view, not the internal cubie positions.
    pub fn is_solved(&self) -> bool {
        crate::facelets::Facelets::from_cube(self) == crate::facelets::Facelets::solved(self.size)
    }

    /// Strict: every cubie at home with identity orientation. Useful for
    /// internal invariant checks; usually you want [`Cube::is_solved`].
    pub fn is_strictly_solved(&self) -> bool {
        self.cubies.iter().all(Cubie::is_at_home)
    }

    /// Number of pieces. Useful as an invariant: must never change.
    pub fn cubie_count(&self) -> usize {
        self.cubies.len()
    }
}

fn component(v: IVec3, axis: usize) -> i32 {
    match axis {
        0 => v.x,
        1 => v.y,
        2 => v.z,
        _ => unreachable!(),
    }
}

/// Parity-aware corner twist value for a cubie at its current position.
///
/// Even-parity positions (`sx*sy*sz > 0`) cycle stickers `Y → X → Z`;
/// odd-parity positions cycle `Y → Z → X`. With this convention the total
/// twist mod 3 is invariant under every face turn — the standard 3×3
/// corner-orientation parity rule.
///
/// Defined here so [`Facelets`] validation, [`Solver2x2`], and the
/// 3×3 solver all share one implementation. Only meaningful for corner
/// cubies (3 outward stickers); calling it on edges/centers gives a
/// well-defined but not-physically-meaningful value.
pub fn corner_twist(cubie: &Cubie) -> u8 {
    let home_y = if cubie.solved_pos.y > 0 {
        IVec3::Y
    } else {
        IVec3::NEG_Y
    };
    let now_y = cubie.orientation.apply(home_y);
    let axis = if now_y.y != 0 {
        1
    } else if now_y.x != 0 {
        0
    } else {
        2
    };
    let parity = cubie.current_pos.x.signum()
        * cubie.current_pos.y.signum()
        * cubie.current_pos.z.signum();
    match (axis, parity) {
        (1, _) => 0,
        (0, 1) => 1,
        (0, -1) => 2,
        (2, 1) => 2,
        (2, -1) => 1,
        _ => unreachable!("corner twist axis not in {{X, Y, Z}}"),
    }
}

/// Standard Kociemba edge orientation. UD-layer edges (home y ≠ 0) use
/// the U/D-colored sticker as primary and Y as their good axis; E-slice
/// edges (home y = 0) use the F/B sticker and Z. Returns 0 if oriented
/// (primary on its good axis), 1 if flipped.
///
/// Preserved by every Phase 2 move (`U, D, R², L², F², B²`) — the
/// invariant the two-phase solver relies on.
pub fn edge_orient(cubie: &Cubie) -> u8 {
    let h = cubie.solved_pos;
    let (primary, good_axis) = if h.y != 0 {
        let dir = if h.y > 0 { IVec3::Y } else { IVec3::NEG_Y };
        (dir, 1u8)
    } else {
        let dir = if h.z > 0 { IVec3::Z } else { IVec3::NEG_Z };
        (dir, 2u8)
    };
    let now = cubie.orientation.apply(primary);
    let now_axis = if now.x != 0 {
        0
    } else if now.y != 0 {
        1
    } else {
        2
    };
    if now_axis == good_axis { 0 } else { 1 }
}

/// Enumerate all visible piece positions for a cube of size `n`.
///
/// A piece is visible iff at least one of its coordinates lies on the outer
/// shell (`|c| == n - 1`).
fn visible_positions(n: u8) -> Vec<IVec3> {
    let n = n as i32;
    let outer = n - 1;
    let mut coords = Vec::with_capacity(n as usize);
    for k in 0..n {
        // -(n-1), -(n-3), ..., (n-1)
        coords.push(2 * k - (n - 1));
    }

    let mut positions = Vec::new();
    for &x in &coords {
        for &y in &coords {
            for &z in &coords {
                let on_shell = x.abs() == outer || y.abs() == outer || z.abs() == outer;
                if on_shell {
                    positions.push(IVec3::new(x, y, z));
                }
            }
        }
    }
    // Stable order for hashing/equality.
    positions.sort_by_key(|v| (v.x, v.y, v.z));
    positions
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::moves::{Face, Turn};

    fn r_cw() -> Move {
        Move::face(Face::R, Turn::Cw)
    }

    fn u_cw() -> Move {
        Move::face(Face::U, Turn::Cw)
    }

    #[test]
    fn solved_3x3_has_26_visible_cubies() {
        let cube = Cube::solved(3).unwrap();
        assert_eq!(cube.cubie_count(), 26);
        assert!(cube.is_solved());
    }

    #[test]
    fn solved_2x2_has_8_cubies() {
        assert_eq!(Cube::solved(2).unwrap().cubie_count(), 8);
    }

    #[test]
    fn solved_4x4_has_56_cubies() {
        assert_eq!(Cube::solved(4).unwrap().cubie_count(), 56);
    }

    #[test]
    fn solved_5x5_has_98_cubies() {
        assert_eq!(Cube::solved(5).unwrap().cubie_count(), 98);
    }

    #[test]
    fn invalid_size_errors() {
        assert!(Cube::solved(1).is_err());
        assert!(Cube::solved(6).is_err());
    }

    #[test]
    fn r_four_times_is_identity() {
        let mut cube = Cube::solved(3).unwrap();
        for _ in 0..4 {
            cube.apply(r_cw()).unwrap();
        }
        assert!(cube.is_solved());
    }

    #[test]
    fn r_then_r_inverse_is_identity() {
        let mut cube = Cube::solved(3).unwrap();
        cube.apply(r_cw()).unwrap();
        cube.apply(r_cw().inverse()).unwrap();
        assert!(cube.is_solved());
    }

    #[test]
    fn r_half_twice_is_identity() {
        let mut cube = Cube::solved(3).unwrap();
        let r2 = Move::face(Face::R, Turn::Half);
        cube.apply(r2).unwrap();
        cube.apply(r2).unwrap();
        assert!(cube.is_solved());
    }

    #[test]
    fn cubie_count_invariant_across_random_sequence() {
        let mut cube = Cube::solved(4).unwrap();
        let count_before = cube.cubie_count();
        for &m in &[r_cw(), u_cw(), r_cw().inverse(), u_cw().inverse()] {
            cube.apply(m).unwrap();
        }
        assert_eq!(cube.cubie_count(), count_before);
    }

    #[test]
    fn r_move_does_not_disturb_left_face() {
        let mut cube = Cube::solved(3).unwrap();
        let solved_left: Vec<_> = cube.cubies.iter().filter(|c| c.solved_pos.x <= 0).cloned().collect();
        cube.apply(r_cw()).unwrap();
        for c in cube.cubies.iter().filter(|c| c.solved_pos.x <= 0) {
            let original = solved_left.iter().find(|o| o.solved_pos == c.solved_pos).unwrap();
            assert_eq!(c.current_pos, original.current_pos);
            assert_eq!(c.orientation, original.orientation);
        }
    }

    #[test]
    fn applying_move_then_inverse_restores_state_2x2() {
        let mut cube = Cube::solved(2).unwrap();
        let initial = cube.clone();
        let m = r_cw();
        cube.apply(m).unwrap();
        cube.apply(m.inverse()).unwrap();
        assert_eq!(cube, initial);
    }

    #[test]
    fn depth_out_of_range_errors() {
        let mut cube = Cube::solved(3).unwrap();
        let m = Move { face: Face::R, turn: Turn::Cw, depth: 4, wide: false };
        assert!(cube.apply(m).is_err());
    }

    #[test]
    fn whole_cube_rotation_y_keeps_pieces_in_place_relative_to_each_other() {
        // After a y rotation, no cubie is "at home" but cube remains a valid state.
        let mut cube = Cube::solved(3).unwrap();
        let y = Move { face: Face::U, turn: Turn::Cw, depth: 3, wide: true };
        cube.apply(y).unwrap();
        // Four y rotations return to solved.
        for _ in 0..3 {
            cube.apply(y).unwrap();
        }
        assert!(cube.is_solved());
    }

    #[test]
    fn sexy_move_six_times_is_identity() {
        // R U R' U' applied 6 times = solved (a famous cube identity).
        let mut cube = Cube::solved(3).unwrap();
        let r = r_cw();
        let u = u_cw();
        let ri = r.inverse();
        let ui = u.inverse();
        for _ in 0..6 {
            cube.apply(r).unwrap();
            cube.apply(u).unwrap();
            cube.apply(ri).unwrap();
            cube.apply(ui).unwrap();
        }
        assert!(cube.is_solved());
    }

    #[test]
    fn sexy_move_three_times_is_not_identity() {
        let mut cube = Cube::solved(3).unwrap();
        let r = r_cw();
        let u = u_cw();
        for _ in 0..3 {
            cube.apply(r).unwrap();
            cube.apply(u).unwrap();
            cube.apply(r.inverse()).unwrap();
            cube.apply(u.inverse()).unwrap();
        }
        assert!(!cube.is_solved());
    }
}
