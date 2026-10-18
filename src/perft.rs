use crate::board::Board;
use crate::movegen::generate_legal_moves;

/// 깊이 depth까지의 모든 수순 개수
/// 역할: 수 생성기가 제대로 작동하는지 확인
pub fn perft(board: &mut Board, depth: u32) -> u64 {
    if depth == 0 {return 1}
    let moves = generate_legal_moves(board);
    if depth == 1 {
        return moves.len() as u64
    }
    let mut count = 0;
    for mv in moves {
        board.make_move(mv);
        count += perft(board, depth -1);
        board.unmake_move();
    }

    count
}

/// 첫 수별 개수를 출력하고 합계를 돌려준다 (디버깅용)
pub fn divide(board: &mut Board, depth: u32) -> u64 {
    let mut total = 0;
    let moves = generate_legal_moves(board);

    for mv in moves {
        board.make_move(mv);
        let n = perft(board, depth -1);
        board.unmake_move();

        println!("{}: {}", mv.to_uci(), n);
        total += n;
    }

    println!("합계 :{}", total);

    total
}

#[cfg(test)]
mod tests {
    use super::*;

    fn check(fen: &str, expected: &[u64]) {
        let mut b = Board::from_fen(fen).unwrap();
        for (i, &n) in expected.iter().enumerate() {
            let depth = i as u32 + 1;
            assert_eq!(perft(&mut b, depth), n, "깊이 {} 불일치: {}", depth, fen);
        }
        assert_eq!(b.to_fen(), Board::from_fen(fen).unwrap().to_fen(), "perft 후 보드가 원래대로 돌아오지 않음");
    }

    // 디버그 빌드에서도 몇 초 안에 끝나는 깊이
    #[test]
    fn start_position() {
        check("rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1", &[20, 400, 8902, 197281]);
    }
    #[test]
    fn kiwipete() {
        // 캐슬링, 앙파상, 승격, 핀이 모두 들어 있는 국면
        check("r3k2r/p1ppqpb1/bn2pnp1/3PN3/1p2P3/2N2Q1p/PPPBBPPP/R3K2R w KQkq - 0 1", &[48, 2039, 97862]);
    }
    #[test]
    fn position3() {
        // 앙파상 핀 등 엔드게임 함정
        check("8/2p5/3p4/KP5r/1R3p1k/8/4P1P1/8 w - - 0 1", &[14, 191, 2812, 43238]);
    }
    #[test]
    fn position4() {
        // 승격과 체크가 많은 국면
        check("r3k2r/Pppp1ppp/1b3nbN/nP6/BBP1P3/q4N2/Pp1P2PP/R2Q1RK1 w kq - 0 1", &[6, 264, 9467]);
    }
    #[test]
    fn position5() {
        check("rnbq1k1r/pp1Pbppp/2p5/8/2B5/8/PPP1NnPP/RNBQK2R w KQ - 1 8", &[44, 1486, 62379]);
    }

    // 깊은 검증: cargo test --release -- --ignored
    #[test]
    #[ignore]
    fn deep() {
        let cases = [
            ("rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1", 5, 4_865_609),
            ("r3k2r/p1ppqpb1/bn2pnp1/3PN3/1p2P3/2N2Q1p/PPPBBPPP/R3K2R w KQkq - 0 1", 4, 4_085_603),
            ("8/2p5/3p4/KP5r/1R3p1k/8/4P1P1/8 w - - 0 1", 5, 674_624),
            ("r3k2r/Pppp1ppp/1b3nbN/nP6/BBP1P3/q4N2/Pp1P2PP/R2Q1RK1 w kq - 0 1", 4, 422_333),
            ("rnbq1k1r/pp1Pbppp/2p5/8/2B5/8/PPP1NnPP/RNBQK2R w KQ - 1 8", 4, 2_103_487),
        ];
        for (fen, depth, expected) in cases {
            let mut b = Board::from_fen(fen).unwrap();
            assert_eq!(perft(&mut b, depth), expected, "{}", fen);
        }
    }
}