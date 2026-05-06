//! The 24-element chiral octahedral rotation group: every rotation that maps
//! a cube to itself. Each element is a 3x3 signed-permutation matrix with
//! determinant +1.
//!
//! We use integer rotations (not quaternions) so that long move sequences
//! never accumulate float drift — orientation after 50_000 moves is exact.
//!
//! Encoding: an [`Orient`] is a `u8` index into a table of 24 [`OrientData`]
//! entries. Composition is a precomputed 24×24 lookup, application to an
//! `IVec3` is a 3-element signed permutation.

use glam::IVec3;
use std::sync::LazyLock;

/// A single rotation element of the cube symmetry group.
///
/// Stored as: for each output axis `i`, the input axis it draws from
/// (`out_from[i]`) and a sign (`out_sign[i]` ∈ {-1, +1}).
///
/// Equivalent matrix: row `i` has a single nonzero entry at column
/// `out_from[i]` with value `out_sign[i]`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct OrientData {
    pub out_from: [u8; 3],
    pub out_sign: [i8; 3],
}

impl OrientData {
    pub const IDENTITY: Self = Self {
        out_from: [0, 1, 2],
        out_sign: [1, 1, 1],
    };

    /// Apply this rotation to a vector.
    #[inline]
    pub fn apply(&self, v: IVec3) -> IVec3 {
        let arr = [v.x, v.y, v.z];
        IVec3::new(
            self.out_sign[0] as i32 * arr[self.out_from[0] as usize],
            self.out_sign[1] as i32 * arr[self.out_from[1] as usize],
            self.out_sign[2] as i32 * arr[self.out_from[2] as usize],
        )
    }

    /// Matrix product `a * b`: applying `compose(a, b)` to v equals
    /// `a.apply(b.apply(v))`.
    #[inline]
    pub fn compose(a: &Self, b: &Self) -> Self {
        let mut out_from = [0u8; 3];
        let mut out_sign = [0i8; 3];
        for i in 0..3 {
            let mid = a.out_from[i] as usize;
            out_from[i] = b.out_from[mid];
            out_sign[i] = a.out_sign[i] * b.out_sign[mid];
        }
        Self { out_from, out_sign }
    }

    /// Determinant of the underlying matrix. Used to filter the 48 signed
    /// permutations down to the 24 with det=+1 (rotations only, no reflections).
    fn det(&self) -> i8 {
        // Sign of permutation × product of signs.
        let perm_sign = perm_sign_3(self.out_from);
        let s = self.out_sign[0] * self.out_sign[1] * self.out_sign[2];
        perm_sign * s
    }
}

fn perm_sign_3(p: [u8; 3]) -> i8 {
    let mut inv = 0;
    for i in 0..3 {
        for j in (i + 1)..3 {
            if p[i] > p[j] {
                inv += 1;
            }
        }
    }
    if inv % 2 == 0 { 1 } else { -1 }
}

fn enumerate_all_24() -> Vec<OrientData> {
    let perms: [[u8; 3]; 6] = [
        [0, 1, 2], [0, 2, 1], [1, 0, 2],
        [1, 2, 0], [2, 0, 1], [2, 1, 0],
    ];
    let mut out = Vec::with_capacity(24);
    for p in perms {
        for s0 in [-1i8, 1] {
            for s1 in [-1i8, 1] {
                for s2 in [-1i8, 1] {
                    let cand = OrientData { out_from: p, out_sign: [s0, s1, s2] };
                    if cand.det() == 1 {
                        out.push(cand);
                    }
                }
            }
        }
    }
    debug_assert_eq!(out.len(), 24);
    out
}

/// Sorted, identity-first table of all 24 rotations.
static ORIENTS: LazyLock<[OrientData; 24]> = LazyLock::new(|| {
    let mut all = enumerate_all_24();
    // Place identity at index 0 for ergonomic `Orient::IDENTITY`.
    let id_idx = all.iter().position(|o| *o == OrientData::IDENTITY).unwrap();
    all.swap(0, id_idx);
    let mut arr = [OrientData::IDENTITY; 24];
    arr.copy_from_slice(&all);
    arr
});

/// Composition table: `COMPOSE[a][b]` is the index of `ORIENTS[a] * ORIENTS[b]`.
static COMPOSE: LazyLock<[[u8; 24]; 24]> = LazyLock::new(|| {
    let mut table = [[0u8; 24]; 24];
    for a in 0..24 {
        for b in 0..24 {
            let c = OrientData::compose(&ORIENTS[a], &ORIENTS[b]);
            let idx = ORIENTS
                .iter()
                .position(|o| *o == c)
                .expect("composition must land in the group");
            table[a][b] = idx as u8;
        }
    }
    table
});

/// Inverse table: `INVERSE[a]` is the index of `ORIENTS[a]^{-1}`.
static INVERSE: LazyLock<[u8; 24]> = LazyLock::new(|| {
    let mut inv = [0u8; 24];
    for a in 0..24 {
        for b in 0..24 {
            if COMPOSE[a][b] == 0 {
                inv[a] = b as u8;
                break;
            }
        }
    }
    inv
});

/// An element of the cube rotation group, encoded as an index 0..24.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct Orient(pub u8);

impl Orient {
    pub const IDENTITY: Self = Self(0);

    /// Look up the underlying matrix data.
    #[inline]
    pub fn data(self) -> OrientData {
        ORIENTS[self.0 as usize]
    }

    /// Apply this rotation to an integer vector.
    #[inline]
    pub fn apply(self, v: IVec3) -> IVec3 {
        self.data().apply(v)
    }

    /// `self * other` — applying the result is equivalent to
    /// applying `other` then `self`.
    #[inline]
    pub fn compose(self, other: Self) -> Self {
        Self(COMPOSE[self.0 as usize][other.0 as usize])
    }

    #[inline]
    pub fn inverse(self) -> Self {
        Self(INVERSE[self.0 as usize])
    }
}

impl Default for Orient {
    fn default() -> Self {
        Self::IDENTITY
    }
}

/// Find the [`Orient`] index matching given matrix data, panicking if not in group.
fn find(data: OrientData) -> Orient {
    let idx = ORIENTS
        .iter()
        .position(|o| *o == data)
        .expect("data must be a cube-group rotation");
    Orient(idx as u8)
}

/// Initialize generator-level rotations. Cached after first access.
static GENERATORS: LazyLock<Generators> = LazyLock::new(|| {
    // +90° rotation around +X (right-hand rule): y → z, z → -y, x fixed.
    // Matrix [[1,0,0],[0,0,-1],[0,1,0]]: out[0]=v[0], out[1]=-v[2], out[2]=v[1].
    let rx_cw = OrientData { out_from: [0, 2, 1], out_sign: [1, -1, 1] };
    // +90° rotation around +Y: z → x, x → -z, y fixed.
    // Matrix [[0,0,1],[0,1,0],[-1,0,0]]: out[0]=v[2], out[1]=v[1], out[2]=-v[0].
    let ry_cw = OrientData { out_from: [2, 1, 0], out_sign: [1, 1, -1] };
    // +90° rotation around +Z: x → y, y → -x, z fixed.
    // Matrix [[0,-1,0],[1,0,0],[0,0,1]]: out[0]=-v[1], out[1]=v[0], out[2]=v[2].
    let rz_cw = OrientData { out_from: [1, 0, 2], out_sign: [-1, 1, 1] };

    let rx_2 = OrientData::compose(&rx_cw, &rx_cw);
    let ry_2 = OrientData::compose(&ry_cw, &ry_cw);
    let rz_2 = OrientData::compose(&rz_cw, &rz_cw);
    let rx_ccw = OrientData::compose(&rx_2, &rx_cw);
    let ry_ccw = OrientData::compose(&ry_2, &ry_cw);
    let rz_ccw = OrientData::compose(&rz_2, &rz_cw);

    Generators {
        rx_cw: find(rx_cw),
        rx_2: find(rx_2),
        rx_ccw: find(rx_ccw),
        ry_cw: find(ry_cw),
        ry_2: find(ry_2),
        ry_ccw: find(ry_ccw),
        rz_cw: find(rz_cw),
        rz_2: find(rz_2),
        rz_ccw: find(rz_ccw),
    }
});

#[derive(Debug, Clone, Copy)]
pub struct Generators {
    pub rx_cw: Orient,
    pub rx_2: Orient,
    pub rx_ccw: Orient,
    pub ry_cw: Orient,
    pub ry_2: Orient,
    pub ry_ccw: Orient,
    pub rz_cw: Orient,
    pub rz_2: Orient,
    pub rz_ccw: Orient,
}

pub fn generators() -> &'static Generators {
    &GENERATORS
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn group_has_24_elements() {
        let _ = &*ORIENTS;
        assert_eq!(ORIENTS.len(), 24);
    }

    #[test]
    fn identity_is_zero() {
        assert_eq!(Orient::IDENTITY.data(), OrientData::IDENTITY);
    }

    #[test]
    fn composition_with_identity_is_self() {
        for i in 0..24 {
            let o = Orient(i);
            assert_eq!(o.compose(Orient::IDENTITY), o);
            assert_eq!(Orient::IDENTITY.compose(o), o);
        }
    }

    #[test]
    fn inverse_round_trip() {
        for i in 0..24 {
            let o = Orient(i);
            assert_eq!(o.compose(o.inverse()), Orient::IDENTITY);
            assert_eq!(o.inverse().compose(o), Orient::IDENTITY);
        }
    }

    #[test]
    fn rx_cw_four_times_is_identity() {
        let g = generators();
        let r = g.rx_cw;
        let four = r.compose(r).compose(r).compose(r);
        assert_eq!(four, Orient::IDENTITY);
    }

    #[test]
    fn rx_cw_rotates_y_to_z() {
        let g = generators();
        let v = IVec3::new(0, 1, 0);
        let out = g.rx_cw.apply(v);
        assert_eq!(out, IVec3::new(0, 0, 1));
    }

    #[test]
    fn ry_cw_rotates_z_to_x() {
        let g = generators();
        let v = IVec3::new(0, 0, 1);
        let out = g.ry_cw.apply(v);
        assert_eq!(out, IVec3::new(1, 0, 0));
    }

    #[test]
    fn rz_cw_rotates_x_to_y() {
        let g = generators();
        let v = IVec3::new(1, 0, 0);
        let out = g.rz_cw.apply(v);
        assert_eq!(out, IVec3::new(0, 1, 0));
    }

    #[test]
    fn apply_matches_compose_apply() {
        // (a*b).apply(v) == a.apply(b.apply(v))
        let g = generators();
        let a = g.rx_cw;
        let b = g.ry_cw;
        let v = IVec3::new(2, -3, 5);
        assert_eq!(a.compose(b).apply(v), a.apply(b.apply(v)));
    }
}
