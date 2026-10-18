use crate::board::Board;
use crate::moves::Move;
use crate::movegen::generate_legal_moves;
use crate::search::{search, MATE};

const START_FEN: &str = "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1";
const DEFAULT_DEPTH: u32 = 5;

pub struct Uci {
    pub board: Board,
    pub quit: bool,
}

impl Uci {
    pub fn new() -> Uci {
        Uci { board: Board::from_fen(START_FEN).unwrap(), quit: false }
    }

    /// 명령 한 줄을 처리하고, 엔진이 보낼 응답 줄들을 돌려준다
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
            "isready" => vec!["readyok".to_string()],
            "position" => {
                self.handle_position(&tokens[1..]);
                vec![]
            },
            "go" => self.handle_go(&tokens[1..]),
            "quit" => {
                self.quit = true;
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

    /// "go ..."의 나머지 토큰 처리
    fn handle_go(&mut self, tokens: &[&str]) -> Vec<String> {
        let depth = tokens
        .iter()
        .position(|&t| t == "depth")
        .and_then(|i| tokens.get(i + 1))
        .and_then(|s| s.parse::<u32>().ok())
        .unwrap_or(DEFAULT_DEPTH)
        .max(1);

        let result = search(&mut self.board, depth);

        let info = format!("info depth {} score {} nodes {}",
        depth, score_to_uci(result.score), result.nodes);

        let mv = match result.best {
            Some(best_mv) => best_mv.to_uci(),
            None => "0000".to_string(),
        };
        let best_move = format!("bestmove {}", mv);

        vec![info, best_move]
    }
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

    fn send(uci: &mut Uci, line: &str) -> Vec<String> {
        uci.handle_line(line)
    }

    #[test]
    fn handshake() {
        let mut u = Uci::new();
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
        let mut u = Uci::new();
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
        let mut u = Uci::new();
        send(&mut u, "position fen 6k1/5ppp/8/8/8/8/8/R5K1 w - - 0 1");
        let out = send(&mut u, "go depth 2");
        assert!(out[0].contains("score mate 1"), "{:?}", out);
        assert_eq!(out.last().unwrap(), "bestmove a1a8");
    }

    #[test]
    fn go_when_no_moves() {
        let mut u = Uci::new();
        send(&mut u, "position fen k7/1Q6/1K6/8/8/8/8/8 b - - 0 1");   // 이미 메이트
        let out = send(&mut u, "go depth 2");
        assert_eq!(out.last().unwrap(), "bestmove 0000");
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
}