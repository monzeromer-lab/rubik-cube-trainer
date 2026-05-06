//! Pure-logic Rubik's cube domain model.
//!
//! Three coexisting representations of cube state:
//! - **Cubie model** ([`Cube`], [`Cubie`]): source of truth. Integer lattice
//!   positions and a 24-element rotation-group orientation per piece.
//! - **Facelet model** ([`Facelets`], [`Color`]): flat 6×N² sticker array.
//!   Drives sticker-input mode and is the natural debug view.
//! - **Coordinate model** (3×3 only, lives in the `cube_solver` crate).
//!
//! No engine or rendering dependencies — this crate is fully testable as a
//! plain library.

pub mod cube;
pub mod error;
pub mod facelets;
pub mod moves;
pub mod orient;
pub mod parser;
pub mod scramble;

pub use cube::{Cube, Cubie, MAX_SIZE, MIN_SIZE, corner_twist, edge_orient};
pub use error::{CubeError, FaceletValidationError, ParseError};
pub use facelets::{Color, Facelets};
pub use moves::{CubeRotation, Face, Move, MoveSeq, Turn};
pub use orient::Orient;
pub use scramble::{default_length, random_move_scramble, wca_scramble};
