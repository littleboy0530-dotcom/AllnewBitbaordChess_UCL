// src/movegen.rs
use crate::attacks::*;
use crate::board::*;
use crate::moves::*;
use crate::types::*;

/// `by` 색의 기물이 `sq`를 공격하는가
pub fn is_attacked(board: &Board, sq: Square, by: Color) -> bool {
    let piece_attacks = 
    [(Piece::Pawn, pawn_attacks(by.flip(), sq)), 
    (Piece::Knight, knight_attacks(sq)), 
    (Piece::Bishop, bishop_attacks(sq, board.all())), 
    (Piece::Rook, rook_attacks(sq, board.all())), 
    (Piece::Queen, queen_attacks(sq, board.all())), 
    (Piece::King, king_attacks(sq))];
    for (piece_type, attacks) in piece_attacks {
        if attacks & board.pieces[by as usize][piece_type as usize] != 0 {
            return true
        }
    }
    false
}
/// make_move 직후에 부른다: 방금 둔 쪽의 킹이 공격받지 않으면 true
pub fn last_move_was_legal(board: &Board) -> bool {
    let mover = board.side.flip();   // make 후에는 차례가 넘어가 있으니, 방금 둔 쪽은 반대편
    let king_sq = board.pieces[mover as usize][Piece::King as usize].trailing_zeros() as Square;
    !is_attacked(board, king_sq, board.side)
}
/// 둘 차례인 쪽의 유사 합법수를 list에 모두 추가
pub fn generate_moves(board: &Board, list: &mut Vec<Move>) {
    generate_pawn_moves(board, list);
    generate_piece_moves(board, list);   // 나이트, 비숍, 룩, 퀸, 킹
    generate_castling(board, list);
}

/// targets의 각 칸으로 가는 수 추가: 상대 기물이 있으면 CAPTURE, 없으면 QUIET
fn add_moves(list: &mut Vec<Move>, from: Square, mut targets: Bitboard, enemy: Bitboard) {
    while targets != 0 {
        let to = pop_lsb(&mut targets);
        let flag = if enemy & bit(to) != 0 { CAPTURE } else { QUIET };
        list.push(Move::new(from, to, flag));
    }
}

fn generate_pawn_moves(board: &Board, list: &mut Vec<Move>) {
    let us = board.side;
    let enemy = board.occupied[us.flip() as usize];
    let empty = !board.all();
    let (up, start_rank, promo_rank): (i8, u8, u8) = match us {
        Color::White => (8, 1, 7),
        Color::Black => (-8, 6, 0),
    };

    let mut pawns = board.pieces[us as usize][Piece::Pawn as usize];
    while pawns != 0 {
        let from = pop_lsb(&mut pawns);

        // 1. 전진
        let one = (from as i8 + up) as Square;
        if empty & bit(one) != 0 {
            if rank_of(one) == promo_rank {
                for flag in PROMO_N..=PROMO_Q {
                    list.push(Move::new(from, one, flag));
                }
            } else {
                list.push(Move::new(from, one, QUIET));
                // 두 칸 전진: 한 칸 앞이 비었을 때만 검사
                let two = (one as i8 + up) as Square;
                if rank_of(from) == start_rank && empty & bit(two) != 0 {
                    list.push(Move::new(from, two, DOUBLE_PUSH));
                }
            }
        }

        // 2. 캡처
        let attacks = pawn_attacks(us, from);
        let mut captures = attacks & enemy;
        while captures != 0 {
            let to = pop_lsb(&mut captures);
            if rank_of(to) == promo_rank {
                for flag in PROMO_CAPTURE_N..=PROMO_CAPTURE_Q {
                    list.push(Move::new(from, to, flag));
                }
            } else {
                list.push(Move::new(from, to, CAPTURE));
            }
        }

        // 3. 앙파상
        if let Some(ep) = board.ep {
            if attacks & bit(ep) != 0 {
                list.push(Move::new(from, ep, EP_CAPTURE));
            }
        }
    }
}
fn generate_piece_moves(board: &Board, list: &mut Vec<Move>) {
    let us = board.side as usize;
    let own = board.occupied[us];
    let enemy = board.occupied[board.side.flip() as usize];
    let occ = board.all();
    for piece in [Piece::Knight, Piece::Bishop, Piece::Rook, Piece::Queen, Piece::King] {
        let mut pieces = board.pieces[us][piece as usize];
        while pieces != 0 {
            let from = pop_lsb(&mut pieces);
            let attacks = match piece {
                Piece::Knight => knight_attacks(from),
                Piece::Bishop => bishop_attacks(from, occ),
                Piece::Rook => rook_attacks(from, occ),
                Piece::Queen => queen_attacks(from, occ),
                Piece::King => king_attacks(from),
                Piece::Pawn => unreachable!(),
            };
            add_moves(list, from, attacks & !own, enemy);
        }
    }
}
fn generate_castling(board: &Board, list: &mut Vec<Move>) {
    let us = board.side;
    let them = us.flip();
    let (k_right, q_right, rank) = match us {
        Color::White => (WK, WQ, 0),
        Color::Black => (BK, BQ, 7),
    };
    let [a, b, c, d, e, f, g, h] = [0, 1, 2, 3, 4, 5, 6, 7].map(|file| square(file, rank));
    let rooks = board.pieces[us as usize][Piece::Rook as usize];
    let occ = board.all();
    let safe = |sq| !is_attacked(board, sq, them);

    if board.castling & k_right != 0
        && rooks & bit(h) != 0
        && occ & (bit(f) | bit(g)) == 0
        && safe(e) && safe(f) && safe(g)
    {
        list.push(Move::new(e, g, KING_CASTLE));
    }
    if board.castling & q_right != 0
        && rooks & bit(a) != 0
        && occ & (bit(b) | bit(c) | bit(d)) == 0
        && safe(e) && safe(d) && safe(c)
    {
        list.push(Move::new(e, c, QUEEN_CASTLE));
    }
}
/// 합법수만 만든다
pub fn generate_legal_moves(board: &mut Board) -> Vec<Move> {
    let mut list = Vec::with_capacity(20); 
    generate_moves(board, &mut list);

    list.retain(|mv| {
        
        board.make_move(*mv);
        let legal = last_move_was_legal(board);
        board.unmake_move();

        legal
    });

    list
}

/// 둘 차례인 쪽의 킹이 체크 중인가
pub fn in_check(board: &Board) -> bool {
    let king_sq = board.pieces[board.side as usize][Piece::King as usize].trailing_zeros() as u8;
    
    is_attacked(board, king_sq, board.side.flip())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sq(name: &str) -> Square {
        parse_square(name).unwrap()
    }
    fn _gen(fen: &str) -> Vec<Move> {
        let b = Board::from_fen(fen).unwrap();
        let mut list = Vec::new();
        generate_moves(&b, &mut list);
        list
    }
    /// "e2e4" 같은 UCI 수가 목록에 있는지
    fn has(list: &[Move], uci: &str) -> bool {
        list.iter().any(|m| m.to_uci() == uci)
    }

    #[test]
    fn attacked_squares() {
        let b = Board::from_fen("rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1").unwrap();
        assert!(is_attacked(&b, sq("f3"), Color::White));   // g2 폰, g1 나이트
        assert!(!is_attacked(&b, sq("e4"), Color::White));
        assert!(is_attacked(&b, sq("f6"), Color::Black));
        assert!(!is_attacked(&b, sq("f3"), Color::Black));

        let b = Board::from_fen("4k3/8/8/8/8/8/8/R3K3 w - - 0 1").unwrap();
        assert!(is_attacked(&b, sq("a8"), Color::White));   // 룩 직선
        assert!(!is_attacked(&b, sq("b8"), Color::White));

        // 폰 방향: 흰 폰 e4는 d5/f5를 공격, d3/f3은 공격하지 않음
        let b = Board::from_fen("4k3/8/8/8/4P3/8/8/4K3 w - - 0 1").unwrap();
        assert!(is_attacked(&b, sq("d5"), Color::White));
        assert!(!is_attacked(&b, sq("d3"), Color::White));
    }

    #[test]
    fn start_position() {
        assert_eq!(_gen("rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1").len(), 20);
        assert_eq!(_gen("rnbqkbnr/pppppppp/8/8/4P3/8/PPPP1PPP/RNBQKBNR b KQkq e3 0 1").len(), 20);
    }

    #[test]
    fn rooks_king_castling() {
        // 킹 5 + 캐슬링 2 + a1 룩 10 + h1 룩 9
        let list = _gen("4k3/8/8/8/8/8/8/R3K2R w KQ - 0 1");
        assert_eq!(list.len(), 26);
        assert!(has(&list, "e1g1"));
        assert!(has(&list, "e1c1"));
    }

    #[test]
    fn castling_rules() {
        // f1이 흑 룩(f2)에게 공격받음 → 킹사이드 불가, 퀸사이드 가능
        let list = _gen("4k3/8/8/8/8/8/5r2/R3K2R w KQ - 0 1");
        assert!(!has(&list, "e1g1"));
        assert!(has(&list, "e1c1"));
        // b1만 공격받는 건 괜찮음
        let list = _gen("4k3/8/8/8/8/8/1r6/R3K2R w KQ - 0 1");
        assert!(has(&list, "e1c1"));
        // 체크 중이면 둘 다 불가
        let list = _gen("4k3/8/8/8/8/8/4r3/R3K2R w KQ - 0 1");
        assert!(!has(&list, "e1g1") && !has(&list, "e1c1"));
        // 권리는 있지만 h1에 룩이 없음 → 불가
        let list = _gen("4k3/8/8/8/8/8/8/R3K3 w KQ - 0 1");
        assert!(!has(&list, "e1g1"));
        // 흑 캐슬링
        let list = _gen("r3k2r/8/8/8/8/8/8/4K3 b kq - 0 1");
        assert!(has(&list, "e8g8") && has(&list, "e8c8"));
    }

    #[test]
    fn pawn_pushes() {
        // e2 앞이 막힘 → 폰 수 없음, 킹 4
        assert_eq!(_gen("4k3/8/8/8/8/4p3/4P3/4K3 w - - 0 1").len(), 4);
        // e4가 막힘 → 한 칸만 + 킹 4
        let list = _gen("4k3/8/8/8/4p3/8/4P3/4K3 w - - 0 1");
        assert_eq!(list.len(), 5);
        assert!(has(&list, "e2e3") && !has(&list, "e2e4"));
    }

    #[test]
    fn promotions() {
        // a8 승격 4 + 킹 3
        let list = _gen("8/P7/8/8/8/8/8/k6K w - - 0 1");
        assert_eq!(list.len(), 7);
        for uci in ["a7a8q", "a7a8r", "a7a8b", "a7a8n"] {
            assert!(has(&list, uci));
        }
        // 전진 승격 4 + b8 잡으며 승격 4 + 킹 3
        let list = _gen("1n6/P7/8/8/8/8/8/k6K w - - 0 1");
        assert_eq!(list.len(), 11);
        assert!(list.iter().any(|m| m.to_uci() == "a7b8q" && m.is_capture()));
        // 흑 승격
        assert!(has(&_gen("4k3/8/8/8/8/8/p7/7K b - - 0 1"), "a2a1q"));
    }

    #[test]
    fn en_passant() {
        // e5 폰: 전진 1 + 앙파상 1 + 킹 5
        let list = _gen("4k3/8/8/3pP3/8/8/8/4K3 w - d6 0 1");
        assert_eq!(list.len(), 7);
        assert!(list.iter().any(|m| m.to_uci() == "e5d6" && m.flag() == EP_CAPTURE));
        // ep 칸이 없으면 앙파상 없음
        assert!(!has(&_gen("4k3/8/8/3pP3/8/8/8/4K3 w - - 0 1"), "e5d6"));
    }

    #[test]
    fn castling_blocked_path() {
        // f1에 비숍 → 킹사이드 불가, 퀸사이드 가능
        let list = _gen("4k3/8/8/8/8/8/8/R3KB1R w KQ - 0 1");
        assert!(!has(&list, "e1g1"));
        assert!(has(&list, "e1c1"));
        // g1에 나이트 → 킹사이드 불가
        assert!(!has(&_gen("4k3/8/8/8/8/8/8/R3K1NR w KQ - 0 1"), "e1g1"));
        // b1에 나이트 → 퀸사이드 불가 (b1은 공격 검사는 안 하지만 비어 있어야 함)
        let list = _gen("4k3/8/8/8/8/8/8/RN2K2R w KQ - 0 1");
        assert!(!has(&list, "e1c1"));
        assert!(has(&list, "e1g1"));
        // 상대 기물이 막아도 불가
        assert!(!has(&_gen("4k3/8/8/8/8/8/8/R2nK2R w KQ - 0 1"), "e1c1"));
        // 흑도 같은 규칙
        let list = _gen("r3k1nr/8/8/8/8/8/8/4K3 b kq - 0 1");
        assert!(!has(&list, "e8g8"));
        assert!(has(&list, "e8c8"));
    }
}