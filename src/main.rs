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
mod bench;

use std::io::{self, BufRead, Write};
use std::sync::mpsc;
use std::thread;
use uci::Uci;

fn main() {
    // `bench [깊이]` → 탐색 벤치, `bench perft` → perft 벤치, 그 외에는 UCI 모드
    let args: Vec<String> = std::env::args().collect();
    if args.get(1).map(String::as_str) == Some("bench") {
        match args.get(2).map(String::as_str) {
            Some("perft") => bench::run_perft_bench(),
            Some(d) => bench::run_search_bench(d.parse().unwrap_or(bench::DEFAULT_DEPTH)),
            None => bench::run_search_bench(bench::DEFAULT_DEPTH),
        }
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
