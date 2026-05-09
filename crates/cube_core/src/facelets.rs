//! Flat 6×N² sticker view of the cube — what a user sees and what the
//! sticker-input mode populates.
//!
//! Singmaster face order: `U R F D L B`. Within a face, indices run
//! row-major where row 0 is the top edge of the face (as viewed from
//! outside the cube), col 0 is the left edge.

use std::collections::HashSet;

use glam::IVec3;
use serde::{Deserialize, Serialize};

use crate::cube::{Cube, corner_twist};
use crate::error::{CubeError, FaceletValidationError};
use crate::moves::Face;
use crate::orient::Orient;

/// Sticker color, named after the face it occupies in the solved state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Color {
    U,
    D,
    F,
    B,
    R,
    L,
}

impl Color {
    /// Outward unit direction this color points toward in the solved cube.
    pub fn direction(self) -> IVec3 {
        match self {
            Color::U => IVec3::Y,
            Color::D => IVec3::NEG_Y,
            Color::R => IVec3::X,
            Color::L => IVec3::NEG_X,
            Color::F => IVec3::Z,
            Color::B => IVec3::NEG_Z,
        }
    }

    /// Inverse of [`Color::direction`].
    pub fn from_direction(d: IVec3) -> Option<Self> {
        match (d.x, d.y, d.z) {
            (0, 1, 0) => Some(Color::U),
            (0, -1, 0) => Some(Color::D),
            (1, 0, 0) => Some(Color::R),
            (-1, 0, 0) => Some(Color::L),
            (0, 0, 1) => Some(Color::F),
            (0, 0, -1) => Some(Color::B),
            _ => None,
        }
    }

    pub fn symbol(self) -> char {
        match self {
            Color::U => 'U',
            Color::D => 'D',
            Color::F => 'F',
            Color::B => 'B',
            Color::R => 'R',
            Color::L => 'L',
        }
    }
}

/// Flat sticker array. Indexed by `face_index(face) * N² + row * N + col`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Facelets {
    pub size: u8,
    pub stickers: Vec<Color>,
}

impl Facelets {
    pub fn solved(size: u8) -> Self {
        let n = size as usize;
        let mut stickers = Vec::with_capacity(6 * n * n);
        for color in [Color::U, Color::R, Color::F, Color::D, Color::L, Color::B] {
            for _ in 0..(n * n) {
                stickers.push(color);
            }
        }
        Self { size, stickers }
    }

    /// Read a sticker by face/row/col coordinates.
    pub fn get(&self, face: Face, row: u8, col: u8) -> Color {
        self.stickers[index(face, self.size, row, col)]
    }

    /// Write a sticker.
    pub fn set(&mut self, face: Face, row: u8, col: u8, color: Color) {
        let i = index(face, self.size, row, col);
        self.stickers[i] = color;
    }

    /// Compute the sticker view of `cube`.
    pub fn from_cube(cube: &Cube) -> Self {
        let n = cube.size;
        let mut stickers = vec![Color::U; 6 * (n as usize) * (n as usize)];
        for face in Face::ALL {
            let axes = face_axes(face);
            for row in 0..n {
                for col in 0..n {
                    let pos = grid_to_pos(face, n, row, col);
                    let cubie = cube
                        .cubies
                        .iter()
                        .find(|c| c.current_pos == pos)
                        .expect("every face cell must be occupied by a cubie");
                    // The sticker now facing `outward` was originally facing
                    // `O^{-1}(outward)`; that direction names the color.
                    let original_dir = cubie.orientation.inverse().apply(axes.outward);
                    let color = Color::from_direction(original_dir)
                        .expect("orientation must map face directions to face directions");
                    stickers[index(face, n, row, col)] = color;
                }
            }
        }
        Self { size: n, stickers }
    }

    /// Run the full sticker-input validation pipeline (plan §9.2). Returns
    /// every detected problem as a `Vec` so the UI can highlight them all.
    /// Sticker-input mode calls this before enabling "Solve". When `Ok`,
    /// [`Self::try_into_cube`] is guaranteed to succeed.
    pub fn validate(&self) -> Result<(), Vec<FaceletValidationError>> {
        let mut errors = Vec::new();

        // Length.
        let n = self.size as usize;
        let expected_len = 6 * n * n;
        if self.stickers.len() != expected_len {
            errors.push(FaceletValidationError::BadFaceletLength {
                got: self.stickers.len(),
                expected: expected_len,
            });
            return Err(errors);
        }

        // Per-color counts.
        let expected_count = n * n;
        for color in [Color::U, Color::D, Color::F, Color::B, Color::R, Color::L] {
            let count = self.stickers.iter().filter(|c| **c == color).count();
            if count != expected_count {
                errors.push(FaceletValidationError::BadColorCount {
                    color,
                    got: count,
                    expected: expected_count,
                });
            }
        }

        // Per-piece duplicates and opposite-color violations.
        let outer = (self.size as i32) - 1;
        let positions = piece_positions_for_size(self.size);
        for pos in positions {
            let stickers = read_stickers_at(self, pos);
            let colors: Vec<Color> = stickers.iter().map(|(_, c)| *c).collect();
            for i in 0..colors.len() {
                for j in (i + 1)..colors.len() {
                    if colors[i] == colors[j] {
                        errors.push(FaceletValidationError::DuplicateColorOnPiece {
                            pos,
                            color: colors[i],
                        });
                    } else if are_opposite(colors[i], colors[j]) {
                        errors.push(FaceletValidationError::OppositeColorsOnPiece {
                            pos,
                            a: colors[i],
                            b: colors[j],
                        });
                    }
                }
            }
        }

        // If color/piece-level checks pass, attempt to assemble a Cube.
        // try_into_cube returns Err if no canonical cubie matches a
        // particular position's color set; surface that as
        // UnknownCubieAt rather than a generic InvalidFaceletState.
        if errors.is_empty() {
            match self.try_into_cube() {
                Ok(cube) => {
                    // Orientation sum invariants. Only meaningful for sizes
                    // where the parity invariants apply uniformly — i.e.
                    // 3×3 (visible cubies are corners + edges + 6 fixed
                    // centers). For 2×2 corner sum still holds; for 4×4/5×5
                    // we skip permutation parity since wing-edge twins make
                    // it ambiguous.
                    let _ = outer;
                    let corner_sum: u32 = cube
                        .cubies
                        .iter()
                        .filter(|c| is_corner(c.solved_pos, self.size))
                        .map(|c| corner_twist(c) as u32)
                        .sum();
                    if corner_sum % 3 != 0 {
                        errors.push(FaceletValidationError::CornerOrientationSum {
                            sum: corner_sum,
                            modulus: corner_sum % 3,
                        });
                    }
                    if self.size == 3 {
                        // Kociemba-standard EO: an edge is "good" iff its
                        // primary sticker (U/D-color for UD-layer edges,
                        // F/B-color for E-slice edges) currently faces a
                        // good face — that is, its current axis is Y or Z,
                        // never X. Sum-mod-2 across all 12 edges is 0 for
                        // every reachable state. This check catches the
                        // single-flipped-edge sticker error that the
                        // permutation parity check below cannot.
                        let edge_sum: u32 = cube
                            .cubies
                            .iter()
                            .filter(|c| is_edge(c.solved_pos, self.size))
                            .map(|c| kociemba_edge_orient(c) as u32)
                            .sum();
                        if edge_sum % 2 != 0 {
                            errors.push(FaceletValidationError::EdgeOrientationSum {
                                sum: edge_sum,
                                modulus: edge_sum % 2,
                            });
                        }
                        let cp = perm_parity_of_kind(&cube, self.size, is_corner);
                        let ep = perm_parity_of_kind(&cube, self.size, is_edge);
                        if cp != ep {
                            errors.push(FaceletValidationError::PermParityMismatch {
                                corner_parity: cp,
                                edge_parity: ep,
                            });
                        }
                    }
                }
                Err(CubeError::InvalidFaceletState { reason: _ }) => {
                    // First look for any position whose color set is not a
                    // canonical cubie's set — that's the most actionable
                    // error to report.
                    let mut found_unknown = false;
                    for pos in piece_positions_for_size(self.size) {
                        let stickers = read_stickers_at(self, pos);
                        let mut have: Vec<Color> = stickers.iter().map(|(_, c)| *c).collect();
                        have.sort_by_key(|c| *c as u8);
                        let any_canonical = piece_positions_for_size(self.size)
                            .into_iter()
                            .any(|p| {
                                let mut canonical = solved_color_set_for(p, self.size);
                                canonical.sort_by_key(|c| *c as u8);
                                canonical == have
                            });
                        if !any_canonical {
                            errors.push(FaceletValidationError::UnknownCubieAt {
                                pos,
                                colors: have,
                            });
                            found_unknown = true;
                            break;
                        }
                    }
                    // If every position has a canonical set but try_into_cube
                    // still failed (e.g. a swap that would require a
                    // reflection), surface a generic "not reachable" error.
                    if !found_unknown {
                        errors.push(FaceletValidationError::NotReachable);
                    }
                }
                Err(_) => {
                    // Length / size errors are handled earlier.
                }
            }
        }

        if errors.is_empty() { Ok(()) } else { Err(errors) }
    }

    /// Reconstruct a cube state from the sticker view.
    ///
    /// For the V1 round-trip path (cube → facelets → cube), this assumes the
    /// facelets describe a reachable cube state. It does *not* perform the
    /// full validation pipeline — call [`Self::validate`] for that.
    pub fn try_into_cube(&self) -> Result<Cube, CubeError> {
        let n = self.size;
        let expected = 6 * (n as usize) * (n as usize);
        if self.stickers.len() != expected {
            return Err(CubeError::FaceletLength { got: self.stickers.len(), expected });
        }

        let mut cube = Cube::solved(n)?;
        // Cubies with identical sticker sets (4×4 wing edges, big-cube centers)
        // are physically interchangeable. Claim each current position the
        // first time we match it to keep the assignment a bijection.
        let mut claimed: HashSet<IVec3> = HashSet::new();
        for cubie in cube.cubies.iter_mut() {
            let exposed = exposed_directions(cubie.solved_pos, n);
            let solved_colors: Vec<Color> = exposed
                .iter()
                .map(|d| Color::from_direction(*d).unwrap())
                .collect();
            let (current_pos, mapping) = find_position_for(self, &solved_colors, &claimed)
                .ok_or_else(|| CubeError::InvalidFaceletState {
                    reason: format!(
                        "no unclaimed current position has the sticker set {:?} for cubie at {:?}",
                        solved_colors, cubie.solved_pos
                    ),
                })?;
            claimed.insert(current_pos);
            cubie.current_pos = current_pos;
            cubie.orientation = derive_orientation(&exposed, &mapping)?;
        }
        Ok(cube)
    }
}

/// Singmaster face order: U R F D L B.
fn face_index(face: Face) -> usize {
    match face {
        Face::U => 0,
        Face::R => 1,
        Face::F => 2,
        Face::D => 3,
        Face::L => 4,
        Face::B => 5,
    }
}

fn index(face: Face, n: u8, row: u8, col: u8) -> usize {
    let n = n as usize;
    face_index(face) * n * n + (row as usize) * n + (col as usize)
}

#[derive(Clone, Copy)]
struct FaceAxes {
    outward: IVec3,
    top: IVec3,
    right: IVec3,
}

/// In-plane axes for each face when viewed from outside the cube.
/// Convention: `top × outward = right` (right-hand rule), so the face's
/// local frame is consistent across all six.
fn face_axes(face: Face) -> FaceAxes {
    match face {
        Face::U => FaceAxes { outward: IVec3::Y, top: IVec3::NEG_Z, right: IVec3::X },
        Face::D => FaceAxes { outward: IVec3::NEG_Y, top: IVec3::Z, right: IVec3::X },
        Face::F => FaceAxes { outward: IVec3::Z, top: IVec3::Y, right: IVec3::X },
        Face::B => FaceAxes { outward: IVec3::NEG_Z, top: IVec3::Y, right: IVec3::NEG_X },
        Face::R => FaceAxes { outward: IVec3::X, top: IVec3::Y, right: IVec3::NEG_Z },
        Face::L => FaceAxes { outward: IVec3::NEG_X, top: IVec3::Y, right: IVec3::Z },
    }
}

fn grid_to_pos(face: Face, n: u8, row: u8, col: u8) -> IVec3 {
    let n = n as i32;
    let row = row as i32;
    let col = col as i32;
    let a = face_axes(face);
    a.outward * (n - 1) + a.top * (n - 1 - 2 * row) + a.right * (2 * col - (n - 1))
}

/// Direction unit vectors for each face that the cubie at `pos` currently
/// presents a sticker on. A piece is exposed on face F iff its coord along
/// F's outward axis equals `±(N-1)` matching F's sign.
fn exposed_directions(pos: IVec3, n: u8) -> Vec<IVec3> {
    let outer = (n as i32) - 1;
    let mut out = Vec::with_capacity(3);
    if pos.x == outer { out.push(IVec3::X); }
    if pos.x == -outer { out.push(IVec3::NEG_X); }
    if pos.y == outer { out.push(IVec3::Y); }
    if pos.y == -outer { out.push(IVec3::NEG_Y); }
    if pos.z == outer { out.push(IVec3::Z); }
    if pos.z == -outer { out.push(IVec3::NEG_Z); }
    out
}

/// Read all sticker colors on the cubie that currently occupies `pos`.
fn read_stickers_at(facelets: &Facelets, pos: IVec3) -> Vec<(IVec3, Color)> {
    let n = facelets.size;
    let mut out = Vec::with_capacity(3);
    for face in Face::ALL {
        let axes = face_axes(face);
        // Does this face contain `pos`? It does iff pos's coord along
        // axes.outward equals axes.outward * (n-1).
        let outer = (n as i32) - 1;
        let along = pos.dot(axes.outward); // dot with unit gives signed coord
        if along != outer {
            continue;
        }
        // Find (row, col) on this face that maps back to pos.
        // pos = outward*(n-1) + top*(n-1-2r) + right*(2c-(n-1)).
        // top component: pos · top = n-1 - 2r → r = (n-1 - pos·top) / 2
        // right component: pos · right = 2c - (n-1) → c = (pos·right + n-1) / 2
        let r = ((n as i32 - 1) - pos.dot(axes.top)) / 2;
        let c = (pos.dot(axes.right) + (n as i32 - 1)) / 2;
        let color = facelets.get(face, r as u8, c as u8);
        out.push((axes.outward, color));
    }
    out
}

/// Find the lex-first current position whose exposed-sticker colors match
/// the multiset `solved_colors` and is not in `claimed`, returning the
/// position and the per-direction color mapping.
fn find_position_for(
    facelets: &Facelets,
    solved_colors: &[Color],
    claimed: &HashSet<IVec3>,
) -> Option<(IVec3, Vec<(IVec3, Color)>)> {
    let n = facelets.size;
    let outer = (n as i32) - 1;
    for x in (-outer..=outer).step_by(2) {
        for y in (-outer..=outer).step_by(2) {
            for z in (-outer..=outer).step_by(2) {
                let pos = IVec3::new(x, y, z);
                if claimed.contains(&pos) {
                    continue;
                }
                let exposed = exposed_directions(pos, n);
                if exposed.len() != solved_colors.len() {
                    continue;
                }
                let stickers = read_stickers_at(facelets, pos);
                let mut found_colors: Vec<Color> = stickers.iter().map(|(_, c)| *c).collect();
                let mut want = solved_colors.to_vec();
                found_colors.sort_by_key(|c| *c as u8);
                want.sort_by_key(|c| *c as u8);
                if found_colors == want {
                    return Some((pos, stickers));
                }
            }
        }
    }
    None
}

/// Find an [`Orient`] that maps each solved-direction to the corresponding
/// current-direction for this cubie's stickers.
///
/// `solved_dirs[i]` is `solved_colors[i].direction()`. We need O such that
/// for every i, `O.apply(solved_dirs[i]) == current_dir_with_color(c_i)`.
fn are_opposite(a: Color, b: Color) -> bool {
    matches!(
        (a, b),
        (Color::U, Color::D)
            | (Color::D, Color::U)
            | (Color::F, Color::B)
            | (Color::B, Color::F)
            | (Color::R, Color::L)
            | (Color::L, Color::R)
    )
}

fn is_corner(pos: IVec3, size: u8) -> bool {
    let outer = (size as i32) - 1;
    pos.x.abs() == outer && pos.y.abs() == outer && pos.z.abs() == outer
}

/// Kociemba-standard edge orientation for the [`Facelets::validate`] sum
/// invariant.
///
/// The "primary sticker" of an edge is determined by cubie *identity*:
/// - UD-layer edges (home `y ≠ 0`): the U/D-colored sticker.
/// - E-slice edges (home `y = 0`): the F/B-colored sticker.
///
/// The "good axis" — what direction the primary should face for EO=0 —
/// is determined by the current *slot*, not the home:
/// - UD-layer slots (current `y ≠ 0`): good axis is Y.
/// - E-slice slots (current `y = 0`): good axis is Z.
///
/// Under these slot-based rules, every quarter turn of `R, L, F, B`
/// flips exactly four edges (so EO sum mod 2 is preserved), and U, D,
/// and the squared turns flip none — the standard G1 invariant.
fn kociemba_edge_orient(cubie: &crate::cube::Cubie) -> u8 {
    let h = cubie.solved_pos;
    let primary = if h.y != 0 {
        if h.y > 0 { IVec3::Y } else { IVec3::NEG_Y }
    } else if h.z > 0 {
        IVec3::Z
    } else {
        IVec3::NEG_Z
    };
    let now = cubie.orientation.apply(primary);
    let now_axis = if now.x != 0 {
        0
    } else if now.y != 0 {
        1
    } else {
        2
    };
    let slot = cubie.current_pos;
    let good_axis: u8 = if slot.y != 0 { 1 } else { 2 };
    if now_axis == good_axis { 0 } else { 1 }
}

fn is_edge(pos: IVec3, size: u8) -> bool {
    let outer = (size as i32) - 1;
    let on_outer = [pos.x, pos.y, pos.z]
        .iter()
        .filter(|c| c.abs() == outer)
        .count();
    on_outer == 2
}

/// Permutation parity (sign of the permutation): +1 for even, –1 for odd.
/// Reads which cubie is currently at each slot of the given kind
/// (corners or edges) and counts inversions.
fn perm_parity_of_kind(cube: &Cube, _size: u8, predicate: fn(IVec3, u8) -> bool) -> i32 {
    let n = cube.size;
    let kind_homes: Vec<IVec3> = cube
        .cubies
        .iter()
        .filter(|c| predicate(c.solved_pos, n))
        .map(|c| c.solved_pos)
        .collect();
    // For each slot (home position), find which cubie's current_pos
    // equals it; record that cubie's home index within `kind_homes`.
    let perm: Vec<usize> = kind_homes
        .iter()
        .map(|slot_home| {
            let cubie_at_slot = cube
                .cubies
                .iter()
                .find(|c| c.current_pos == *slot_home)
                .expect("every slot must be occupied");
            kind_homes
                .iter()
                .position(|h| *h == cubie_at_slot.solved_pos)
                .expect("cubie's home must be in kind_homes if predicate matches")
        })
        .collect();
    let mut inversions = 0;
    for i in 0..perm.len() {
        for j in (i + 1)..perm.len() {
            if perm[i] > perm[j] {
                inversions += 1;
            }
        }
    }
    if inversions % 2 == 0 { 1 } else { -1 }
}

fn piece_positions_for_size(size: u8) -> Vec<IVec3> {
    let cube = Cube::solved(size).expect("valid size");
    cube.cubies.into_iter().map(|c| c.solved_pos).collect()
}

fn solved_color_set_for(pos: IVec3, size: u8) -> Vec<Color> {
    let outer = (size as i32) - 1;
    let mut colors = Vec::new();
    if pos.x == outer { colors.push(Color::R); }
    if pos.x == -outer { colors.push(Color::L); }
    if pos.y == outer { colors.push(Color::U); }
    if pos.y == -outer { colors.push(Color::D); }
    if pos.z == outer { colors.push(Color::F); }
    if pos.z == -outer { colors.push(Color::B); }
    colors
}

fn derive_orientation(
    solved_dirs: &[IVec3],
    current_stickers: &[(IVec3, Color)],
) -> Result<Orient, CubeError> {
    // Build target map: solved direction → required current direction.
    // Solved dir = color.direction(); look up in current_stickers by color.
    let required: Vec<(IVec3, IVec3)> = solved_dirs
        .iter()
        .map(|d| {
            let color = Color::from_direction(*d).unwrap();
            let target = current_stickers
                .iter()
                .find(|(_, c)| *c == color)
                .map(|(t, _)| *t)
                .ok_or(CubeError::InvalidFaceletState {
                    reason: format!("missing color {color:?} on cubie"),
                })?;
            Ok((*d, target))
        })
        .collect::<Result<Vec<_>, CubeError>>()?;

    // Pick the smallest-index orientation that satisfies all constraints.
    // - Corners (3 stickers): unique solution.
    // - Edges (2 stickers): unique solution.
    // - Centers (1 sticker): rotation around the outward axis is unobservable,
    //   so multiple orientations satisfy the constraint; lowest-index = the
    //   canonical "no twist" choice.
    for i in 0..24 {
        let o = Orient(i);
        if required.iter().all(|(s, t)| o.apply(*s) == *t) {
            return Ok(o);
        }
    }
    Err(CubeError::InvalidFaceletState {
        reason: "no rotation in the cube symmetry group satisfies sticker constraints".into(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cube::Cube;
    use crate::moves::{Face, Move, MoveSeq, Turn};

    #[test]
    fn solved_facelets_have_uniform_faces() {
        let f = Facelets::solved(3);
        for face in Face::ALL {
            for row in 0..3 {
                for col in 0..3 {
                    let expected = match face {
                        Face::U => Color::U,
                        Face::D => Color::D,
                        Face::F => Color::F,
                        Face::B => Color::B,
                        Face::R => Color::R,
                        Face::L => Color::L,
                    };
                    assert_eq!(f.get(face, row, col), expected, "{face:?}({row},{col})");
                }
            }
        }
    }

    #[test]
    fn from_solved_cube_matches_solved_facelets() {
        for size in 2..=5u8 {
            let cube = Cube::solved(size).unwrap();
            assert_eq!(Facelets::from_cube(&cube), Facelets::solved(size));
        }
    }

    #[test]
    fn round_trip_solved_cube() {
        for size in 2..=5u8 {
            let cube = Cube::solved(size).unwrap();
            let f = Facelets::from_cube(&cube);
            let back = f.try_into_cube().unwrap();
            assert_eq!(back, cube);
        }
    }

    #[test]
    fn round_trip_after_one_move_3x3() {
        let mut cube = Cube::solved(3).unwrap();
        cube.apply(Move::face(Face::R, Turn::Cw)).unwrap();
        let f = Facelets::from_cube(&cube);
        let back = f.try_into_cube().unwrap();
        // Sticker view must round-trip exactly.
        assert_eq!(Facelets::from_cube(&back), f);
    }

    #[test]
    fn round_trip_long_sequence_4x4() {
        let mut cube = Cube::solved(4).unwrap();
        let seq = MoveSeq::parse("R U R' U' Rw F2 D' L Rw' B U2 r u' f' L'", 4).unwrap();
        cube.apply_seq(&seq).unwrap();
        let f = Facelets::from_cube(&cube);
        let back = f.try_into_cube().unwrap();
        assert_eq!(Facelets::from_cube(&back), f);
    }

    #[test]
    fn validate_solved_cube_has_no_errors() {
        for size in 2..=5u8 {
            let f = Facelets::solved(size);
            f.validate().unwrap_or_else(|e| panic!("size {size} validate err: {e:?}"));
        }
    }

    #[test]
    fn validate_after_random_scrambles_passes() {
        use rand::SeedableRng;
        use rand_chacha::ChaCha8Rng;
        let mut rng = ChaCha8Rng::seed_from_u64(0xC0FFEE);
        for size in 2..=5u8 {
            let mut cube = Cube::solved(size).unwrap();
            let seq = crate::random_move_scramble(size, 30, &mut rng);
            cube.apply_seq(&seq).unwrap();
            let f = Facelets::from_cube(&cube);
            f.validate().unwrap_or_else(|e| panic!("size {size}: {e:?}"));
        }
    }

    #[test]
    fn validate_bad_color_count_detected() {
        // Replace one sticker so two colors have wrong counts.
        let mut f = Facelets::solved(3);
        f.set(Face::U, 0, 0, Color::D);
        let errs = f.validate().unwrap_err();
        assert!(errs.iter().any(|e| matches!(
            e,
            FaceletValidationError::BadColorCount { color: Color::U, .. }
        )));
        assert!(errs.iter().any(|e| matches!(
            e,
            FaceletValidationError::BadColorCount { color: Color::D, .. }
        )));
    }

    #[test]
    fn validate_single_edge_flip_detected() {
        // Swap the U-color and R-color stickers on the UR edge. That
        // puts U-color (the UR edge's primary sticker) onto the R face
        // — an X-axis face, which Kociemba calls "flipped". Total EO
        // sum = 1 (mod 2 = 1), which is unreachable on a real 3×3.
        // Validate must flag it as EdgeOrientationSum.
        let mut f = Facelets::solved(3);
        let u_ur = f.get(Face::U, 1, 2); // U-color sticker on the UR edge
        let r_ur = f.get(Face::R, 0, 1); // R-color sticker on the UR edge
        f.set(Face::U, 1, 2, r_ur);
        f.set(Face::R, 0, 1, u_ur);
        let errs = f.validate().unwrap_err();
        assert!(
            errs.iter().any(|e| matches!(e, FaceletValidationError::EdgeOrientationSum { .. })),
            "expected EdgeOrientationSum; got {errs:?}",
        );
    }

    #[test]
    fn validate_two_pieces_swap_reports_not_reachable() {
        // Swap the R-color sticker on URF position with the L-color
        // sticker on UFL. Each position now has a canonical color set
        // (just at the wrong location), but the resulting state would
        // require a reflection to reach — i.e. it's an odd permutation
        // and unreachable on a real cube. Validate must flag it.
        let mut f = Facelets::solved(3);
        let r = f.get(Face::R, 0, 0);
        let l = f.get(Face::L, 0, 2);
        f.set(Face::R, 0, 0, l);
        f.set(Face::L, 0, 2, r);
        let errs = f.validate().unwrap_err();
        assert!(errs.iter().any(|e| matches!(e, FaceletValidationError::NotReachable)),
            "expected NotReachable error; got {errs:?}");
    }
}
