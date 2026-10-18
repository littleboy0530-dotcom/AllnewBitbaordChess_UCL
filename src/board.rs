use crate::types::*;
use crate::moves::Move;

// 캐슬링 권리 비트
pub const WK: u8 = 1; // 백 킹사이드 (K)
pub const WQ: u8 = 2; // 백 퀸사이드 (Q)
pub const BK: u8 = 4; // 흑 킹사이드 (k)
pub const BQ: u8 = 8; // 흑 퀸사이드 (q)

#[derive(Clone)]
pub struct Board {
    pub pieces: [[Bitboard; 6]; 2],      // [색][기물]
    pub occupied: [Bitboard; 2],         // [색]
    pub mailbox: [Option<Piece>; 64],
    pub side: Color,
    pub castling: u8,
    pub ep: Option<Square>,
    pub halfmove: u8,
    pub fullmove: u16,
    pub history: Vec<Undo>,
}

impl Board {
    /// 아무것도 없는 보드 (백 차례, 전체 수 1)
    pub fn empty() -> Board { 
        Board{pieces: [[0;6];2], occupied: [0;2], mailbox: [None; 64],
        side: Color::White, castling: 0, ep: None, halfmove: 0, fullmove: 1, history:Vec::new()}
    }

    /// 기물 놓기: pieces, occupied, mailbox를 한 번에 갱신
    pub fn put_piece(&mut self, color: Color, piece: Piece, sq: Square) {
        self.pieces[color as usize][piece as usize] |= bit(sq);
        self.occupied[color as usize] |= bit(sq);
        self.mailbox[sq as usize] = Some(piece);
    }

    /// 그 칸의 (색, 기물). 빈칸이면 None
    pub fn piece_at(&self, sq: Square) -> Option<(Color, Piece)> {
        if let Some(piece) = self.mailbox[sq as usize] {
            if self.pieces[Color::White as usize][piece as usize] & bit(sq) != 0 {
                return Some((Color::White, piece))
            }else{
                return Some((Color::Black, piece))
            }
        }
        
        None
    }

    pub fn from_fen(fen: &str) -> Result<Board, String> {
        let parts:Vec<&str> = fen.split_whitespace().collect();
        if parts.len() < 4 {return Err(format!("잘못된 FEN: {}", fen))}//길이 검사

        let mut b = Board::empty();

        //기물 배치
        let mut rank:i8 = 7;
        let mut file = 0;

        for c in parts[0].chars() {
            match c {
                '1'..='8' => file += c as u8 - b'0',
                '/' => {
                    if file == 8 && rank > 0 {
                        rank-=1;
                        file = 0;
                    }else{
                        return Err(format!("잘못된 FEN: {}", fen))
                    }
                },
                _ => {
                    let (color, piece) = piece_from_char(c).ok_or_else(|| format!("알 수 없는 기물: {}", c))?;
                    if file >= 8 {return Err(format!("잘못된 FEN: {}", fen))}
                    b.put_piece(color, piece, square(file, rank as u8));
                    file += 1;
                }
            }
        }
        if file != 8 || rank != 0 {
            return Err(format!("잘못된 FEN: {}", fen))
        }

        //차례
        b.side = match parts[1] {
            "w" => Color::White,
            "b" => Color::Black,
            _ => return Err(format!("잘못된 시작 색깔 정보 {}", parts[1]))
        };

        //캐슬링 권한
        for c in parts[2].chars() {
            match c {
                '-' => break,
                'K' => b.castling |= WK,
                'Q' => b.castling |= WQ,
                'k' => b.castling |= BK,
                'q' => b.castling |= BQ,
                _ => return Err(format!("잘못된 캐슬링 권한: {}", parts[2]))
            }
        }

        //앙파상 위치
        if parts[3] != "-" {
            match parse_square(parts[3]) {
                Some(sq) => b.ep = Some(sq),
                None => return Err(format!("앙파상 위치 오류 {}", parts[3])),
            }
        }

        //수 카운터
        b.halfmove = parts.get(4).and_then(|&s| s.parse().ok()).unwrap_or(0);
        b.fullmove = parts.get(5).and_then(|&s| s.parse().ok()).unwrap_or(1);

        Ok(b)
    }
    pub fn to_fen(&self) -> String {
        //포지션
        let mut placement = String::new();

        for rank in (0..8).rev() {
            let mut empty:u8 = 0;
            for file in 0..8 {
                match self.piece_at(square(file, rank)) {
                    Some((color, piece)) => {
                        if empty > 0 {
                            placement.push((b'0' + empty) as char);
                            empty = 0;
                        }
                        placement.push(piece_to_char(color, piece));
                    },
                    None => empty += 1,
                }
            }
            if empty > 0 {placement.push((b'0' + empty) as char);}
            if rank > 0 {placement.push('/');}
        }

        //시작 색
        let side = match self.side {
            Color::Black => "b",
            Color::White => "w",
        };

        //캐슬링
        let mut castle = String::new();
        for (flag, ch) in [(WK, 'K'), (WQ, 'Q'), (BK, 'k'), (BQ, 'q')] {
            if self.castling & flag != 0 {
                castle.push(ch);
            }
        }
        if castle.is_empty() {castle.push_str("-");}

        //앙파상
        let ep = match self.ep {
            Some(ep_pos) => square_name(ep_pos),
            None => "-".to_string(),
        };


        format!("{} {} {} {} {} {}", placement, side, castle, ep, self.halfmove, self.fullmove)
    }

    /// 옛 엔진의 `d`와 같은 모양으로 출력
    pub fn print(&self) {
        print!("{}", self.to_display_string());
    }

    pub fn to_display_string(&self) -> String {
        let mut s = String::new();
        for rank in (0..8).rev() {
            s.push((b'1' + rank) as char);
            for file in 0..8 {
                s.push(' ');
                if let Some((c, p)) = self.piece_at(square(file, rank)){
                    s.push(piece_to_char(c, p));
                }else{
                    s.push('.');

                }
            }
            s.push('\n');
        }

        s.push_str(" a b c d e f g h\n");

        s
    }

    /// 기물 빼기: put_piece의 반대
    pub fn remove_piece(&mut self, color: Color, piece: Piece, sq: Square) {
        self.pieces[color as usize][piece as usize] &= !bit(sq);
        self.occupied[color as usize] &= !bit(sq);
        self.mailbox[sq as usize] = None;
    }

    /// from의 기물을 to로 옮기기 (to는 비어 있어야 함)
    pub fn move_piece(&mut self, color: Color, piece: Piece, from: Square, to: Square) {
        self.remove_piece(color, piece, from);
        self.put_piece(color, piece, to);
    }

    /// 전체 점유 칸 (백 | 흑)
    pub fn all(&self) -> Bitboard { self.occupied[0] | self.occupied[1] }
}

#[derive(Clone, Copy, Debug)]
pub struct Undo {
    pub mv: Move,
    pub captured: Option<Piece>,
    pub castling: u8,
    pub ep: Option<Square>,
    pub halfmove: u8,
    pub fullmove: u16,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_board() {
        let b = Board::empty();
        assert_eq!(b.occupied, [0, 0]);
        assert_eq!(b.side, Color::White);
        assert_eq!(b.fullmove, 1);
        for sq in 0..64 {
            assert_eq!(b.piece_at(sq), None);
        }
    }

    #[test]
    fn put_and_read() {
        let mut b = Board::empty();
        let e1 = parse_square("e1").unwrap();
        let d8 = parse_square("d8").unwrap();
        b.put_piece(Color::White, Piece::King, e1);
        b.put_piece(Color::Black, Piece::Queen, d8);

        // 메일박스 쪽 확인
        assert_eq!(b.piece_at(e1), Some((Color::White, Piece::King)));
        assert_eq!(b.piece_at(d8), Some((Color::Black, Piece::Queen)));
        assert_eq!(b.piece_at(parse_square("e4").unwrap()), None);

        // 비트보드 쪽 확인: 두 곳이 같은 내용인지
        assert_eq!(b.pieces[Color::White as usize][Piece::King as usize], bit(e1));
        assert_eq!(b.pieces[Color::Black as usize][Piece::Queen as usize], bit(d8));
        assert_eq!(b.occupied[Color::White as usize], bit(e1));
        assert_eq!(b.occupied[Color::Black as usize], bit(d8));
    }

    #[test]
    fn same_piece_many_squares() {
        // 같은 종류를 여러 칸에 놓으면 비트가 쌓여야 함
        let mut b = Board::empty();
        for file in 0..8 {
            b.put_piece(Color::White, Piece::Pawn, square(file, 1));
        }
        assert_eq!(b.pieces[Color::White as usize][Piece::Pawn as usize], 0xFF00); // 2랭크 전체
        assert_eq!(b.occupied[Color::White as usize].count_ones(), 8);
    }

    #[test]
    fn display_empty() {
        let b = Board::empty();
        let expected = "\
8 . . . . . . . .
7 . . . . . . . .
6 . . . . . . . .
5 . . . . . . . .
4 . . . . . . . .
3 . . . . . . . .
2 . . . . . . . .
1 . . . . . . . .
 a b c d e f g h
";
        assert_eq!(b.to_display_string(), expected);
    }

    #[test]
    fn display_pieces() {
        let mut b = Board::empty();
        b.put_piece(Color::White, Piece::King, parse_square("e1").unwrap());
        b.put_piece(Color::White, Piece::Pawn, parse_square("e4").unwrap());
        b.put_piece(Color::Black, Piece::Knight, parse_square("b8").unwrap());
        b.put_piece(Color::Black, Piece::King, parse_square("h8").unwrap());
        let expected = "\
8 . n . . . . . k
7 . . . . . . . .
6 . . . . . . . .
5 . . . . . . . .
4 . . . . P . . .
3 . . . . . . . .
2 . . . . . . . .
1 . . . . K . . .
 a b c d e f g h
";
        assert_eq!(b.to_display_string(), expected);
    }

        const START: &str = "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1";

    #[test]
    fn fen_start_position() {
        let b = Board::from_fen(START).unwrap();
        assert_eq!(b.piece_at(parse_square("e1").unwrap()), Some((Color::White, Piece::King)));
        assert_eq!(b.piece_at(parse_square("d8").unwrap()), Some((Color::Black, Piece::Queen)));
        assert_eq!(b.piece_at(parse_square("e4").unwrap()), None);
        assert_eq!(b.side, Color::White);
        assert_eq!(b.castling, WK | WQ | BK | BQ);
        assert_eq!(b.ep, None);
        assert_eq!(b.halfmove, 0);
        assert_eq!(b.fullmove, 1);
        assert_eq!(b.occupied[0].count_ones(), 16);
        assert_eq!(b.occupied[1].count_ones(), 16);
        assert_eq!(b.pieces[Color::White as usize][Piece::Pawn as usize], 0xFF00);
    }

    #[test]
    fn fen_state_fields() {
        // 흑 차례, 앙파상 e3, 일부 캐슬링, 카운터
        let b = Board::from_fen("rnbqkbnr/pppppppp/8/8/4P3/8/PPPP1PPP/RNBQKBNR b Kq e3 5 12").unwrap();
        assert_eq!(b.side, Color::Black);
        assert_eq!(b.castling, WK | BQ);
        assert_eq!(b.ep, parse_square("e3"));
        assert_eq!(b.halfmove, 5);
        assert_eq!(b.fullmove, 12);
        assert_eq!(b.piece_at(parse_square("e4").unwrap()), Some((Color::White, Piece::Pawn)));
        assert_eq!(b.piece_at(parse_square("e2").unwrap()), None);
    }

    #[test]
    fn fen_short_form() {
        // 카운터 생략 → 기본값 0, 1
        let b = Board::from_fen("8/8/8/8/8/8/8/K6k w - -").unwrap();
        assert_eq!(b.castling, 0);
        assert_eq!(b.halfmove, 0);
        assert_eq!(b.fullmove, 1);
        assert_eq!(b.piece_at(0), Some((Color::White, Piece::King)));   // a1
        assert_eq!(b.piece_at(7), Some((Color::Black, Piece::King)));   // h1
    }

    #[test]
    fn fen_kiwipete_display() {
        let b = Board::from_fen("r3k2r/p1ppqpb1/bn2pnp1/3PN3/1p2P3/2N2Q1p/PPPBBPPP/R3K2R w KQkq - 0 1").unwrap();
        let expected = "\
8 r . . . k . . r
7 p . p p q p b .
6 b n . . p n p .
5 . . . P N . . .
4 . p . . P . . .
3 . . N . . Q . p
2 P P P B B P P P
1 R . . . K . . R
 a b c d e f g h
";
        assert_eq!(b.to_display_string(), expected);
    }

    #[test]
    fn fen_errors() {
        assert!(Board::from_fen("").is_err());                                                      // 빈 문자열
        assert!(Board::from_fen("8/8/8/8 w").is_err());                                             // 칸 부족
        assert!(Board::from_fen("rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR x KQkq - 0 1").is_err()); // 차례 오류
        assert!(Board::from_fen("rnbqkxnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1").is_err()); // 잘못된 기물
        assert!(Board::from_fen("rnbqkbnr/pppppppp/9/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1").is_err());  // 한 랭크 9칸
        assert!(Board::from_fen("rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR/8 w KQkq - 0 1").is_err()); // 랭크 9개
        assert!(Board::from_fen("rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq z9 0 1").is_err()); // 앙파상 오류
    }

        #[test]
    fn fen_round_trip() {
        for fen in [
            START,
            "rnbqkbnr/pppppppp/8/8/4P3/8/PPPP1PPP/RNBQKBNR b KQkq e3 0 1",
            "r3k2r/p1ppqpb1/bn2pnp1/3PN3/1p2P3/2N2Q1p/PPPBBPPP/R3K2R w KQkq - 0 1",
            "8/2p5/3p4/KP5r/1R3p1k/8/4P1P1/8 w - - 0 1",
            "r3k2r/Pppp1ppp/1b3nbN/nP6/BBP1P3/q4N2/Pp1P2PP/R2Q1RK1 w kq - 0 1",
            "rnbq1k1r/pp1Pbppp/2p5/8/2B5/8/PPP1NnPP/RNBQK2R w KQ - 1 8",
            "8/8/8/8/8/8/8/8 w - - 0 1",
            "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR b Kq - 5 12",
        ] {
            assert_eq!(Board::from_fen(fen).unwrap().to_fen(), fen);
        }
    }

    #[test]
    fn to_fen_fills_defaults() {
        // 카운터가 생략된 FEN을 읽으면 기본값이 채워져서 나와야 함
        let b = Board::from_fen("8/8/8/8/8/8/8/K6k w - -").unwrap();
        assert_eq!(b.to_fen(), "8/8/8/8/8/8/8/K6k w - - 0 1");
    }
}