//! Coordinate encodings for the two-phase algorithm.
//!
//! Each coordinate is a small integer that captures one aspect of the cube
//! state. Move tables map `(coord, move) → coord`; pruning tables map
//! coordinate pairs to a lower-bound move count.
//!
//! ## Phase 1 coordinates (`G0 → G1`)
//! - `co` (corner orientation): `0..2187 = 3^7`
//! - `eo` (edge orientation): `0..2048 = 2^11`
//! - `udslice`: `0..495 = C(12, 4)` — which 4 of 12 slots hold E-slice edges
//!
//! ## Phase 2 coordinates (`G1 → solved`)
//! - `cp` (corner permutation): `0..40320 = 8!`
//! - `ep` (UD edge permutation): `0..40320 = 8!`
//! - `udslice_perm`: `0..24 = 4!` — permutation of E-slice edges within slots 8..12

use super::state::{CornerState, EdgeState, E_SLICE_FIRST};

pub const N_CO: usize = 2187; // 3^7
pub const N_EO: usize = 2048; // 2^11
pub const N_UDSLICE: usize = 495; // C(12, 4)
pub const N_CP: usize = 40320; // 8!
pub const N_EP: usize = 40320; // 8!
pub const N_UDSLICE_PERM: usize = 24; // 4!

// ---- Corner orientation ----

/// Encode `corner.orient[0..7]` as a base-3 integer. Index 7 is determined
/// by the sum-mod-3 invariant.
pub fn encode_co(c: &CornerState) -> u32 {
    let mut v: u32 = 0;
    for &o in &c.orient[..7] {
        v = v * 3 + o as u32;
    }
    v
}

pub fn decode_co(coord: u32) -> CornerState {
    let mut orient = [0u8; 8];
    let mut v = coord;
    for i in (0..7).rev() {
        orient[i] = (v % 3) as u8;
        v /= 3;
    }
    let sum: u32 = orient[..7].iter().map(|&x| x as u32).sum();
    orient[7] = ((3 - (sum % 3)) % 3) as u8;
    CornerState { perm: [0, 1, 2, 3, 4, 5, 6, 7], orient }
}

// ---- Edge orientation ----

pub fn encode_eo(e: &EdgeState) -> u32 {
    let mut v: u32 = 0;
    for &o in &e.orient[..11] {
        v = v * 2 + o as u32;
    }
    v
}

pub fn decode_eo(coord: u32) -> EdgeState {
    let mut orient = [0u8; 12];
    let mut v = coord;
    for i in (0..11).rev() {
        orient[i] = (v % 2) as u8;
        v /= 2;
    }
    let sum: u32 = orient[..11].iter().map(|&x| x as u32).sum();
    orient[11] = (sum % 2) as u8;
    EdgeState {
        perm: [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11],
        orient,
        // Decoded coord-only states don't carry a real rotation; downstream
        // code reads `orient` directly. Leaving `rot` at identity keeps the
        // state hashable and round-trippable through `encode_eo`.
        rot: [0; 12],
    }
}

// ---- UD-slice (which 4 slots hold E-slice edges) ----

const fn binomial(n: u32, k: u32) -> u32 {
    if k > n {
        return 0;
    }
    let mut num = 1u64;
    let mut den = 1u64;
    let k = if k > n - k { n - k } else { k };
    let mut i = 0u32;
    while i < k {
        num *= (n - i) as u64;
        den *= (i + 1) as u64;
        i += 1;
    }
    (num / den) as u32
}

/// Lex-rank encoding of which 4 of 12 slots hold E-slice edges. The "all
/// E-slice in slots 0..4" combination has coord 0; "all in slots 8..12"
/// has coord `N_UDSLICE - 1`.
pub fn encode_udslice(e: &EdgeState) -> u32 {
    let mut is_eslice = [false; 12];
    for slot in 0..12 {
        if e.perm[slot] as usize >= E_SLICE_FIRST {
            is_eslice[slot] = true;
        }
    }
    udslice_from_mask(&is_eslice)
}

pub fn udslice_from_mask(is_eslice: &[bool; 12]) -> u32 {
    let mut rank = 0u32;
    let mut prev = 0usize;
    let mut found = 0u32;
    for slot in 0..12 {
        if is_eslice[slot] {
            for v in prev..slot {
                let pool = (12 - v - 1) as u32;
                let remaining = 4 - found - 1;
                rank += binomial(pool, remaining);
            }
            prev = slot + 1;
            found += 1;
            if found == 4 {
                break;
            }
        }
    }
    rank
}

/// Decode a `udslice` coord to a slot-mask (true at the 4 E-slice slots).
pub fn decode_udslice_mask(coord: u32) -> [bool; 12] {
    let mut mask = [false; 12];
    let mut rem = coord;
    let mut found = 0u32;
    let mut prev = 0usize;
    let mut slot = 0usize;
    while found < 4 && slot < 12 {
        // Try placing the next E-slice at the smallest possible slot ≥ prev.
        // Skip slots while there's "budget" left in `rem`.
        let mut placed = false;
        for v in prev..12 {
            let pool = (12 - v - 1) as u32;
            let remaining = 4 - found - 1;
            let skip = binomial(pool, remaining);
            if rem < skip {
                mask[v] = true;
                prev = v + 1;
                slot = v + 1;
                found += 1;
                placed = true;
                break;
            }
            rem -= skip;
        }
        if !placed {
            break;
        }
    }
    mask
}

/// Fill an `EdgeState` with E-slice edges placed per `coord`, identity
/// orientation, and the 8 UD-layer edges in their home slots in lex order.
pub fn decode_udslice(coord: u32) -> EdgeState {
    let mask = decode_udslice_mask(coord);
    let mut perm = [0u8; 12];
    let mut ud_iter = 0u8..8u8;
    let mut es_iter = 8u8..12u8;
    for (slot, &is_es) in mask.iter().enumerate() {
        if is_es {
            perm[slot] = es_iter.next().unwrap();
        } else {
            perm[slot] = ud_iter.next().unwrap();
        }
    }
    EdgeState { perm, orient: [0; 12], rot: [0; 12] }
}

// ---- Corner permutation (Phase 2) ----

pub fn encode_cp(c: &CornerState) -> u32 {
    lex_rank_8(&c.perm)
}

pub fn decode_cp(coord: u32) -> CornerState {
    let perm = lex_unrank_8(coord);
    CornerState { perm, orient: [0; 8] }
}

// ---- UD edge permutation (Phase 2: edges 0..8 within slots 0..8) ----

pub fn encode_ep(e: &EdgeState) -> u32 {
    let mut p = [0u8; 8];
    p.copy_from_slice(&e.perm[..8]);
    lex_rank_8(&p)
}

pub fn decode_ep(coord: u32) -> EdgeState {
    let p = lex_unrank_8(coord);
    let mut perm = [0u8; 12];
    perm[..8].copy_from_slice(&p);
    perm[8..].copy_from_slice(&[8, 9, 10, 11]);
    EdgeState { perm, orient: [0; 12], rot: [0; 12] }
}

// ---- E-slice permutation (Phase 2: edges 8..12 within slots 8..12) ----

pub fn encode_udslice_perm(e: &EdgeState) -> u32 {
    let mut p = [0u8; 4];
    for i in 0..4 {
        p[i] = e.perm[8 + i] - 8;
    }
    lex_rank_4(&p)
}

pub fn decode_udslice_perm(coord: u32) -> EdgeState {
    let p = lex_unrank_4(coord);
    let mut perm = [0u8; 12];
    for i in 0..8 {
        perm[i] = i as u8;
    }
    for i in 0..4 {
        perm[8 + i] = p[i] + 8;
    }
    EdgeState { perm, orient: [0; 12], rot: [0; 12] }
}

// ---- Lex rank / unrank helpers ----

fn lex_rank_8(perm: &[u8; 8]) -> u32 {
    let mut rank = 0u32;
    for i in 0..8 {
        let mut count = 0u32;
        for j in (i + 1)..8 {
            if perm[j] < perm[i] {
                count += 1;
            }
        }
        rank = rank * (8 - i as u32) + count;
    }
    rank
}

fn lex_unrank_8(mut idx: u32) -> [u8; 8] {
    let mut pool: Vec<u8> = (0..8).collect();
    let mut perm = [0u8; 8];
    for (i, slot) in perm.iter_mut().enumerate() {
        let base = (8 - i) as u32;
        let factorial: u32 = (1..base).product::<u32>().max(1);
        let pick = (idx / factorial) as usize;
        idx %= factorial;
        *slot = pool.remove(pick);
    }
    perm
}

fn lex_rank_4(perm: &[u8; 4]) -> u32 {
    let mut rank = 0u32;
    for i in 0..4 {
        let mut count = 0u32;
        for j in (i + 1)..4 {
            if perm[j] < perm[i] {
                count += 1;
            }
        }
        rank = rank * (4 - i as u32) + count;
    }
    rank
}

fn lex_unrank_4(mut idx: u32) -> [u8; 4] {
    let mut pool: Vec<u8> = (0..4).collect();
    let mut perm = [0u8; 4];
    for (i, slot) in perm.iter_mut().enumerate() {
        let base = (4 - i) as u32;
        let factorial: u32 = (1..base).product::<u32>().max(1);
        let pick = (idx / factorial) as usize;
        idx %= factorial;
        *slot = pool.remove(pick);
    }
    perm
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn co_round_trip() {
        for v in [0, 1, 1093, 2186] {
            let s = decode_co(v);
            assert_eq!(encode_co(&s), v);
        }
    }

    #[test]
    fn eo_round_trip() {
        for v in [0, 1, 1024, 2047] {
            let s = decode_eo(v);
            assert_eq!(encode_eo(&s), v);
        }
    }

    #[test]
    fn udslice_round_trip() {
        for v in [0, 1, 247, 494] {
            let s = decode_udslice(v);
            assert_eq!(encode_udslice(&s), v);
        }
    }

    #[test]
    fn udslice_solved_value() {
        // Solved cube: E-slice edges in slots 8..12.
        // That's the lex-LAST combination → coord = N_UDSLICE - 1.
        let solved = EdgeState::solved();
        assert_eq!(encode_udslice(&solved) as usize, N_UDSLICE - 1);
    }

    #[test]
    fn cp_round_trip() {
        for v in [0u32, 1, 9999, 40319] {
            let s = decode_cp(v);
            assert_eq!(encode_cp(&s), v);
        }
    }

    #[test]
    fn ep_round_trip() {
        for v in [0u32, 1, 9999, 40319] {
            let s = decode_ep(v);
            assert_eq!(encode_ep(&s), v);
        }
    }

    #[test]
    fn udslice_perm_round_trip() {
        for v in 0u32..24 {
            let s = decode_udslice_perm(v);
            assert_eq!(encode_udslice_perm(&s), v);
        }
    }

    #[test]
    fn binomial_smoke() {
        assert_eq!(binomial(12, 4), 495);
        assert_eq!(binomial(12, 0), 1);
        assert_eq!(binomial(12, 12), 1);
        assert_eq!(binomial(0, 0), 1);
    }
}
