use crate::board::*;
use crate::moves::*;
use crate::types::*;
use crate::zobrist::KEYS;

/// 칸별 캐슬링 마스크: 이 칸에서 출발하거나 이 칸에 도착하면 남는 권리
const CASTLE_MASK: [u8; 64] = {
    let mut m = [0b1111u8; 64];
    m[0] = !WQ & 0b1111;            // a1
    m[4] = !(WK | WQ) & 0b1111;     // e1
    m[7] = !WK & 0b1111;            // h1
    m[56] = !BQ & 0b1111;           // a8
    m[60] = !(BK | BQ) & 0b1111;    // e8
    m[63] = !BK & 0b1111;           // h8

    m
};

/// 앙파상으로 잡히는 폰의 칸 (us = 수를 둔 쪽)
fn ep_victim(us: Color, to: Square) -> Square {
    match us {
        Color::White => to - 8,
        Color::Black => to + 8,
    }
}

impl Board {
    pub fn make_move(&mut self, mv: Move) {
        let (us, them) = (self.side, self.side.flip());
        let piece = self.mailbox[mv.from() as usize].unwrap();

        let captured_sq = if mv.flag() == EP_CAPTURE { ep_victim(us, mv.to()) } else { mv.to() };
        let captured = self.mailbox[captured_sq as usize];

        let undo = Undo { mv: mv, captured: captured, 
            castling: self.castling, ep: self.ep, 
            halfmove: self.halfmove, fullmove: self.fullmove,
            hash: self.hash, 
        };
        self.history.push(undo);
        
        if let Some(captured_piece) = captured {
            self.remove_piece(them, captured_piece, captured_sq);
        }

        self.move_piece(us, piece, mv.from(), mv.to());
        
        if let Some(promo_piece) = mv.promotion_piece() {
            self.remove_piece(us, Piece::Pawn, mv.to());
            self.put_piece(us, promo_piece, mv.to());
        }

        if mv.flag() == KING_CASTLE || mv.flag() == QUEEN_CASTLE {
            let rank = match us {
                Color::Black => 7, Color::White => 0,
            };
            let [a, d, f, h] = [0, 3, 5, 7].map(|file| square(file, rank));
            match mv.flag() {
                KING_CASTLE => self.move_piece(us, Piece::Rook, h, f),
                QUEEN_CASTLE => self.move_piece(us, Piece::Rook, a, d),
                _ => unreachable!(),
            }
            

        }

        //기존 해시 끄기
        self.hash ^= KEYS.castling[self.castling as usize];
        if let Some(ep) = self.ep {
            self.hash ^= KEYS.ep_file[file_of(ep) as usize];
        }

        //상태 갱신
        self.ep = if mv.flag() == DOUBLE_PUSH {Some((mv.from()+mv.to())/2)}else{None};
        if let Some(ep) = self.ep {
            self.hash ^= KEYS.ep_file[file_of(ep) as usize];
        }

        self.castling &= CASTLE_MASK[mv.from() as usize] & CASTLE_MASK[mv.to() as usize];
        self.hash ^= KEYS.castling[self.castling as usize];

        self.halfmove = if Piece::Pawn == piece || mv.is_capture() {
            0
        }else {
            self.halfmove +1
        };
        if us == Color::Black {self.fullmove += 1;}

        self.side = them;
        self.hash ^= KEYS.side;
    }

    pub fn unmake_move(&mut self) {
        let undo = self.history.pop().unwrap();
        self.side = self.side.flip();
        let us = self.side;

        self.castling = undo.castling;
        self.ep = undo.ep;
        self.halfmove = undo.halfmove;
        self.fullmove = undo.fullmove;

        if undo.mv.is_promotion() {
            self.remove_piece(us, self.mailbox[undo.mv.to() as usize].unwrap(), undo.mv.to());
            self.put_piece(us, Piece::Pawn, undo.mv.from());
        }else{
            self.move_piece(us, self.mailbox[undo.mv.to() as usize].unwrap(), undo.mv.to(), undo.mv.from());
            if undo.mv.flag() == KING_CASTLE {
                let rank = match us {Color::Black => 7, Color::White => 0};
                self.move_piece(us, Piece::Rook, square(5, rank), square(7, rank));
            }else if undo.mv.flag() == QUEEN_CASTLE {
                let rank = match us {Color::Black => 7, Color::White => 0};
                self.move_piece(us, Piece::Rook, square(3, rank), square(0, rank));
            }
        }

        if let Some(captured) = undo.captured {
            let captured_sq = if undo.mv.flag() == EP_CAPTURE { ep_victim(us, undo.mv.to()) } else { undo.mv.to() };
            self.put_piece(us.flip(), captured, captured_sq);
        }

        self.hash = undo.hash;
    }

    pub fn make_null_move(&mut self) {
        let undo = Undo{mv: Move::new(0, 0, QUIET), captured: None,
        castling: self.castling, ep: self.ep, 
        halfmove: self.halfmove, fullmove: self.fullmove,
        hash: self.hash};
        self.history.push(undo);

        if let Some(ep_sq) = self.ep {
            self.hash ^= KEYS.ep_file[file_of(ep_sq) as usize];
        }
        self.ep = None;
        self.halfmove += 1;
        self.side = self.side.flip();
        self.hash ^= KEYS.side
    }
    pub fn unmake_null_move(&mut self) {
        let undo = self.history.pop().unwrap();
        self.side = self.side.flip();

        self.castling = undo.castling;
        self.ep = undo.ep;
        self.halfmove = undo.halfmove;
        self.fullmove = undo.fullmove;
        self.hash = undo.hash;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::movegen::generate_moves;

    /// UCI 수를 찾아서 둔다
    fn play(b: &mut Board, uci: &str) {
        let mut list = Vec::new();
        generate_moves(b, &mut list);
        let mv = *list.iter().find(|m| m.to_uci() == uci).expect("생성되지 않은 수");
        b.make_move(mv);
    }

    fn after(fen: &str, uci: &str) -> String {
        let mut b = Board::from_fen(fen).unwrap();
        play(&mut b, uci);
        b.to_fen()
    }

    /// 비트보드와 메일박스가 서로 같은 내용인지
    fn assert_consistent(b: &Board) {
        for sq in 0..64u8 {
            let from_bits = (0..2).flat_map(|c| (0..6).map(move |p| (c, p)))
                .find(|&(c, p)| b.pieces[c][p] & bit(sq) != 0);
            assert_eq!(from_bits.map(|(_, p)| p), b.mailbox[sq as usize].map(|p| p as usize), "칸 {}", square_name(sq));
        }
        let w = b.pieces[0].iter().fold(0, |a, x| a | x);
        let bl = b.pieces[1].iter().fold(0, |a, x| a | x);
        assert_eq!((w, bl), (b.occupied[0], b.occupied[1]));
    }

    #[test]
    fn simple_moves() {
        let start = "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1";
        assert_eq!(after(start, "e2e4"), "rnbqkbnr/pppppppp/8/8/4P3/8/PPPP1PPP/RNBQKBNR b KQkq e3 0 1");
        assert_eq!(after(start, "g1f3"), "rnbqkbnr/pppppppp/8/8/8/5N2/PPPPPPPP/RNBQKB1R b KQkq - 1 1");
        // 흑이 두면 전체 수 증가
        assert_eq!(after("rnbqkbnr/pppppppp/8/8/4P3/8/PPPP1PPP/RNBQKBNR b KQkq e3 0 1", "e7e5"),
                   "rnbqkbnr/pppp1ppp/8/4p3/4P3/8/PPPP1PPP/RNBQKBNR w KQkq e6 0 2");
    }

    #[test]
    fn castling() {
        let fen = "r3k2r/8/8/8/8/8/8/R3K2R w KQkq - 0 1";
        assert_eq!(after(fen, "e1g1"), "r3k2r/8/8/8/8/8/8/R4RK1 b kq - 1 1");
        assert_eq!(after(fen, "e1c1"), "r3k2r/8/8/8/8/8/8/2KR3R b kq - 1 1");
        let fen = "r3k2r/8/8/8/8/8/8/R3K2R b KQkq - 0 1";
        assert_eq!(after(fen, "e8g8"), "r4rk1/8/8/8/8/8/8/R3K2R w KQ - 1 2");
        assert_eq!(after(fen, "e8c8"), "2kr3r/8/8/8/8/8/8/R3K2R w KQ - 1 2");
    }

    #[test]
    fn castling_rights() {
        let fen = "r3k2r/8/8/8/8/8/8/R3K2R w KQkq - 0 1";
        // 킹이 움직이면 그쪽 권리 둘 다 잃음
        assert_eq!(after(fen, "e1e2"), "r3k2r/8/8/8/8/8/4K3/R6R b kq - 1 1");
        // a1 룩이 움직이면 Q만 잃음
        assert_eq!(after(fen, "a1b1"), "r3k2r/8/8/8/8/8/8/1R2K2R b Kkq - 1 1");
        // h1 룩이 h8 룩을 잡음 → 백은 K, 흑은 k를 잃음
        assert_eq!(after(fen, "h1h8"), "r3k2R/8/8/8/8/8/8/R3K3 b Qq - 0 1");
    }

    #[test]
    fn special_moves() {
        // 앙파상: d5 폰이 사라져야 함
        assert_eq!(after("4k3/8/8/3pP3/8/8/8/4K3 w - d6 0 1", "e5d6"), "4k3/8/3P4/8/8/8/8/4K3 b - - 0 1");
        // 승격
        assert_eq!(after("4k3/P7/8/8/8/8/8/4K3 w - - 0 1", "a7a8q"), "Q3k3/8/8/8/8/8/8/4K3 b - - 0 1");
        assert_eq!(after("4k3/P7/8/8/8/8/8/4K3 w - - 0 1", "a7a8n"), "N3k3/8/8/8/8/8/8/4K3 b - - 0 1");
        // 잡으면서 승격
        assert_eq!(after("1n2k3/P7/8/8/8/8/8/4K3 w - - 0 1", "a7b8q"), "1Q2k3/8/8/8/8/8/8/4K3 b - - 0 1");
    }

    #[test]
    fn make_unmake_restores_everything() {
        // 이 국면들에서 생성되는 '모든' 수를 두고 되돌렸을 때 완전히 원래대로 돌아와야 함
        for fen in [
            "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1",
            "r3k2r/p1ppqpb1/bn2pnp1/3PN3/1p2P3/2N2Q1p/PPPBBPPP/R3K2R w KQkq - 0 1",
            "r3k2r/Pppp1ppp/1b3nbN/nP6/BBP1P3/q4N2/Pp1P2PP/R2Q1RK1 w kq - 0 1",
            "rnbq1k1r/pp1Pbppp/2p5/8/2B5/8/PPP1NnPP/RNBQK2R w KQ - 1 8",
            "4k3/8/8/3pP3/8/8/8/4K3 w - d6 0 1",
            "r3k2r/8/8/8/8/8/8/R3K2R b KQkq - 0 1",
        ] {
            let mut b = Board::from_fen(fen).unwrap();
            let mut list = Vec::new();
            generate_moves(&b, &mut list);
            for mv in list {
                b.make_move(mv);
                assert_consistent(&b);
                b.unmake_move();
                assert_eq!(b.to_fen(), fen, "{} 되돌리기 실패", mv.to_uci());
                assert_eq!(b.pieces, Board::from_fen(fen).unwrap().pieces, "{} 비트보드 불일치", mv.to_uci());
                assert_consistent(&b);
                assert!(b.history.is_empty());
            }
        }
    }
        #[test]
    fn null_move_round_trip() {
        // 앙파상 칸이 있는 국면: 널 무브 후 ep가 지워지고, 되돌리면 원래대로
        let fen = "rnbqkbnr/ppp1pppp/8/3pP3/8/8/PPPP1PPP/RNBQKBNR w KQkq d6 0 3";
        let mut b = Board::from_fen(fen).unwrap();
        let hash = b.hash;

        b.make_null_move();
        assert_eq!(b.side, Color::Black);
        assert_eq!(b.ep, None);
        assert_eq!(b.hash, b.compute_hash(), "널 무브 후 증분 해시가 틀림");

        b.unmake_null_move();
        assert_eq!(b.to_fen(), fen);
        assert_eq!(b.hash, hash);
    }
}