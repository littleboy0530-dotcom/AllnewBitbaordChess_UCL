pub type Bitboard = u64;
pub type Square = u8;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Color { White = 0, Black = 1 }

impl Color {
    pub fn flip(self) -> Color { 
        if self == Color::Black{
            Color::White
        }else{
            Color::Black
        }
    }// 백 ↔ 흑
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Piece { Pawn = 0, Knight = 1, Bishop = 2, Rook = 3, Queen = 4, King = 5 }

pub fn square(file: u8, rank: u8) -> Square { rank*8 + file }   // (0~7, 0~7) → 0~63
pub fn file_of(sq: Square) -> u8 { sq % 8 }
pub fn rank_of(sq: Square) -> u8 { sq / 8 }
pub fn bit(sq: Square) -> Bitboard { 1u64 << sq }            // 그 칸만 켜진 비트보드

/// 가장 낮은 비트의 칸을 돌려주고 bb에서 지운다
pub fn pop_lsb(bb: &mut Bitboard) -> Square { 
    debug_assert!(*bb != 0);
    let sq = bb.trailing_zeros() as u8;
    *bb &= *bb -1;

    sq
}

/// "e4" → Some(28), 잘못된 입력 → None
pub fn parse_square(s: &str) -> Option<Square> {
        let b = s.as_bytes();
        if b.len() != 2 {return None}

        let file = b[0].wrapping_sub(b'a');
        let rank = b[1].wrapping_sub(b'1');

        if file < 8 && rank < 8{
            Some(square(file, rank))
        }else{
            None
        }
    }
/// 28 → "e4"
pub fn square_name(sq: Square) -> String {
    let file = (b'a' + file_of(sq)) as char;
    let rank = (b'1' + rank_of(sq)) as char;

    format!("{}{}", file, rank)
}

/// 'N' → Some((White, Knight)), 'p' → Some((Black, Pawn)), 'x' → None
pub fn piece_from_char(c: char) -> Option<(Color, Piece)> {
    let mut color = Color::Black;
    //find color
    let mut ch = c;
    if ch.is_ascii_uppercase() {
        color = Color::White;
        ch = c.to_ascii_lowercase();
    }

    //find piece
    let piece = match ch {
        'p' => Piece::Pawn,
        'n' => Piece::Knight,
        'b' => Piece::Bishop,
        'r' => Piece::Rook,
        'q' => Piece::Queen,
        'k' => Piece::King,
         _  => return None,
    };

    Some((color, piece))
}
/// (White, Knight) → 'N', (Black, Pawn) → 'p'
pub fn piece_to_char(color: Color, piece: Piece) -> char {
    let mut ch = match piece {
        Piece::Pawn => 'p',
        Piece::Knight => 'n',
        Piece::Bishop => 'b',
        Piece::Rook => 'r',
        Piece::Queen => 'q',
        Piece::King => 'k',
    };

    if color == Color::White {
        ch = ch.to_ascii_uppercase();
    }

    ch
}


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn squares() {
        assert_eq!(square(0, 0), 0);          // a1
        assert_eq!(square(7, 7), 63);         // h8
        assert_eq!(square(4, 3), 28);         // e4
        assert_eq!(file_of(28), 4);
        assert_eq!(rank_of(28), 3);
        assert_eq!(bit(0), 1);
        assert_eq!(bit(63), 1u64 << 63);
    }

    #[test]
    fn names() {
        assert_eq!(parse_square("e4"), Some(28));
        assert_eq!(parse_square("h8"), Some(63));
        assert_eq!(parse_square("i9"), None);
        assert_eq!(parse_square("e"), None);
        assert_eq!(square_name(28), "e4");
    }

    #[test]
    fn pop() {
        let mut bb: Bitboard = bit(3) | bit(10) | bit(63);
        assert_eq!(pop_lsb(&mut bb), 3);
        assert_eq!(pop_lsb(&mut bb), 10);
        assert_eq!(pop_lsb(&mut bb), 63);
        assert_eq!(bb, 0);
    }

    #[test]
    fn color() {
        assert_eq!(Color::White.flip(), Color::Black);
        assert_eq!(Color::Black.flip(), Color::White);
    }

    #[test]
    fn piece_from_char_all() {
        use Color::*;
        use Piece::*;
        // 대문자 = 백
        assert_eq!(piece_from_char('P'), Some((White, Pawn)));
        assert_eq!(piece_from_char('N'), Some((White, Knight)));
        assert_eq!(piece_from_char('B'), Some((White, Bishop)));
        assert_eq!(piece_from_char('R'), Some((White, Rook)));
        assert_eq!(piece_from_char('Q'), Some((White, Queen)));
        assert_eq!(piece_from_char('K'), Some((White, King)));
        // 소문자 = 흑
        assert_eq!(piece_from_char('p'), Some((Black, Pawn)));
        assert_eq!(piece_from_char('n'), Some((Black, Knight)));
        assert_eq!(piece_from_char('b'), Some((Black, Bishop)));
        assert_eq!(piece_from_char('r'), Some((Black, Rook)));
        assert_eq!(piece_from_char('q'), Some((Black, Queen)));
        assert_eq!(piece_from_char('k'), Some((Black, King)));
    }

    #[test]
    fn piece_from_char_invalid() {
        // FEN에서 기물이 아닌 글자들: 숫자, 구분자, 공백, 다른 알파벳
        for c in ['x', 'X', 'a', 'z', '1', '8', '/', '-', ' '] {
            assert_eq!(piece_from_char(c), None, "'{}'는 기물이 아니어야 함", c);
        }
    }

    #[test]
    fn piece_char_round_trip() {
        // 12가지 기물 모두: 기물 → 문자 → 기물로 되돌아와야 함
        let colors = [Color::White, Color::Black];
        let pieces = [Piece::Pawn, Piece::Knight, Piece::Bishop, Piece::Rook, Piece::Queen, Piece::King];
        for color in colors {
            for piece in pieces {
                let c = piece_to_char(color, piece);
                assert_eq!(piece_from_char(c), Some((color, piece)), "{:?} {:?} → '{}'", color, piece, c);
            }
        }
        // 대소문자 확인
        assert_eq!(piece_to_char(Color::White, Piece::Knight), 'N');
        assert_eq!(piece_to_char(Color::Black, Piece::Knight), 'n');
    }
}