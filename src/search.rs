use crate::board::Board;
use crate::eval::evaluate;
use crate::moves::{EP_CAPTURE, Move};
use crate::movegen::{generate_legal_moves, in_check};
use crate::types::Piece;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

pub const INF: i32 = 1_000_000;
pub const MATE: i32 = 100_000;

/// 탐색을 언제 멈출지 정하는 조건들
pub struct Limits {
    pub depth: u32,                     // 최대 깊이
    pub time: Option<Duration>,         // 이번 수에 쓸 시간 (None이면 제한 없음)
    pub stop: Option<Arc<AtomicBool>>,  // 바깥(UCI stop)에서 켜면 멈춤
}

impl Limits {
    /// 깊이만 정하고 시간·stop 제한은 없음
    pub fn depth(depth: u32) -> Limits {
        Limits { depth, time: None, stop: None }
    }
}

pub struct SearchResult {
    pub best: Option<Move>,   // 둘 수 없으면 None (메이트/스테일메이트)
    pub score: i32,           // 둘 차례인 쪽 기준
    pub nodes: u64,           // 방문한 국면 수
    pub time_ms: u64,         // 탐색 시작부터 걸린 시간
}

/// 탐색 중 계속 들고 다니는 상태
pub struct SearchState {
    pub nodes: u64,
    pub deadline: Option<Instant>,       // None이면 시간 제한 없음
    pub stop: Option<Arc<AtomicBool>>,   // 바깥에서 켜는 정지 신호
    pub stopped: bool,                   // 시간 초과나 stop으로 중단했는가
}

impl SearchState {
    pub fn new(deadline: Option<Instant>) -> SearchState {
        SearchState { nodes: 0, deadline, stop: None, stopped: false }
    }

    /// 2048 노드마다 시계와 정지 신호를 확인해서, 멈춰야 하면 stopped를 켠다
    fn check_time(&mut self) {
        if self.nodes & 2047 == 0 {
            let timed_out = self.deadline.map_or(false, |d| Instant::now() >= d);
            let stop_requested = self.stop.as_ref().map_or(false, |s| s.load(Ordering::Relaxed));
            if timed_out || stop_requested {
                self.stopped = true;
            }
        }
    }
}

/// 깊이 depth까지 탐색해서 최선의 수를 고른다 (시간 제한 없음)
pub fn search(board: &mut Board, depth: u32) -> SearchResult {
    search_root(board, depth, None, &mut SearchState::new(None))
}

/// 깊이 1부터 limits.depth까지 차례로 탐색한다 (반복 심화)
/// - 깊이 하나가 끝날 때마다 report(깊이, 결과)를 부른다. 노드 수는 누적값
/// - 시간이 다 되거나 stop이 켜지면 멈추고, 마지막으로 끝까지 마친 깊이의 결과를 돌려준다
pub fn iterative_deepening(
    board: &mut Board,
    limits: &Limits,
    mut report: impl FnMut(u32, &SearchResult),
) -> SearchResult {
    let start = Instant::now();
    let time_limit = limits.time;
    let deadline = time_limit.map(|t| start + t);
    let mut total_nodes = 0;
    let mut last = SearchResult { best: None, score: 0, nodes: 0, time_ms: 0 };

    for depth in 1..=limits.depth.max(1) {
        // 깊이 1은 시간·stop과 상관없이 끝까지 (둘 수가 하나는 있어야 하므로)
        let mut st = SearchState::new(None);
        if depth > 1 {
            // 이미 stop이 왔으면 다음 깊이는 시작하지 않음
            // (얕은 깊이는 2048 노드 전에 끝나서 탐색 중 확인에 걸리지 않을 수 있음)
            if limits.stop.as_ref().map_or(false, |s| s.load(Ordering::Relaxed)) {
                break;
            }
            st.deadline = deadline;
            st.stop = limits.stop.clone();
        }
        // 이전 깊이의 최선수를 가장 먼저 탐색 → α-β가 더 많이 잘라 냄
        let mut r = search_root(board, depth, last.best, &mut st);
        if st.stopped {
            break;   // 중간에 멈춘 깊이는 아직 못 본 수가 있어 믿을 수 없음 → 버림
        }
        total_nodes += r.nodes;
        r.nodes = total_nodes;
        r.time_ms = start.elapsed().as_millis() as u64;
        report(depth, &r);

        let mate_found = r.score.abs() >= MATE - 1000;
        last = r;
        if mate_found {
            break;   // 가장 빠른 메이트를 이미 찾음
        }
        // 이미 배정 시간의 절반을 썼다면 다음 깊이(보통 몇 배 더 걸림)는 시작하지 않음
        if let Some(t) = time_limit {
            if start.elapsed() >= t / 2 {
                break;
            }
        }
    }

    last
}

/// first가 있으면 그 수를 가장 먼저 탐색한다
fn search_root(board: &mut Board, depth: u32, first: Option<Move>, st: &mut SearchState) -> SearchResult {
    let mut list = generate_legal_moves(board);
    let mut best = None;
    let mut alpha = -INF;
    let beta = INF;
    if let Some(f) = first {
        if let Some(i) = list.iter().position(|&m| m == f) {
            list.swap(0, i);
        }
    }
    if list.is_empty() {
        let score = if in_check(board) { -MATE } else { 0 };
        return SearchResult { best, score, nodes: st.nodes, time_ms: 0 };
    };

    for mv in list {
        board.make_move(mv);
        let score = -negamax(board, depth - 1, 1, -beta, -alpha, st);
        board.unmake_move();
        if st.stopped {
            break;   // 멈춘 뒤의 점수는 의미 없음 (보드는 위에서 이미 되돌림)
        }

        if score > alpha {
            alpha = score;
            best = Some(mv);
        }
    }

    SearchResult { best, score: alpha, nodes: st.nodes, time_ms: 0 }
}

/// 네가맥스 + α-β. 둘 차례인 쪽 기준 점수를 돌려준다
fn negamax(board: &mut Board, depth: u32, ply: i32, mut alpha: i32, beta: i32, st: &mut SearchState) -> i32 {
    st.nodes += 1;
    st.check_time();
    if st.stopped {
        return 0;   // 이 점수는 어차피 버려짐
    }

    if depth == 0 {
        return quiescence(board, ply, alpha, beta, st)
    }

    let list = generate_legal_moves(board);
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
        let score = -negamax(board, depth -1, ply +1, -beta, -alpha, st);
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

/// 캡처 정렬용 기물 가치 (폰, 나이트, 비숍, 룩, 퀸, 킹)
const PIECE_VALUE: [i32; 6] = [100, 320, 330, 500, 900, 20000];

/// MVV-LVA 점수: 클수록 먼저 탐색
fn mvv_lva(board: &Board, mv: Move) -> i32 {
    debug_assert!(mv.is_capture(), "mvv_lva는 캡처에만 써야 함: {}", mv.to_uci());
    let captured = if mv.flag() == EP_CAPTURE {
        Piece::Pawn
    }else{
        board.mailbox[mv.to() as usize].unwrap()
    };
    let moving = board.mailbox[mv.from() as usize].unwrap();

    10*PIECE_VALUE[captured as usize] - PIECE_VALUE[moving as usize]
}

/// 정적 탐색: 캡처만 이어서 탐색하고, 조용해지면 평가
fn quiescence(board: &mut Board, ply: i32, mut alpha: i32, beta: i32, st: &mut SearchState) -> i32 {
    st.nodes += 1;
    st.check_time();
    if st.stopped {
        return 0;
    }

    let standard_pat = evaluate(board);

    if standard_pat >= beta {
        return standard_pat
    }

    if standard_pat >= alpha {
        alpha = standard_pat;
    }

    let mut best = standard_pat;

    let mut legal_mv = generate_legal_moves(board);
    legal_mv.retain(|&m| m.is_capture());
    legal_mv.sort_by_key(|&m| -mvv_lva(board, m) );

    for mv in legal_mv {
        board.make_move(mv);
        let score = -quiescence(board, ply+1, -beta, -alpha, st);
        board.unmake_move();

        if score >= best {
            best = score;
        }

        if score >= alpha {
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
        if depth == 0 { return quiescence(board, ply, -INF, INF, &mut SearchState::new(None)); }
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
    #[ignore]
    fn alphabeta_equals_minimax() {
        // α-β는 미니맥스와 '같은 점수'를 더 적은 노드로 내야 함
        for fen in [
            "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1",
            "r3k2r/p1ppqpb1/bn2pnp1/3PN3/1p2P3/2N2Q1p/PPPBBPPP/R3K2R w KQkq - 0 1",
            "8/2p5/3p4/KP5r/1R3p1k/8/4P1P1/8 w - - 0 1",
            "r3k2r/Pppp1ppp/1b3nbN/nP6/BBP1P3/q4N2/Pp1P2PP/R2Q1RK1 w kq - 0 1",
        ] {
            let mut b = Board::from_fen(fen).unwrap();
            let expected = minimax(&mut b, 2, 0);
            let r = search(&mut b, 2);
            assert_eq!(r.score, expected, "{}", fen);
        }
    }

    #[test]
    fn quiet_position_equals_eval() {
        // 잡을 게 없는 국면이면 정적 탐색 = 평가 그대로
        let mut b = Board::from_fen("rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1").unwrap();
        assert_eq!(quiescence(&mut b, 0, -INF, INF, &mut SearchState::new(None)), evaluate(&b));
    }

    #[test]
    fn quiescence_sees_recapture() {
        // b7 폰을 잡으면 b8 룩이 되잡음 → 정적 탐색은 그걸 보고 "안 잡는다(stand pat)"를 고름
        let mut b = Board::from_fen("1r2k3/1p6/8/8/8/8/8/1Q2K3 w - - 0 1").unwrap();
        let q = quiescence(&mut b, 0, -INF, INF, &mut SearchState::new(None));
        assert_eq!(q, evaluate(&b), "되잡히는 캡처를 하지 않아야 함");
    }

    #[test]
    fn quiescence_takes_free_piece() {
        // 공짜 퀸은 잡아야 함 → 평가보다 훨씬 높아야 함
        let mut b = Board::from_fen("4k3/8/8/3q4/4P3/8/8/4K3 w - - 0 1").unwrap();
        let q = quiescence(&mut b, 0, -INF, INF, &mut SearchState::new(None));
        assert!(q > evaluate(&b) + 800, "q={} eval={}", q, evaluate(&b));
    }

    #[test]
    fn depth_one_avoids_poisoned_capture() {
        // 8-1에서는 깊이 2가 필요했던 테스트: 이제 깊이 1로도 피해야 함
        let r = run("1r2k3/1p6/8/8/8/8/8/1Q2K3 w - - 0 1", 1);
        assert_ne!(r.best.unwrap().to_uci(), "b1b7");
    }

    #[test]
    fn deepening_reports_every_depth() {
        let mut b = Board::from_fen("rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1").unwrap();
        let mut depths = Vec::new();
        iterative_deepening(&mut b, &Limits::depth(4), |d, _| depths.push(d));
        assert_eq!(depths, vec![1, 2, 3, 4]);
    }

    #[test]
    fn deepening_same_score_as_fixed_depth() {
        // 먼저 볼 수를 바꿔도 α-β 결과(점수)는 같아야 함
        for fen in [
            "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1",
            "r3k2r/p1ppqpb1/bn2pnp1/3PN3/1p2P3/2N2Q1p/PPPBBPPP/R3K2R w KQkq - 0 1",
        ] {
            let mut b = Board::from_fen(fen).unwrap();
            // 깊이 2: Kiwipete 깊이 3은 디버그 빌드에서 수십 초가 걸림 (수 정렬이 생기면 다시 늘릴 것)
            let fixed = search(&mut b, 2).score;
            let deep = iterative_deepening(&mut b, &Limits::depth(2), |_, _| {}).score;
            assert_eq!(fixed, deep, "{}", fen);
            assert_eq!(b.to_fen(), fen);
        }
    }

    #[test]
    fn deepening_stops_at_mate() {
        // 1수 메이트: 깊이 2에서 메이트를 찾고 더 깊이 가지 않아야 함
        let mut b = Board::from_fen("6k1/5ppp/8/8/8/8/8/R5K1 w - - 0 1").unwrap();
        let mut depths = Vec::new();
        let r = iterative_deepening(&mut b, &Limits::depth(8), |d, _| depths.push(d));
        assert_eq!(r.best.unwrap().to_uci(), "a1a8");
        assert_eq!(r.score, MATE - 1);
        assert_eq!(depths, vec![1, 2]);
    }

    #[test]
    fn deepening_nodes_are_cumulative() {
        let mut b = Board::from_fen("rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1").unwrap();
        let mut nodes = Vec::new();
        iterative_deepening(&mut b, &Limits::depth(3), |_, r| nodes.push(r.nodes));
        assert!(nodes.windows(2).all(|w| w[0] < w[1]), "{:?}", nodes);
    }

    #[test]
    fn first_move_is_searched_first() {
        // first로 준 수를 맨 앞에 두어도 결과 점수는 같아야 함
        let mut b = Board::from_fen("rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1").unwrap();
        let normal = search(&mut b, 3).score;
        let h2h3 = generate_legal_moves(&mut b).into_iter().find(|m| m.to_uci() == "h2h3");
        assert_eq!(search_root(&mut b, 3, h2h3, &mut SearchState::new(None)).score, normal);
    }

    #[test]
    fn deadline_in_past_still_returns_move() {
        // 시간이 전혀 없어도 깊이 1은 끝까지 해서 수를 내야 함
        let mut b = Board::from_fen("r3k2r/p1ppqpb1/bn2pnp1/3PN3/1p2P3/2N2Q1p/PPPBBPPP/R3K2R w KQkq - 0 1").unwrap();
        let fen = b.to_fen();
        let mut depths = Vec::new();
        let r = iterative_deepening(&mut b, &Limits { depth: 64, time: Some(Duration::from_millis(0)), stop: None }, |d, _| depths.push(d));
        assert!(r.best.is_some());
        assert_eq!(depths, vec![1]);
        assert_eq!(b.to_fen(), fen, "중단 후에도 보드가 원래대로여야 함");
    }

    #[test]
    fn stopped_search_restores_board() {
        // 깊은 탐색을 도중에 끊어도 보드와 히스토리가 원래대로여야 함
        let fen = "r3k2r/p1ppqpb1/bn2pnp1/3PN3/1p2P3/2N2Q1p/PPPBBPPP/R3K2R w KQkq - 0 1";
        let mut b = Board::from_fen(fen).unwrap();
        let start = Instant::now();
        iterative_deepening(&mut b, &Limits { depth: 64, time: Some(Duration::from_millis(100)), stop: None }, |_, _| {});
        assert!(start.elapsed() < Duration::from_secs(3), "{:?}", start.elapsed());
        assert_eq!(b.to_fen(), fen);
        assert!(b.history.is_empty());
    }

    #[test]
    fn stop_flag_stops_search() {
        // stop이 이미 켜져 있으면 깊이 1만 끝내고 멈춰야 함
        let mut b = Board::from_fen("rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1").unwrap();
        let stop = Arc::new(AtomicBool::new(true));
        let mut depths = Vec::new();
        let r = iterative_deepening(&mut b, &Limits { depth: 64, time: None, stop: Some(stop) }, |d, _| depths.push(d));
        assert_eq!(depths, vec![1]);
        assert!(r.best.is_some());
    }

    #[test]
    fn mvv_lva_order() {
        // 폰으로 퀸 잡기(e4xd5) > 퀸으로 폰 잡기(b3xb7)
        let mut b = Board::from_fen("4k3/1p6/8/3q4/4P3/1Q6/8/4K3 w - - 0 1").unwrap();
        let moves = generate_legal_moves(&mut b);
        let pxq = *moves.iter().find(|m| m.to_uci() == "e4d5").unwrap();
        let qxp = *moves.iter().find(|m| m.to_uci() == "b3b7").unwrap();
        assert!(mvv_lva(&b, pxq) > mvv_lva(&b, qxp));
    }
}