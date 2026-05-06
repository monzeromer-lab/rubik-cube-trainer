//! WCA-style notation parser and pretty-printer.
//!
//! Accepts:
//! - Face turns: `U D L R F B` with optional modifier `'` or `2`.
//! - Wide turns: `Rw`, `r` (lowercase) — both mean "outer two layers".
//! - Layered: `2R` (slice 2 only), `3Rw` (outer three layers).
//! - Slice moves: `M E S` (3×3 middle slices, encoded as inner-layer moves).
//! - Cube rotations: `x y z` (and `X Y Z`); these expand to size-`N` wide moves.
//! - Whitespace ignored between moves.
//!
//! `MoveSeq::to_string` produces a canonical re-parseable string.

use crate::error::ParseError;
use crate::moves::{CubeRotation, Face, Move, MoveSeq, Turn};

impl MoveSeq {
    /// Parse a WCA-style notation string for a cube of the given size.
    pub fn parse(input: &str, size: u8) -> Result<Self, ParseError> {
        let mut p = Parser::new(input, size);
        let mut out = Vec::new();
        loop {
            p.skip_ws();
            if p.eof() {
                break;
            }
            out.push(p.parse_token()?);
        }
        Ok(Self(out))
    }

}

impl std::fmt::Display for MoveSeq {
    /// Pretty-print as canonical WCA notation, space-separated.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        for (i, m) in self.0.iter().enumerate() {
            if i > 0 {
                f.write_str(" ")?;
            }
            let mut buf = String::new();
            format_move(m, &mut buf);
            f.write_str(&buf)?;
        }
        Ok(())
    }
}

fn format_move(m: &Move, out: &mut String) {
    let suffix = match m.turn {
        Turn::Cw => "",
        Turn::Ccw => "'",
        Turn::Half => "2",
    };
    let face_char = m.face.symbol();
    if m.wide && m.depth == 1 {
        // Wide-1 is identical to single-layer; emit canonical single-layer form.
        out.push(face_char);
    } else if m.wide && m.depth == 2 {
        out.push(face_char);
        out.push('w');
    } else if m.wide {
        // 3-layer wide and beyond.
        push_u8(out, m.depth);
        out.push(face_char);
        out.push('w');
    } else if m.depth == 1 {
        out.push(face_char);
    } else {
        // Inner-slice single-layer turn.
        push_u8(out, m.depth);
        out.push(face_char);
    }
    out.push_str(suffix);
}

fn push_u8(s: &mut String, n: u8) {
    use std::fmt::Write;
    let _ = write!(s, "{n}");
}

struct Parser<'a> {
    src: &'a [u8],
    pos: usize,
    size: u8,
}

impl<'a> Parser<'a> {
    fn new(src: &'a str, size: u8) -> Self {
        Self { src: src.as_bytes(), pos: 0, size }
    }

    fn eof(&self) -> bool {
        self.pos >= self.src.len()
    }

    fn peek(&self) -> Option<u8> {
        self.src.get(self.pos).copied()
    }

    fn bump(&mut self) -> Option<u8> {
        let c = self.peek()?;
        self.pos += 1;
        Some(c)
    }

    fn skip_ws(&mut self) {
        while let Some(c) = self.peek() {
            if c.is_ascii_whitespace() {
                self.pos += 1;
            } else {
                break;
            }
        }
    }

    fn read_layer_count(&mut self) -> Option<u8> {
        let start = self.pos;
        while let Some(c) = self.peek() {
            if c.is_ascii_digit() {
                self.pos += 1;
            } else {
                break;
            }
        }
        if self.pos == start {
            return None;
        }
        let s = std::str::from_utf8(&self.src[start..self.pos]).ok()?;
        s.parse::<u8>().ok()
    }

    fn parse_token(&mut self) -> Result<Move, ParseError> {
        let token_start = self.pos;
        let layer = self.read_layer_count();
        let c = self.bump().ok_or(ParseError::UnexpectedEnd)?;

        // Branch on character class.
        let (face, mut depth, mut wide) = match c {
            b'U' => (Face::U, 1u8, false),
            b'D' => (Face::D, 1, false),
            b'L' => (Face::L, 1, false),
            b'R' => (Face::R, 1, false),
            b'F' => (Face::F, 1, false),
            b'B' => (Face::B, 1, false),
            b'u' => (Face::U, 2, true),
            b'd' => (Face::D, 2, true),
            b'l' => (Face::L, 2, true),
            b'r' => (Face::R, 2, true),
            b'f' => (Face::F, 2, true),
            b'b' => (Face::B, 2, true),
            // Slice moves on 3×3: encoded as depth=2 single-layer of the matching face.
            // M follows L direction, E follows D, S follows F.
            b'M' => (Face::L, 2, false),
            b'E' => (Face::D, 2, false),
            b'S' => (Face::F, 2, false),
            // Cube rotations: lowered to wide turns covering all layers.
            b'x' | b'X' => {
                if layer.is_some() {
                    return Err(ParseError::UnexpectedChar(c as char, token_start));
                }
                let turn = self.read_turn_modifier();
                return Ok(CubeRotation::X(turn).as_move(self.size));
            }
            b'y' | b'Y' => {
                if layer.is_some() {
                    return Err(ParseError::UnexpectedChar(c as char, token_start));
                }
                let turn = self.read_turn_modifier();
                return Ok(CubeRotation::Y(turn).as_move(self.size));
            }
            b'z' | b'Z' => {
                if layer.is_some() {
                    return Err(ParseError::UnexpectedChar(c as char, token_start));
                }
                let turn = self.read_turn_modifier();
                return Ok(CubeRotation::Z(turn).as_move(self.size));
            }
            other => return Err(ParseError::UnexpectedChar(other as char, self.pos - 1)),
        };

        // Optional `w` after uppercase face → wide, depth defaults to 2 unless
        // an explicit layer count was given. Lowercase already implied wide.
        let is_uppercase_face = matches!(c, b'U' | b'D' | b'L' | b'R' | b'F' | b'B');
        let mut saw_w = false;
        if is_uppercase_face && let Some(b'w') = self.peek() {
            self.pos += 1;
            wide = true;
            depth = 2;
            saw_w = true;
        }

        // Layer count overrides default depth. Only valid for uppercase + (optional)w
        // and for slice moves (`M E S`). Lowercase + layer is rejected as ambiguous.
        if let Some(d) = layer {
            if d == 0 {
                return Err(ParseError::InvalidLayerCount(d));
            }
            let lowercase_face = matches!(c, b'u' | b'd' | b'l' | b'r' | b'f' | b'b');
            let slice = matches!(c, b'M' | b'E' | b'S');
            if lowercase_face || slice {
                return Err(ParseError::UnexpectedChar(c as char, token_start));
            }
            depth = d;
            // For uppercase face with explicit layer and no `w`, keep wide=false (slice).
            if !saw_w {
                wide = false;
            }
        }

        let turn = self.read_turn_modifier();
        Ok(Move { face, turn, depth, wide })
    }

    fn read_turn_modifier(&mut self) -> Turn {
        match self.peek() {
            Some(b'\'') => {
                self.pos += 1;
                Turn::Ccw
            }
            Some(b'2') => {
                self.pos += 1;
                Turn::Half
            }
            _ => Turn::Cw,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(s: &str) -> MoveSeq {
        MoveSeq::parse(s, 3).unwrap()
    }

    #[test]
    fn parse_simple_face_turn() {
        let seq = parse("R");
        assert_eq!(seq.0, vec![Move::face(Face::R, Turn::Cw)]);
    }

    #[test]
    fn parse_inverse() {
        let seq = parse("R'");
        assert_eq!(seq.0, vec![Move::face(Face::R, Turn::Ccw)]);
    }

    #[test]
    fn parse_half() {
        let seq = parse("R2");
        assert_eq!(seq.0, vec![Move::face(Face::R, Turn::Half)]);
    }

    #[test]
    fn parse_sequence_with_whitespace() {
        let seq = parse("R U R' U'");
        assert_eq!(seq.0.len(), 4);
    }

    #[test]
    fn parse_wide_uppercase() {
        let seq = parse("Rw");
        assert_eq!(seq.0, vec![Move { face: Face::R, turn: Turn::Cw, depth: 2, wide: true }]);
    }

    #[test]
    fn parse_wide_lowercase() {
        let seq = parse("r");
        assert_eq!(seq.0, vec![Move { face: Face::R, turn: Turn::Cw, depth: 2, wide: true }]);
    }

    #[test]
    fn parse_layered_wide() {
        let seq = MoveSeq::parse("3Rw", 5).unwrap();
        assert_eq!(seq.0, vec![Move { face: Face::R, turn: Turn::Cw, depth: 3, wide: true }]);
    }

    #[test]
    fn parse_slice_only() {
        // 2R = depth 2 single layer (slice between R and middle).
        let seq = MoveSeq::parse("2R", 4).unwrap();
        assert_eq!(seq.0, vec![Move { face: Face::R, turn: Turn::Cw, depth: 2, wide: false }]);
    }

    #[test]
    fn parse_middle_slice_m() {
        let seq = parse("M");
        assert_eq!(seq.0, vec![Move { face: Face::L, turn: Turn::Cw, depth: 2, wide: false }]);
    }

    #[test]
    fn parse_cube_rotation_x() {
        // x on 3×3 = R wide depth 3.
        let seq = parse("x");
        assert_eq!(seq.0, vec![Move { face: Face::R, turn: Turn::Cw, depth: 3, wide: true }]);
    }

    #[test]
    fn parse_cube_rotation_y_prime() {
        let seq = parse("y'");
        assert_eq!(seq.0, vec![Move { face: Face::U, turn: Turn::Ccw, depth: 3, wide: true }]);
    }

    #[test]
    fn empty_string_parses_to_empty_seq() {
        assert!(parse("").is_empty());
        assert!(parse("   ").is_empty());
    }

    #[test]
    fn lowercase_with_layer_count_is_rejected() {
        assert!(MoveSeq::parse("2r", 3).is_err());
    }

    #[test]
    fn unknown_character_is_rejected() {
        assert!(MoveSeq::parse("Q", 3).is_err());
    }

    #[test]
    fn round_trip_basic() {
        let original = "R U R' U' R U2 Rw F'";
        let seq = MoveSeq::parse(original, 3).unwrap();
        let printed = seq.to_string();
        let reparsed = MoveSeq::parse(&printed, 3).unwrap();
        assert_eq!(seq, reparsed);
    }

    #[test]
    fn pretty_print_canonical_forms() {
        let m1 = Move::face(Face::R, Turn::Cw);
        let m2 = Move { face: Face::R, turn: Turn::Half, depth: 1, wide: false };
        let m3 = Move { face: Face::R, turn: Turn::Cw, depth: 2, wide: true };
        let m4 = Move { face: Face::R, turn: Turn::Ccw, depth: 3, wide: true };
        let m5 = Move { face: Face::R, turn: Turn::Cw, depth: 2, wide: false };
        let seq = MoveSeq::from_vec(vec![m1, m2, m3, m4, m5]);
        assert_eq!(seq.to_string(), "R R2 Rw 3Rw' 2R");
    }

    #[test]
    fn inverse_round_trips() {
        let seq = parse("R U R' U' F2 R");
        let inv = seq.inverse();
        let combined = MoveSeq::from_vec({
            let mut v = seq.0.clone();
            v.extend(inv.0);
            v
        });
        assert!(combined.cancel().is_empty());
    }
}
