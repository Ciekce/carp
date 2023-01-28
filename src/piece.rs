/// Structures to represent a piece along with its color
use std::fmt;
use std::ops::Not;

use crate::from;

/// Piece/Player color enum
#[repr(u8)]
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Debug, Hash)]
pub enum Color {
    White,
    Black,
}
use Color::*;

impl Not for Color {
    type Output = Color;

    // get opposite color
    fn not(self) -> Color {
        from!(self as u8 ^ 1, 1)
    }
}

/// Implement functions to get each piece based on color
macro_rules! impl_conversions {
    ($($piece:ident, $val:literal),*) => {
        $(
            impl Color {
                pub const fn $piece(self) -> Piece {
                    from!($val + self as u8, 15)
                }
            }
        )*
    };
}

impl_conversions! {
    pawn,   0x00u8,
    knight, 0x02u8,
    bishop, 0x04u8,
    rook,   0x06u8,
    queen,  0x08u8,
    king,   0x0Au8
}

/// Pretty print color to string
impl fmt::Display for Color {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}",
            match self {
                White => "White",
                Black => "Black",
            }
        )
    }
}

/// Chess Piece enum (includes color)
/// Pieces alternate between Black and White so that the least significant bit is the color
#[repr(u8)]
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Debug, Hash)]
#[rustfmt::skip]
pub enum Piece {
    WP, BP, WN, BN, WB, BB, WR, BR, WQ, BQ, WK, BK
}
use Piece::*;

pub const PIECE_COUNT: usize = 12;

/// Piece index constants
pub const WPAWN: usize = 0;
pub const BPAWN: usize = 1;
pub const WKNIGHT: usize = 2;
pub const BKNIGHT: usize = 3;
pub const WBISHOP: usize = 4;
pub const BBISHOP: usize = 5;
pub const WROOK: usize = 6;
pub const BROOK: usize = 7;
pub const WQUEEN: usize = 8;
pub const BQUEEN: usize = 9;
pub const WKING: usize = 10;
pub const BKING: usize = 11;

#[rustfmt::skip]
pub const ALL_PIECES: [Piece; PIECE_COUNT] = [
    WP, BP, WN, BN, WB, BB,
    WR, BR, WQ, BQ, WK, BK,
];

/// All pieces indexed by color
#[rustfmt::skip]
pub const PIECES: [[Piece; 6]; 2] = [
    [ WP, WN, WB, WR, WQ, WK ],
    [ BP, BN, BB, BR, BQ, BK ]
];

/// All possible promotions, ordered by "usefulness"
#[rustfmt::skip]
pub const PROMOTIONS: [[Piece; 4]; 2] = [
    [ WQ, WN, WR, WB ],
    [ BQ, BN, BR, BB ]
];

// used for printing/reading pieces
#[rustfmt::skip]
const PIECE_CHAR: [char; PIECE_COUNT] = [
    'P', 'p', 'N', 'n', 'B', 'b',
    'R', 'r', 'Q', 'q', 'K', 'k',
];

#[rustfmt::skip]
const PIECE_UNICODE: [char; PIECE_COUNT] = [
    '♟', '♙', '♞', '♘', '♝', '♗',
    '♜', '♖', '♛', '♕', '♚', '♔',
];

/// Prints piece as unicode character
impl fmt::Display for Piece {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", PIECE_UNICODE[*self as usize])
    }
}

/// Reads piece from uci string
impl TryFrom<char> for Piece {
    type Error = &'static str;

    fn try_from(value: char) -> Result<Self, Self::Error> {
        Ok(Self::from(
            PIECE_CHAR
                .iter()
                .position(|&x| x == value)
                .ok_or("Invalid piece!")?,
        ))
    }
}

/// Create piece from usize index
///
/// UB:
/// If 12 <= index mod 16 <=15 this will try to transmute to a non-existent piece
/// Simply use indices that make sense
impl From<usize> for Piece {
    fn from(index: usize) -> Self {
        from!(index as u8, 15)
    }
}

impl Piece {
    /// Returns fen formatted piece
    pub const fn to_char(self) -> char {
        PIECE_CHAR[self as usize]
    }

    /// Get piece color
    pub const fn color(self) -> Color {
        from!(self as u8, 1)
    }

    /// Switch piece color
    pub const fn opposite_color(self) -> Piece {
        from!(self as u8 ^ 1, 15) // ^1 flips color bit
    }
}
