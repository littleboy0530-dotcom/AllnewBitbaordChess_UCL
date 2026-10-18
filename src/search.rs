use crate::board::Board;
use crate::eval::evaluate;
use crate::moves::Move;
use crate::movegen::{generate_legal_moves, in_check};

pub const INF: i32 = 1_000_000;
pub const MATE: i32 = 100_000;

pub struct SearchResult {
    pub best: Option<Move>,   // 둘 수 없으면 None (메이트/스테일메이트)
    pub score: i32,           // 둘 차례인 쪽 기준
    pub nodes: u64,           // 방문한 국면 수
}

/// 깊이 depth까지 탐색해서 최선의 수를 고른다
pub fn search(board: &mut Board, depth: u32) -> SearchResult {
    let mut list = generate_legal_moves(board);
    let mut nodes = 0;
    let mut best = None;
    let (mut alpha ,mut beta) = (-INF, INF);
    if list.is_empty() {
        if in_check(board) {
            return SearchResult { best, score: -MATE, nodes }
        }else{
            return SearchResult { best, score: 0, nodes }
        }
    };

    for mv in list {
        board.make_move(mv);
        let score = -negamax(board, depth - 1, 1, -beta, -alpha, &mut nodes);
        board.unmake_move();

        if score > alpha {
            alpha = score;
            best = Some(mv);
        }
    }

    SearchResult { best, score: alpha, nodes }
}

/// 네가맥스 + α-β. 둘 차례인 쪽 기준 점수를 돌려준다
fn negamax(board: &mut Board, depth: u32, ply: i32, mut alpha: i32, beta: i32, nodes: &mut u64) -> i32 {
    *nodes += 1;

    if depth == 0 {
        return evaluate(board)
    }

    let mut list = generate_legal_moves(board);
    if list.is_empty() {
        if in_check(board) {
            return -MATE + ply
        }else{
            return 0
        }
    }

    let mut best = -INF;

    for mv in list {
        board.make_move(mv);
        let score = -negamax(board, depth -1, ply +1, -beta, -alpha, nodes);
        board.unmake_move();

        if score > best {
            best = score;
        }
        if score > alpha {
            alpha = score;
        }
        if alpha >= beta {
            break;
        }
    }

    best
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(fen: &str, depth: u32) -> SearchResult {
        let mut b = Board::from_fen(fen).unwrap();
        let r = search(&mut b, depth);
        assert_eq!(b.to_fen(), fen, "탐색 후 보드가 원래대로 돌아오지 않음");
        r
    }

    #[test]
    fn mate_in_one() {
        // 백 룩 a8 백랭크 메이트
        // 깊이 2가 필요: 내 수(1) + 상대에게 둘 수가 없는지 확인(2)
        let r = run("6k1/5ppp/8/8/8/8/8/R5K1 w - - 0 1", 2);
        assert_eq!(r.best.unwrap().to_uci(), "a1a8");
        assert_eq!(r.score, MATE - 1);
        // 흑도 같은 메이트
        let r = run("r5k1/8/8/8/8/8/5PPP/6K1 b - - 0 1", 2);
        assert_eq!(r.best.unwrap().to_uci(), "a8a1");
        assert_eq!(r.score, MATE - 1);
    }

    #[test]
    fn mate_in_two() {
        // 킹과 룩으로 2수 메이트: 백-흑-백(3 ply) + 메이트 확인(1) = 깊이 4가 필요
        let r = run("k7/8/2K5/8/8/8/8/7R w - - 0 1", 4);
        assert_eq!(r.score, MATE - 3);
    }

    #[test]
    fn already_mated_or_stalemate() {
        // 흑이 이미 체크메이트당함: 둘 수 없음
        let r = run("k7/1Q6/1K6/8/8/8/8/8 b - - 0 1", 2);
        assert!(r.best.is_none());
        assert_eq!(r.score, -MATE);
        // 흑 스테일메이트
        let r = run("k7/8/1Q6/8/8/8/8/7K b - - 0 1", 2);
        assert!(r.best.is_none());
        assert_eq!(r.score, 0);
    }

    #[test]
    fn grabs_free_queen() {
        // e4 폰으로 공짜 퀸을 잡아야 함
        let r = run("4k3/8/8/3q4/4P3/8/8/4K3 w - - 0 1", 2);
        assert_eq!(r.best.unwrap().to_uci(), "e4d5");
    }

    #[test]
    fn avoids_poisoned_capture() {
        // b7 폰을 퀸으로 잡으면 b8 룩에게 퀸을 잃음 → 깊이 2면 피해야 함
        let r = run("1r2k3/1p6/8/8/8/8/8/1Q2K3 w - - 0 1", 2);
        assert_ne!(r.best.unwrap().to_uci(), "b1b7");
    }

    /// 가지치기 없는 미니맥스 (비교용 정답지)
    fn minimax(board: &mut Board, depth: u32, ply: i32) -> i32 {
        if depth == 0 { return evaluate(board); }
        let moves = generate_legal_moves(board);
        if moves.is_empty() {
            return if in_check(board) { -MATE + ply } else { 0 };
        }
        let mut best = -INF;
        for mv in moves {
            board.make_move(mv);
            best = best.max(-minimax(board, depth - 1, ply + 1));
            board.unmake_move();
        }
        best
    }

    #[test]
    fn alphabeta_equals_minimax() {
        // α-β는 미니맥스와 '같은 점수'를 더 적은 노드로 내야 함
        for fen in [
            "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1",
            "r3k2r/p1ppqpb1/bn2pnp1/3PN3/1p2P3/2N2Q1p/PPPBBPPP/R3K2R w KQkq - 0 1",
            "8/2p5/3p4/KP5r/1R3p1k/8/4P1P1/8 w - - 0 1",
            "r3k2r/Pppp1ppp/1b3nbN/nP6/BBP1P3/q4N2/Pp1P2PP/R2Q1RK1 w kq - 0 1",
        ] {
            let mut b = Board::from_fen(fen).unwrap();
            let expected = minimax(&mut b, 3, 0);
            let r = search(&mut b, 3);
            assert_eq!(r.score, expected, "{}", fen);
        }
    }
}