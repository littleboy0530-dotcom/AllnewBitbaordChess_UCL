mod types;
mod board;
mod attacks;
mod moves;
mod movegen;
mod makemoves;
mod perft;
mod zobrist;
mod eval;
mod search;
mod uci;
mod tt;

use std::io::{self, BufRead, Write};
use std::sync::mpsc;
use std::thread;
use std::time::Instant;
use board::Board;
use uci::Uci;

fn perft_bench() {
    let cases = [
        ("시작 국면", "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1", 5),
        ("Kiwipete", "r3k2r/p1ppqpb1/bn2pnp1/3PN3/1p2P3/2N2Q1p/PPPBBPPP/R3K2R w KQkq - 0 1", 4),
    ];
    for (name, fen, depth) in cases {
        let mut b = Board::from_fen(fen).unwrap();
        let start = Instant::now();
        let nodes = perft::perft(&mut b, depth);
        let secs = start.elapsed().as_secs_f64();
        println!("{} 깊이 {}: {} 노드, {:.2}초, {:.1}M 노드/초",
                 name, depth, nodes, secs, nodes as f64 / secs / 1e6);
    }
}

fn main() {
    // `cargo run --release -- bench`면 perft 벤치마크, 아니면 UCI 모드
    if std::env::args().any(|a| a == "bench") {
        perft_bench();
        return;
    }

    // 출력은 전부 이 통로로 모아서 출력 스레드 하나가 내보낸다
    // (명령 응답과 탐색 스레드의 info·bestmove가 한 줄씩 섞이지 않게)
    let (tx, rx) = mpsc::channel::<String>();
    let printer = thread::spawn(move || {
        let stdout = io::stdout();
        for line in rx {
            let mut out = stdout.lock();
            let _ = writeln!(out, "{}", line);
            let _ = out.flush();
        }
    });

    let mut uci = Uci::new(tx.clone());
    let stdin = io::stdin();
    for line in stdin.lock().lines() {
        let Ok(line) = line else { break };
        for out in uci.handle_line(line.trim()) {
            let _ = tx.send(out);
        }
        if uci.quit { break; }
    }

    // 입력이 끝났으면(quit 없이) 진행 중인 탐색은 끝까지 마치고 bestmove를 보낸다
    uci.wait_search();
    drop(uci);
    drop(tx);            // 보내는 쪽이 모두 사라지면 출력 스레드가 끝남
    let _ = printer.join();
}
