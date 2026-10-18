mod types;
mod board;
mod attacks;
mod moves;
mod movegen;
mod makemoves;
mod perft;

use std::time::Instant;
use board::Board;

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
    perft_bench()
}
