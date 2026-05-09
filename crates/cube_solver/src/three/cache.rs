//! On-disk cache for [`PruningTables`].
//!
//! Building from scratch runs five BFS sweeps and takes a few seconds on
//! a mid-range CPU; loading from a pre-built file is essentially memcpy.
//! The format is intentionally simple: a fixed magic, a `u32` version,
//! the `udslice_goal` scalar, and length-prefixed byte arrays for each
//! pruning table. The version is bumped whenever the table layout, BFS
//! rules, or coord encodings change so stale caches are rejected on load
//! rather than silently producing wrong solutions.
//!
//! Errors on either end (missing file, bad magic, version mismatch, size
//! mismatch, truncated read) are returned as [`std::io::Error`]; callers
//! typically fall back to building from scratch and saving the result.

use std::fs;
use std::io::{self, Read, Write};
use std::path::Path;

use super::coords::{N_CO, N_CP, N_EO, N_EP, N_UDSLICE, N_UDSLICE_PERM};
use super::tables::PruningTables;

const MAGIC: &[u8; 8] = b"R3PRUN\0\0";
/// Bump on any change that affects table contents: BFS rules, coord
/// encodings, table sizes, or apply_*_op semantics.
pub const CACHE_VERSION: u32 = 1;

pub fn save(tables: &PruningTables, path: &Path) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    // Write to a sibling temp and atomic-rename so a partial write can't
    // leave a corrupt cache file in place (which would then fail loud
    // on next load — still better than recovery, but rename is cheap).
    let mut tmp = path.to_path_buf();
    tmp.set_extension("tmp");
    {
        let mut f = fs::File::create(&tmp)?;
        f.write_all(MAGIC)?;
        f.write_all(&CACHE_VERSION.to_le_bytes())?;
        f.write_all(&tables.udslice_goal.to_le_bytes())?;
        write_table(&mut f, &tables.p1_co_udslice)?;
        write_table(&mut f, &tables.p1_eo_udslice)?;
        write_table(&mut f, &tables.p1_co_eo)?;
        write_table(&mut f, &tables.p2_cp_uperm)?;
        write_table(&mut f, &tables.p2_ep_uperm)?;
        f.flush()?;
    }
    fs::rename(&tmp, path)?;
    Ok(())
}

pub fn load(path: &Path) -> io::Result<PruningTables> {
    let bytes = fs::read(path)?;
    let mut r = io::Cursor::new(&bytes[..]);

    let mut magic = [0u8; 8];
    r.read_exact(&mut magic)?;
    if &magic != MAGIC {
        return Err(io::Error::new(io::ErrorKind::InvalidData, "bad cache magic"));
    }
    let version = read_u32(&mut r)?;
    if version != CACHE_VERSION {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("cache version {version} != expected {CACHE_VERSION}"),
        ));
    }
    let udslice_goal = read_u16(&mut r)?;
    let p1_co_udslice = read_table(&mut r, N_CO * N_UDSLICE)?;
    let p1_eo_udslice = read_table(&mut r, N_EO * N_UDSLICE)?;
    let p1_co_eo = read_table(&mut r, N_CO * N_EO)?;
    let p2_cp_uperm = read_table(&mut r, N_CP * N_UDSLICE_PERM)?;
    let p2_ep_uperm = read_table(&mut r, N_EP * N_UDSLICE_PERM)?;

    Ok(PruningTables {
        p1_co_udslice,
        p1_eo_udslice,
        p1_co_eo,
        p2_cp_uperm,
        p2_ep_uperm,
        udslice_goal,
    })
}

fn write_table<W: Write>(w: &mut W, t: &[u8]) -> io::Result<()> {
    w.write_all(&(t.len() as u64).to_le_bytes())?;
    w.write_all(t)?;
    Ok(())
}

fn read_table<R: Read>(r: &mut R, expected: usize) -> io::Result<Vec<u8>> {
    let len = read_u64(r)? as usize;
    if len != expected {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("table size {len} != expected {expected}"),
        ));
    }
    let mut buf = vec![0u8; len];
    r.read_exact(&mut buf)?;
    Ok(buf)
}

fn read_u32<R: Read>(r: &mut R) -> io::Result<u32> {
    let mut b = [0u8; 4];
    r.read_exact(&mut b)?;
    Ok(u32::from_le_bytes(b))
}

fn read_u64<R: Read>(r: &mut R) -> io::Result<u64> {
    let mut b = [0u8; 8];
    r.read_exact(&mut b)?;
    Ok(u64::from_le_bytes(b))
}

fn read_u16<R: Read>(r: &mut R) -> io::Result<u16> {
    let mut b = [0u8; 2];
    r.read_exact(&mut b)?;
    Ok(u16::from_le_bytes(b))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::three::tables::{MoveTables, PruningTables};

    /// Build, save, load, and verify byte-for-byte equality on every
    /// pruning table plus the scalar goal.
    #[test]
    fn save_load_round_trip_preserves_tables() {
        let tmpdir = std::env::temp_dir().join(format!(
            "rubiks_cache_test_{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&tmpdir);
        let path = tmpdir.join("3x3_pruning_v1.bin");

        let move_tables = MoveTables::build();
        let pruning = PruningTables::build(&move_tables);
        save(&pruning, &path).expect("save");

        let loaded = load(&path).expect("load");
        assert_eq!(loaded.udslice_goal, pruning.udslice_goal);
        assert_eq!(loaded.p1_co_udslice, pruning.p1_co_udslice);
        assert_eq!(loaded.p1_eo_udslice, pruning.p1_eo_udslice);
        assert_eq!(loaded.p1_co_eo, pruning.p1_co_eo);
        assert_eq!(loaded.p2_cp_uperm, pruning.p2_cp_uperm);
        assert_eq!(loaded.p2_ep_uperm, pruning.p2_ep_uperm);

        std::fs::remove_dir_all(&tmpdir).ok();
    }

    #[test]
    fn load_rejects_bad_magic() {
        let tmpdir = std::env::temp_dir().join(format!(
            "rubiks_cache_test_bad_magic_{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&tmpdir);
        std::fs::create_dir_all(&tmpdir).unwrap();
        let path = tmpdir.join("bad.bin");
        std::fs::write(&path, b"NOTRIGHT\x01\x00\x00\x00").unwrap();
        let err = match load(&path) {
            Err(e) => e,
            Ok(_) => panic!("load should have failed"),
        };
        assert_eq!(err.kind(), std::io::ErrorKind::InvalidData);
        std::fs::remove_dir_all(&tmpdir).ok();
    }

    #[test]
    fn load_rejects_version_mismatch() {
        let tmpdir = std::env::temp_dir().join(format!(
            "rubiks_cache_test_version_{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&tmpdir);
        std::fs::create_dir_all(&tmpdir).unwrap();
        let path = tmpdir.join("ver.bin");
        let mut bytes = Vec::new();
        bytes.extend_from_slice(MAGIC);
        // wrong version
        bytes.extend_from_slice(&u32::MAX.to_le_bytes());
        std::fs::write(&path, &bytes).unwrap();
        let err = match load(&path) {
            Err(e) => e,
            Ok(_) => panic!("load should have failed"),
        };
        assert_eq!(err.kind(), std::io::ErrorKind::InvalidData);
        std::fs::remove_dir_all(&tmpdir).ok();
    }
}
