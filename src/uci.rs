use crate::board::Board;
use crate::moves::Move;
use crate::movegen::{generate_legal_moves, in_check};
use crate::search::{iterative_deepening, Limits, MATE};
use crate::tt::TranspositionTable;
use crate::types::Color;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::Duration;

const START_FEN: &str = "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1";
const DEFAULT_DEPTH: u32 = 5;
/// 시간 제한으로 탐색할 때의 최대 깊이 (실제로는 시간이 먼저 멈춤)
const MAX_DEPTH: u32 = 64;
/// bestmove를 보낸 뒤 GUI가 시계를 멈추기까지의 여유 시간
const MOVE_OVERHEAD_MS: u64 = 30;
/// movestogo가 없을 때 가정하는 남은 수
const DEFAULT_MOVES_TO_GO: u64 = 30;
/// 전치 테이블 크기: 칸 2^20개 (약 100만 칸)
const TT_BITS: u32 = 20;

/// UCI `go` 명령의 파라미터
#[derive(Default, Debug, PartialEq)]
struct GoParams {
    depth: Option<u32>,
    movetime: Option<u64>,
    wtime: Option<u64>,
    btime: Option<u64>,
    winc: u64,
    binc: u64,
    movestogo: Option<u64>,
    infinite: bool,
}

/// 진행 중인 탐색 하나
struct SearchJob {
    stop: Arc<AtomicBool>,   // 이 탐색만의 정지 신호
    handle: JoinHandle<()>,
}

pub struct Uci {
    pub board: Board,
    pub quit: bool,
    /// 탐색 스레드가 info·bestmove를 보내는 출력 통로
    out: Sender<String>,
    search: Option<SearchJob>,
    /// 대국 내내 유지하는 전치 테이블 (탐색 스레드와 함께 쓰므로 Mutex로 감쌈)
    tt: Arc<Mutex<TranspositionTable>>,
}

impl Uci {
    pub fn new(out: Sender<String>) -> Uci {
        Uci {
            board: Board::from_fen(START_FEN).unwrap(),
            quit: false,
            out,
            search: None,
            tt: Arc::new(Mutex::new(TranspositionTable::new(TT_BITS))),
        }
    }

    /// 진행 중인 탐색에 정지 신호를 보내고 끝날 때까지 기다린다 (bestmove는 탐색 스레드가 보냄)
    pub fn stop_search(&mut self) {
        if let Some(job) = self.search.take() {
            job.stop.store(true, Ordering::Relaxed);
            let _ = job.handle.join();
        }
    }

    /// 진행 중인 탐색이 스스로 끝날 때까지 기다린다 (입력이 끝났을 때 사용)
    pub fn wait_search(&mut self) {
        if let Some(job) = self.search.take() {
            let _ = job.handle.join();
        }
    }

    /// 명령 한 줄을 처리하고, 바로 보낼 응답 줄들을 돌려준다
    /// (탐색 결과는 탐색 스레드가 out으로 따로 보낸다)
    pub fn handle_line(&mut self, line: &str) -> Vec<String> {
        let tokens:Vec<&str> = line.split_whitespace().collect();
        let Some(&cmd) = tokens.first() else { return vec![] };
        match cmd {
            "uci" => {
                vec![
                    "id name AllnewBitboardChess".to_string(),
                    "id author Joon".to_string(),
                    "uciok".to_string(),
                ]
            },
            // 탐색 중에도 바로 응답해야 함 (GUI가 엔진이 살아 있는지 확인하는 용도)
            "isready" => vec!["readyok".to_string()],
            "ucinewgame" => {
                self.stop_search();
                self.board = Board::from_fen(START_FEN).unwrap();
                self.tt.lock().unwrap().clear();   // 지난 대국의 기록은 필요 없음
                vec![]
            },
            "position" => {
                self.stop_search();
                self.handle_position(&tokens[1..]);
                vec![]
            },
            "go" => {
                self.handle_go(&tokens[1..]);
                vec![]
            },
            "stop" => {
                self.stop_search();
                vec![]
            },
            "quit" => {
                self.stop_search();
                self.quit = true;
                vec![]
            },
            "state" => self.handle_state(),
            "print" => {
                self.board.print();
                vec![]
            },
            _ =>{
                vec![]
            }
        }
    }

    /// "position ..."의 나머지 토큰 처리
    fn handle_position(&mut self, tokens: &[&str]) {
        let moves_index = tokens.iter().position(|&t| t == "moves");
        let head = &tokens[..moves_index.unwrap_or(tokens.len())];

        let fen = match head.first() {
            Some(&"startpos") => START_FEN.to_string(),
            Some(&"fen") => head[1..].join(" "),
            _ => return,
        };

        let Ok(mut board) = Board::from_fen(&fen)else{ return };

        if let Some(i) = moves_index {
            for s in &tokens[i +1..] {
                let Some(mv) = parse_move(&mut board, s) else { return };
                board.make_move(mv);
            }
        }

        self.board = board;
    }

    /// "go ..."의 나머지 토큰 처리: 탐색 스레드를 띄우고 바로 돌아온다
    fn handle_go(&mut self, tokens: &[&str]) {
        self.stop_search();   // 탐색은 항상 하나만

        let params = parse_go(tokens);
        let time = time_budget(&params, self.board.side == Color::White);
        // 깊이 지정이 우선. 없으면 infinite나 시간 제한일 때는 멈출 때까지, 둘 다 없으면 기본 깊이
        let depth = match (params.depth, time) {
            (Some(d), _) => d.max(1),
            (None, _) if params.infinite => MAX_DEPTH,
            (None, Some(_)) => MAX_DEPTH,
            (None, None) => DEFAULT_DEPTH,
        };

        // 정지 신호는 스레드를 띄우기 '전에' 새로 만든다.
        // 그래서 go 직후에 stop이 와도 신호가 사라지지 않는다 (옛 엔진의 멈춤 버그 원인)
        let stop = Arc::new(AtomicBool::new(false));
        let limits = Limits { depth, time, stop: Some(Arc::clone(&stop)) };
        let infinite = params.infinite;
        let stop_seen = Arc::clone(&stop);
        let mut board = self.board.clone();   // 탐색 스레드 전용 보드
        let out = self.out.clone();
        let tt = Arc::clone(&self.tt);

        let handle = thread::spawn(move || {
            let mut tt = tt.lock().unwrap();   // 탐색하는 동안 이 스레드만 씀
            let result = iterative_deepening(&mut board, &limits, &mut tt, |d, r| {
                let nps = if r.time_ms > 0 { r.nodes * 1000 / r.time_ms } else { 0 };
                let _ = out.send(format!("info depth {} score {} nodes {} time {} nps {}",
                    d, score_to_uci(r.score), r.nodes, r.time_ms, nps));
            });
            // UCI 규약: infinite 탐색은 stop을 받기 전에는 bestmove를 보내면 안 됨
            while infinite && !stop_seen.load(Ordering::Relaxed) {
                thread::sleep(Duration::from_millis(5));
            }
            let mv = match result.best {
                Some(best_mv) => best_mv.to_uci(),
                None => "0000".to_string(),
            };
            let _ = out.send(format!("bestmove {}", mv));
        });

        self.search = Some(SearchJob { stop, handle });
    }
    fn handle_state(&mut self) -> Vec<String> {
        let fen = self.board.to_fen();
        let check = if in_check(&self.board) {1} else {0};
        let legals:Vec<String>= generate_legal_moves(&mut self.board)
        .iter()
        .map(|&mv| mv.to_uci())
        .collect();
        let legal_uci = legals.join(" ");

        vec![
            format!("state fen {}", fen),
            format!("state check {}", check),
            format!("state legal {}", legal_uci),
            format!("stateok"),
        ]
    }
}

impl Drop for Uci {
    /// Uci가 사라질 때 탐색 스레드를 남겨 두지 않는다
    fn drop(&mut self) {
        self.stop_search();
    }
}

/// go 토큰 → GoParams. 모르는 토큰은 무시한다
fn parse_go(tokens: &[&str]) -> GoParams {
    let mut p = GoParams::default();
    let mut i = 0;
    while i < tokens.len() {
        // 숫자 값을 받는 키: 다음 토큰을 읽고 하나 더 건너뜀
        let num = || tokens.get(i + 1).and_then(|s| s.parse::<u64>().ok());
        match tokens[i] {
            "depth" => { p.depth = num().map(|v| v as u32); i += 1; }
            "movetime" => { p.movetime = num(); i += 1; }
            "wtime" => { p.wtime = num(); i += 1; }
            "btime" => { p.btime = num(); i += 1; }
            "winc" => { p.winc = num().unwrap_or(0); i += 1; }
            "binc" => { p.binc = num().unwrap_or(0); i += 1; }
            "movestogo" => { p.movestogo = num(); i += 1; }
            "infinite" => p.infinite = true,
            _ => {}
        }
        i += 1;
    }
    p
}

/// 이번 수에 쓸 시간. None이면 시간 제한 없음
fn time_budget(p: &GoParams, white_to_move: bool) -> Option<Duration> {
    if p.infinite {
        return None;
    }
    let ms = if let Some(mt) = p.movetime {
        mt.saturating_sub(MOVE_OVERHEAD_MS)
    } else {
        let (time, inc) = if white_to_move { (p.wtime?, p.winc) } else { (p.btime?, p.binc) };
        let moves_to_go = p.movestogo.unwrap_or(DEFAULT_MOVES_TO_GO).max(1);
        let share = time / moves_to_go + inc * 3 / 4;   // 남은 수만큼 나눠 쓰기
        let cap = time / 2;                             // 한 수에 절반 넘게 쓰지 않기
        share.min(cap).saturating_sub(MOVE_OVERHEAD_MS)
    };
    Some(Duration::from_millis(ms.max(1)))
}

/// "e2e4", "e7e8q" → 지금 국면의 합법수 중 같은 것
fn parse_move(board: &mut Board, s: &str) -> Option<Move> {
    generate_legal_moves(board).into_iter().find(|m| m.to_uci() == s)
}

/// 탐색 점수 → UCI 점수 표기 ("cp 35" 또는 "mate 3", "mate -2")
fn score_to_uci(score: i32) -> String {
    if score >= MATE -1000 {
        let ply = MATE - score;
        let num = (ply + 1)/2;
        format!("mate {}", num)
    }else if score <= -MATE +1000 {
        let ply = MATE +score;
        let num = (ply +1)/2;
        format!("mate {}", -num)
    }else{
        format!("cp {}", score)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc::{channel, Receiver};
    use std::time::Instant;

    fn new_uci() -> (Uci, Receiver<String>) {
        let (tx, rx) = channel();
        (Uci::new(tx), rx)
    }

    /// 바로 오는 응답
    fn send(uci: &mut Uci, line: &str) -> Vec<String> {
        uci.handle_line(line)
    }

    /// 탐색 스레드가 보내는 줄을 bestmove가 나올 때까지 모은다
    fn collect(rx: &Receiver<String>) -> Vec<String> {
        let mut lines = Vec::new();
        loop {
            let line = rx.recv_timeout(Duration::from_secs(20)).expect("bestmove가 오지 않음");
            let done = line.starts_with("bestmove");
            lines.push(line);
            if done { return lines; }
        }
    }

    /// go를 보내고 bestmove까지의 출력을 돌려준다
    fn go(uci: &mut Uci, rx: &Receiver<String>, line: &str) -> Vec<String> {
        assert!(send(uci, line).is_empty(), "go는 바로 돌아와야 함");
        collect(rx)
    }

    #[test]
    fn handshake() {
        let (mut u, _rx) = new_uci();
        let out = send(&mut u, "uci");
        assert!(out[0].starts_with("id name"));
        assert_eq!(out.last().unwrap(), "uciok");
        assert_eq!(send(&mut u, "isready"), vec!["readyok"]);
        assert!(send(&mut u, "아무거나").is_empty());   // 모르는 명령은 무시
        send(&mut u, "quit");
        assert!(u.quit);
    }

    #[test]
    fn position_commands() {
        let (mut u, _rx) = new_uci();
        send(&mut u, "position startpos moves e2e4 e7e5 g1f3");
        assert_eq!(u.board.to_fen(), "rnbqkbnr/pppp1ppp/8/4p3/4P3/5N2/PPPP1PPP/RNBQKB1R b KQkq - 1 2");

        send(&mut u, "position fen 4k3/P7/8/8/8/8/8/4K3 w - - 0 1 moves a7a8q");
        assert_eq!(u.board.to_fen(), "Q3k3/8/8/8/8/8/8/4K3 b - - 0 1");

        send(&mut u, "position startpos");
        assert_eq!(u.board.to_fen(), START_FEN);

        // 불법 수가 섞이면 국면을 바꾸지 않음
        send(&mut u, "position startpos moves e2e5");
        assert_eq!(u.board.to_fen(), START_FEN);
    }

    #[test]
    fn go_finds_mate() {
        let (mut u, rx) = new_uci();
        send(&mut u, "position fen 6k1/5ppp/8/8/8/8/8/R5K1 w - - 0 1");
        let out = go(&mut u, &rx, "go depth 2");
        // 마지막 info 줄(bestmove 바로 앞)에 메이트 점수가 있어야 함
        assert!(out[out.len() - 2].contains("score mate 1"), "{:?}", out);
        assert_eq!(out.last().unwrap(), "bestmove a1a8");
    }

    #[test]
    fn parse_go_params() {
        let p = parse_go(&["wtime", "60000", "btime", "58000", "winc", "1000", "binc", "500"]);
        assert_eq!(p.wtime, Some(60000));
        assert_eq!(p.btime, Some(58000));
        assert_eq!((p.winc, p.binc), (1000, 500));
        assert_eq!(p.depth, None);

        let p = parse_go(&["depth", "7"]);
        assert_eq!(p.depth, Some(7));
        assert!(parse_go(&["infinite"]).infinite);
        assert_eq!(parse_go(&[]), GoParams::default());
        // 값이 빠졌거나 숫자가 아니어도 패닉 없이 무시
        assert_eq!(parse_go(&["depth"]).depth, None);
        assert_eq!(parse_go(&["wtime", "abc"]).wtime, None);
    }

    #[test]
    fn time_budget_rules() {
        let ms = |d: Option<Duration>| d.map(|d| d.as_millis() as u64);
        // movetime은 여유 시간만 뺌
        assert_eq!(ms(time_budget(&parse_go(&["movetime", "1000"]), true)), Some(970));
        // 60초, 증가 1초: 60000/30 + 750 - 30
        let p = parse_go(&["wtime", "60000", "btime", "1000", "winc", "1000", "binc", "0"]);
        assert_eq!(ms(time_budget(&p, true)), Some(2720));
        // 흑은 btime을 봄: 1000/30 + 0 = 33, 상한 500 → 33 - 30 = 3
        assert_eq!(ms(time_budget(&p, false)), Some(3));
        // 시간이 거의 없어도 최소 1ms
        let p = parse_go(&["wtime", "10", "btime", "10"]);
        assert_eq!(ms(time_budget(&p, true)), Some(1));
        // movestogo가 있으면 그만큼 나눔: 60000/10 - 30
        let p = parse_go(&["wtime", "60000", "btime", "60000", "movestogo", "10"]);
        assert_eq!(ms(time_budget(&p, true)), Some(5970));
        // 시간 정보가 없으면 제한 없음
        assert_eq!(time_budget(&parse_go(&["depth", "5"]), true), None);
        assert_eq!(time_budget(&parse_go(&["infinite"]), true), None);
    }

    #[test]
    fn go_with_clock_moves_in_time() {
        // 시계 기준으로 탐색해도 정해진 시간 안에 bestmove를 내야 함
        let (mut u, rx) = new_uci();
        send(&mut u, "position fen r3k2r/p1ppqpb1/bn2pnp1/3PN3/1p2P3/2N2Q1p/PPPBBPPP/R3K2R w KQkq - 0 1");
        let start = std::time::Instant::now();
        let out = go(&mut u, &rx, "go wtime 3000 btime 3000");   // 약 70ms 배정
        assert!(start.elapsed().as_millis() < 2000, "너무 오래 걸림: {:?}", start.elapsed());
        assert!(out.last().unwrap().starts_with("bestmove "));
        assert_ne!(out.last().unwrap(), "bestmove 0000");
    }

    #[test]
    fn go_movetime() {
        let (mut u, rx) = new_uci();
        let start = std::time::Instant::now();
        let out = go(&mut u, &rx, "go movetime 200");
        assert!(start.elapsed().as_millis() < 2000, "{:?}", start.elapsed());
        assert!(out[0].contains(" time "), "{:?}", out);
        assert!(out.last().unwrap().starts_with("bestmove "));
    }

    #[test]
    fn go_reports_each_depth() {
        let (mut u, rx) = new_uci();
        let out = go(&mut u, &rx, "go depth 3");
        assert_eq!(out.len(), 4);   // info 3줄 + bestmove
        assert!(out[0].starts_with("info depth 1 "));
        assert!(out[2].starts_with("info depth 3 "));
        assert!(out[3].starts_with("bestmove "));
    }

    #[test]
    fn go_when_no_moves() {
        let (mut u, rx) = new_uci();
        send(&mut u, "position fen k7/1Q6/1K6/8/8/8/8/8 b - - 0 1");   // 이미 메이트
        let out = go(&mut u, &rx, "go depth 2");
        assert_eq!(out.last().unwrap(), "bestmove 0000");
    }

    #[test]
    fn go_returns_immediately_and_stays_responsive() {
        // 탐색 중에도 명령을 받아야 함: go는 바로 돌아오고, isready에 바로 답함
        let (mut u, rx) = new_uci();
        let start = Instant::now();
        assert!(send(&mut u, "go infinite").is_empty());
        assert!(start.elapsed() < Duration::from_millis(100), "go가 탐색을 기다림");
        assert_eq!(send(&mut u, "isready"), vec!["readyok"]);
        thread::sleep(Duration::from_millis(100));
        send(&mut u, "stop");
        let out = collect(&rx);
        assert!(out.iter().any(|l| l.starts_with("info depth")));
        assert_ne!(out.last().unwrap(), "bestmove 0000");
    }

    #[test]
    fn infinite_waits_for_stop() {
        // 1수 메이트는 금방 찾지만, infinite면 stop 전에는 bestmove를 보내면 안 됨
        let (mut u, rx) = new_uci();
        send(&mut u, "position fen 6k1/5ppp/8/8/8/8/8/R5K1 w - - 0 1");
        send(&mut u, "go infinite");
        thread::sleep(Duration::from_millis(300));
        let early: Vec<String> = rx.try_iter().collect();
        assert!(early.iter().all(|l| !l.starts_with("bestmove")), "{:?}", early);
        send(&mut u, "stop");
        assert_eq!(collect(&rx).last().unwrap(), "bestmove a1a8");
    }

    #[test]
    fn stop_right_after_go_never_hangs() {
        // 옛 엔진의 멈춤 버그: go infinite 직후 stop이 오면 정지 신호가 사라졌음
        let (mut u, rx) = new_uci();
        for _ in 0..50 {
            send(&mut u, "go infinite");
            send(&mut u, "stop");
            let out = collect(&rx);   // 20초 안에 bestmove가 안 오면 실패
            assert!(out.last().unwrap().starts_with("bestmove "));
        }
    }

    #[test]
    fn new_commands_stop_running_search() {
        // 탐색 중에 position이나 quit이 오면 탐색을 멈추고 bestmove를 보낸 뒤 처리
        let (mut u, rx) = new_uci();
        send(&mut u, "go infinite");
        send(&mut u, "position startpos moves e2e4");
        assert!(collect(&rx).last().unwrap().starts_with("bestmove "));
        assert_eq!(u.board.to_fen(), "rnbqkbnr/pppppppp/8/8/4P3/8/PPPP1PPP/RNBQKBNR b KQkq e3 0 1");

        send(&mut u, "go infinite");
        send(&mut u, "quit");
        assert!(u.quit);
        assert!(collect(&rx).last().unwrap().starts_with("bestmove "));
    }

    #[test]
    fn mate_scores() {
        assert_eq!(score_to_uci(35), "cp 35");
        assert_eq!(score_to_uci(-120), "cp -120");
        assert_eq!(score_to_uci(MATE - 1), "mate 1");
        assert_eq!(score_to_uci(MATE - 3), "mate 2");
        assert_eq!(score_to_uci(-MATE + 2), "mate -1");
        assert_eq!(score_to_uci(-MATE + 4), "mate -2");
    }

    #[test]
    fn state_command() {
        let (mut u, _rx) = new_uci();
        send(&mut u, "position startpos moves e2e4");
        let out = send(&mut u, "state");
        assert_eq!(out[0], "state fen rnbqkbnr/pppppppp/8/8/4P3/8/PPPP1PPP/RNBQKBNR b KQkq e3 0 1");
        assert_eq!(out[1], "state check 0");
        assert!(out[2].starts_with("state legal "));
        assert_eq!(out[2].split_whitespace().count() - 2, 20);   // 흑의 합법수 20개
        assert_eq!(out.last().unwrap(), "stateok");

        // 메이트 국면: 체크 1, 합법수 없음
        send(&mut u, "position fen k7/1Q6/1K6/8/8/8/8/8 b - - 0 1");
        let out = send(&mut u, "state");
        assert_eq!(out[1], "state check 1");
        assert_eq!(out[2].trim(), "state legal");
    }
}