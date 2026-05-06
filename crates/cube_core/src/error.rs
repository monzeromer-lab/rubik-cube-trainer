use crate::facelets::Color;
use glam::IVec3;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ParseError {
    #[error("unexpected character {0:?} at position {1}")]
    UnexpectedChar(char, usize),
    #[error("unexpected end of input")]
    UnexpectedEnd,
    #[error("invalid layer count {0} (must be ≥ 1)")]
    InvalidLayerCount(u8),
    #[error("expected face symbol after layer count at position {0}")]
    ExpectedFaceAfterLayer(usize),
}

#[derive(Debug, Error)]
pub enum CubeError {
    #[error("invalid cube size {0} (supported: 2..=5)")]
    InvalidSize(u8),
    #[error("move depth {depth} exceeds cube size {size}")]
    DepthOutOfRange { depth: u8, size: u8 },
    #[error("move depth must be ≥ 1")]
    ZeroDepth,
    #[error("facelet array has wrong length: got {got}, expected {expected}")]
    FaceletLength { got: usize, expected: usize },
    #[error("facelet state is not a valid cube ({reason})")]
    InvalidFaceletState { reason: String },
}

/// Fine-grained validation failures for sticker input. Multiple of these
/// can be reported at once so the UI can highlight every problem.
#[derive(Debug, Clone, Error, PartialEq, Eq)]
pub enum FaceletValidationError {
    #[error("facelet array has wrong length: got {got}, expected {expected}")]
    BadFaceletLength { got: usize, expected: usize },

    #[error("color {color:?} appears {got} times; expected {expected}")]
    BadColorCount { color: Color, got: usize, expected: usize },

    #[error("piece at {pos:?} has opposite colors {a:?} and {b:?} on the same cubie")]
    OppositeColorsOnPiece { pos: IVec3, a: Color, b: Color },

    #[error("piece at {pos:?} has duplicate color {color:?}")]
    DuplicateColorOnPiece { pos: IVec3, color: Color },

    #[error("no canonical cubie has the sticker set found at {pos:?}: {colors:?}")]
    UnknownCubieAt { pos: IVec3, colors: Vec<Color> },

    #[error("corner orientation sum is {sum} (mod 3 = {modulus}); must be 0 — a single corner is twisted")]
    CornerOrientationSum { sum: u32, modulus: u32 },

    #[error("edge orientation sum is {sum} (mod 2 = {modulus}); must be 0 — a single edge is flipped")]
    EdgeOrientationSum { sum: u32, modulus: u32 },

    #[error("permutation parity mismatch: corner perm has parity {corner_parity}, edge perm {edge_parity} — two pieces are swapped")]
    PermParityMismatch { corner_parity: i32, edge_parity: i32 },

    #[error("the sticker arrangement is not reachable on a real cube — likely two pieces are swapped or a piece is twisted/flipped impossibly")]
    NotReachable,
}
