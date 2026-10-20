use crate::board::Board;
use crate::eval::evaluate;
use crate::moves::{EP_CAPTURE, Move};
use crate::movegen::{generate_legal_moves, generate_moves, in_check, last_move_was_legal};
use crate::types::Piece;
use crate::tt::{Bound, TranspositionTable, score_from_tt, score_to_tt};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

pub const INF: i32 = 1_000_000;
pub const MATE: i32 = 100_000;

const MAX_PLY: usize = 128;

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
    pub killers: [[Option<Move>; 2]; MAX_PLY],//킬러 무브(킬러 휴리스틱용)
}

impl SearchState {
    pub fn new(deadline: Option<Instant>) -> SearchState {
        SearchState { nodes: 0, deadline, stop: None, stopped: false , killers: [[None;2];MAX_PLY]}
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
    // 한 번 쓰고 마는 탐색이라 작은 표를 새로 만들어 쓴다
    let mut tt = TranspositionTable::new(16);
    search_root(board, depth, None, &mut SearchState::new(None), &mut tt)
}

/// 깊이 1부터 limits.depth까지 차례로 탐색한다 (반복 심화)
/// - 깊이 하나가 끝날 때마다 report(깊이, 결과)를 부른다. 노드 수는 누적값
/// - 시간이 다 되거나 stop이 켜지면 멈추고, 마지막으로 끝까지 마친 깊이의 결과를 돌려준다
pub fn iterative_deepening(
    board: &mut Board,
    limits: &Limits,
    tt: &mut TranspositionTable,
    mut report: impl FnMut(u32, &SearchResult),
) -> SearchResult {
    let start = Instant::now();
    let time_limit = limits.time;
    let deadline = time_limit.map(|t| start + t);
    let mut total_nodes = 0;
    let mut last = SearchResult { best: None, score: 0, nodes: 0, time_ms: 0 };
    let mut st = SearchState::new(None);

    for depth in 1..=limits.depth.max(1) {
        // 깊이 1은 시간·stop과 상관없이 끝까지 (둘 수가 하나는 있어야 하므로)
        st.nodes = 0;
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
        let mut r = search_root(board, depth, last.best, &mut st, tt);
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
fn search_root(board: &mut Board, depth: u32, first: Option<Move>, st: &mut SearchState, tt: &mut TranspositionTable) -> SearchResult {
    let mut list = generate_legal_moves(board);
    let mut best = None;
    let mut alpha = -INF;
    let beta = INF;
    order_moves(board, &mut list, first, [None;2]);
    if list.is_empty() {
        let score = if in_check(board) { -MATE } else { 0 };
        return SearchResult { best, score, nodes: st.nodes, time_ms: 0 };
    };

    for mv in list {
        board.make_move(mv);
        let score = -negamax(board, depth - 1, 1, -beta, -alpha, st, tt, true);
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
fn negamax(board: &mut Board, 
    depth: u32, ply: i32, 
    mut alpha: i32, beta: i32, 
    st: &mut SearchState, 
    tt: &mut TranspositionTable,
    allow_null: bool,
    ) -> i32 {
    st.nodes += 1;
    st.check_time();
    
    if st.stopped {
        return 0;   // 이 점수는 어차피 버려짐
    }

    if depth == 0 {
        return quiescence(board, ply, alpha, beta, st)
    }

    let mut tt_move = None;
    if let Some(entry) = tt.probe(board.hash) {
        tt_move = entry.best;   // 깊이가 부족해도 수는 정렬에 쓸 수 있음
        if entry.depth >= depth {
            let score = score_from_tt(entry.score, ply);
            match entry.bound {
                Bound::Exact => return score,
                Bound::Lower => if score >= beta {return score},
                Bound::Upper => if score <= alpha {return score},
            }
        }
    }

    const NULL_R: u32 = 2;

    let in_check_now = in_check(board);   // 아래 메이트 판정에서도 재사용 가능

    if allow_null && !in_check_now && depth >= 3 && has_non_pawn_material(board) {
        board.make_null_move();
        let score = -negamax(board, depth - 1 - NULL_R, ply + 1, -beta, -beta + 1, st, tt, false);
        board.unmake_null_move();

        if st.stopped { return 0; }
        if score >= beta {
            return beta// β 컷
        }
    }


    // 의사 합법 수를 정렬하고, 합법성은 둘 때 확인 (β 컷 뒤의 수는 검사하지 않음)
    let mut list = Vec::with_capacity(64);
    generate_moves(board, &mut list);
    order_moves(board, &mut list, tt_move, st.killers[ply as usize]);

    let alpha_origin = alpha;
    let mut best = -INF;
    let mut best_move = None;
    let mut legal_count = 0;

    for mv in list {
        board.make_move(mv);
        if !last_move_was_legal(board) {
            board.unmake_move();
            continue;
        }
        legal_count += 1;
        let score = -negamax(board, depth -1, ply +1, -beta, -alpha, st, tt, true);
        board.unmake_move();

        if score > best {
            best = score;
            best_move = Some(mv);
        }
        if score > alpha {
            alpha = score;
        }
        if alpha >= beta {
            if !mv.is_capture() && st.killers[ply as usize][0] != Some(mv) {
                st.killers[ply as usize][1] = st.killers[ply as usize][0];
                st.killers[ply as usize][0] = Some(mv);
            }
            break;
        }
    }

    // 둘 수 있는 수가 하나도 없었음: 메이트 또는 스테일메이트 (TT 저장 전에 판정)
    if legal_count == 0 {
        return if in_check_now { -MATE + ply } else { 0 };
    }

    if st.stopped {return 0}

    let bound = if best <= alpha_origin {
        Bound::Upper
    }else if best >= beta {
        Bound::Lower
    }else {
        Bound::Exact
    };

    tt.store(board.hash, depth, score_to_tt(best, ply), bound, best_move);

    best
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

    // 의사 합법 수에서 캡처만 남긴 뒤, 합법성은 둘 때 확인 (조용한 수는 검사도 안 함)
    let mut captures = Vec::with_capacity(64);
    generate_moves(board, &mut captures);
    captures.retain(|&m| m.is_capture());
    order_moves(board, &mut captures, None, [None;2]);

    for mv in captures {
        board.make_move(mv);
        if !last_move_was_legal(board) {
            board.unmake_move();
            continue;
        }
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

const TT_MOVE_SCORE: i32 = 1_000_000;
const CAPTURE_SCORE: i32 = 100_000;
const QUEEN_PROMO_SCORE: i32 = 90_000;
const KILLER_1_SCORE: i32 = 80_000;
const KILLER_2_SCORE: i32 = 70_000;

/// 정렬 점수: 클수록 먼저 탐색
fn score_move(board: &Board, mv: Move, tt_move: Option<Move>, killers: [Option<Move>;2]) -> i32 {
    if tt_move == Some(mv) {
        TT_MOVE_SCORE
    }else if mv.is_capture() {
        CAPTURE_SCORE + mvv_lva(board, mv)
    }else if mv.promotion_piece() == Some(Piece::Queen) {
        QUEEN_PROMO_SCORE
    }else if killers[0] == Some(mv) {
        KILLER_1_SCORE
    }else if killers[1] == Some(mv){
        KILLER_2_SCORE
    }else{
        0
    }
}

/// 정렬 점수가 높은 순으로 정렬
fn order_moves(board: &Board, moves: &mut Vec<Move>, tt_move: Option<Move>, killers: [Option<Move>;2]) {
    moves.sort_by_cached_key(|&mv| -score_move(board, mv, tt_move, killers));
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

/// 둘 차례인 쪽에 나이트·비숍·룩·퀸이 하나라도 있는가
fn has_non_pawn_material(board: &Board) -> bool {
    for p in 1..=4 {
        if board.pieces[board.side as usize][p] != 0 {
            return true
        }
    }

    false
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
        iterative_deepening(&mut b, &Limits::depth(4), &mut TranspositionTable::new(16), |d, _| depths.push(d));
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
            let deep = iterative_deepening(&mut b, &Limits::depth(2), &mut TranspositionTable::new(16), |_, _| {}).score;
            assert_eq!(fixed, deep, "{}", fen);
            assert_eq!(b.to_fen(), fen);
        }
    }

    #[test]
    fn deepening_stops_at_mate() {
        // 1수 메이트: 깊이 2에서 메이트를 찾고 더 깊이 가지 않아야 함
        let mut b = Board::from_fen("6k1/5ppp/8/8/8/8/8/R5K1 w - - 0 1").unwrap();
        let mut depths = Vec::new();
        let r = iterative_deepening(&mut b, &Limits::depth(8), &mut TranspositionTable::new(16), |d, _| depths.push(d));
        assert_eq!(r.best.unwrap().to_uci(), "a1a8");
        assert_eq!(r.score, MATE - 1);
        assert_eq!(depths, vec![1, 2]);
    }

    #[test]
    fn deepening_nodes_are_cumulative() {
        let mut b = Board::from_fen("rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1").unwrap();
        let mut nodes = Vec::new();
        iterative_deepening(&mut b, &Limits::depth(3), &mut TranspositionTable::new(16), |_, r| nodes.push(r.nodes));
        assert!(nodes.windows(2).all(|w| w[0] < w[1]), "{:?}", nodes);
    }

    #[test]
    fn first_move_is_searched_first() {
        // first로 준 수를 맨 앞에 두어도 결과 점수는 같아야 함
        let mut b = Board::from_fen("rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1").unwrap();
        let normal = search(&mut b, 3).score;
        let h2h3 = generate_legal_moves(&mut b).into_iter().find(|m| m.to_uci() == "h2h3");
        assert_eq!(search_root(&mut b, 3, h2h3, &mut SearchState::new(None), &mut TranspositionTable::new(16)).score, normal);
    }

    #[test]
    fn deadline_in_past_still_returns_move() {
        // 시간이 전혀 없어도 깊이 1은 끝까지 해서 수를 내야 함
        let mut b = Board::from_fen("r3k2r/p1ppqpb1/bn2pnp1/3PN3/1p2P3/2N2Q1p/PPPBBPPP/R3K2R w KQkq - 0 1").unwrap();
        let fen = b.to_fen();
        let mut depths = Vec::new();
        let r = iterative_deepening(&mut b, &Limits { depth: 64, time: Some(Duration::from_millis(0)), stop: None }, &mut TranspositionTable::new(16), |d, _| depths.push(d));
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
        iterative_deepening(&mut b, &Limits { depth: 64, time: Some(Duration::from_millis(100)), stop: None }, &mut TranspositionTable::new(16), |_, _| {});
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
        let r = iterative_deepening(&mut b, &Limits { depth: 64, time: None, stop: Some(stop) }, &mut TranspositionTable::new(16), |d, _| depths.push(d));
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

    #[test]
    fn tt_makes_second_search_cheaper() {
        // 같은 국면을 같은 TT로 다시 탐색하면 노드가 크게 줄어야 함
        let mut b = Board::from_fen("r3k2r/p1ppqpb1/bn2pnp1/3PN3/1p2P3/2N2Q1p/PPPBBPPP/R3K2R w KQkq - 0 1").unwrap();
        let mut tt = TranspositionTable::new(18);
        let first = iterative_deepening(&mut b, &Limits::depth(3), &mut tt, |_, _| {});
        let second = iterative_deepening(&mut b, &Limits::depth(3), &mut tt, |_, _| {});
        assert!(second.nodes * 2 < first.nodes, "첫 탐색 {} / 두 번째 {}", first.nodes, second.nodes);
    }

    #[test]
    fn mate_distance_stays_correct_across_moves() {
        // 같은 TT를 쓰며 수를 진행해도 메이트까지 남은 수가 정확히 줄어야 함
        let mut b = Board::from_fen("k7/8/2K5/8/8/8/8/7R w - - 0 1").unwrap();
        let mut tt = TranspositionTable::new(18);

        let r1 = iterative_deepening(&mut b, &Limits::depth(4), &mut tt, |_, _| {});
        assert_eq!(r1.score, MATE - 3);                  // 백: 2수 메이트
        b.make_move(r1.best.unwrap());

        let r2 = iterative_deepening(&mut b, &Limits::depth(3), &mut tt, |_, _| {});
        assert_eq!(r2.score, -MATE + 2);                 // 흑: 1수 뒤 메이트당함
        b.make_move(r2.best.unwrap());

        let r3 = iterative_deepening(&mut b, &Limits::depth(2), &mut tt, |_, _| {});
        assert_eq!(r3.score, MATE - 1);                  // 백: 1수 메이트
    }

        #[test]
    fn move_order_groups() {
        // 백: e4xd5(폰이 퀸 잡기), b3xb7(퀸이 폰 잡기), a7a8q(퀸 승격), 그 외 조용한 수
        let mut b = Board::from_fen("4k3/Pp6/8/3q4/4P3/1Q6/8/4K3 w - - 0 1").unwrap();
        let mut moves = generate_legal_moves(&mut b);
        let find = |moves: &Vec<Move>, uci: &str| *moves.iter().find(|m| m.to_uci() == uci).unwrap();
        let quiet = find(&moves, "e1f2");

        order_moves(&b, &mut moves, Some(quiet), [None;2]);
        let pos = |uci: &str| moves.iter().position(|m| m.to_uci() == uci).unwrap();

        assert_eq!(pos("e1f2"), 0, "TT 수가 맨 앞");
        assert!(pos("e4d5") < pos("b3b7"), "폰으로 퀸 잡기 > 퀸으로 폰 잡기");
        assert!(pos("e4d5") < pos("a7a8q"), "퀸 잡기 > 퀸 승격");
        assert!(pos("a7a8q") < pos("e1f1"), "퀸 승격 > 조용한 수");
        assert!(pos("b3b7") < pos("e1f1"), "캡처 > 조용한 수");
    }

    #[test]
    fn worst_capture_still_before_quiet() {
        // 킹으로 폰 잡기(MVV-LVA 최저점)도 조용한 수보다 먼저
        let mut b = Board::from_fen("4k3/8/8/8/8/8/4p3/4K3 w - - 0 1").unwrap();
        let mut moves = generate_legal_moves(&mut b);
        order_moves(&b, &mut moves, None, [None;2]);
        assert_eq!(moves[0].to_uci(), "e1e2");
    }

    #[test]
    fn killer_scores_between_promotion_and_quiet() {
        let mut b = Board::from_fen("4k3/Pp6/8/3q4/4P3/1Q6/8/4K3 w - - 0 1").unwrap();
        let mut moves = generate_legal_moves(&mut b);
        let find = |uci: &str| *moves.iter().find(|m| m.to_uci() == uci).unwrap();
        let (k1, k2) = (find("e1f2"), find("e1f1"));

        order_moves(&b, &mut moves, None, [Some(k1), Some(k2)]);
        let pos = |uci: &str| moves.iter().position(|m| m.to_uci() == uci).unwrap();

        assert!(pos("a7a8q") < pos("e1f2"), "퀸 승격 > 킬러 1");
        assert!(pos("e1f2") < pos("e1f1"), "킬러 1 > 킬러 2");
        assert!(pos("e1f1") < pos("b3a4"), "킬러 2 > 일반 조용한 수");
    }

    #[test]
    fn killers_are_quiet_and_distinct() {
        let mut b = Board::from_fen("rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1").unwrap();
        let mut st = SearchState::new(None);
        let mut tt = TranspositionTable::new(16);
        for d in 1..=4 {
            search_root(&mut b, d, None, &mut st, &mut tt);
        }
        let mut found = 0;
        for k in st.killers.iter() {
            if let Some(m) = k[0] {
                found += 1;
                assert!(!m.is_capture(), "캡처가 킬러로 저장됨: {}", m.to_uci());
                assert_ne!(k[0], k[1], "두 칸이 같은 수");
            }
        }
        assert!(found > 0, "킬러가 하나도 저장되지 않음");
    }

    #[test]
    fn non_pawn_material_detection() {
        assert!(has_non_pawn_material(&Board::from_fen("4k3/8/8/8/8/8/4P3/4KN2 w - - 0 1").unwrap()));
        assert!(!has_non_pawn_material(&Board::from_fen("4k3/8/8/8/8/8/4P3/4K3 w - - 0 1").unwrap()));
        // 상대 기물은 상관없음: 둘 차례인 백에게는 폰뿐
        assert!(!has_non_pawn_material(&Board::from_fen("3qk3/8/8/8/8/8/4P3/4K3 w - - 0 1").unwrap()));
    }
}