// src/bench.rs
// 벤치마크: 고정된 국면들을 고정 깊이로 탐색해서 노드 수와 속도를 잰다
//
// - 노드 수(Nodes)는 탐색 결과의 "지문": 탐색 로직이 같으면 PC 부하와 상관없이 항상 같다
//   → 속도만 바꾼 변경(매직 비트보드 등)은 노드 수가 그대로여야 하고,
//     탐색을 바꾼 변경(LMR 등)은 노드 수가 바뀐다
// - nps는 PC 상태에 따라 흔들리니 여러 번 재서 비교한다
use crate::attacks::init_tables;
use crate::board::Board;
use crate::perft::perft;
use crate::search::{iterative_deepening, Limits};
use crate::tt::TranspositionTable;
use std::time::Instant;

pub const DEFAULT_DEPTH: u32 = 8;

/// 국면마다 새 TT를 쓰므로 크기가 결과(노드 수)에 영향을 준다 → 바꾸면 기준값도 다시 재야 함
const BENCH_TT_BITS: u32 = 20;

const POSITIONS: [(&str, &str); 10] = [
    ("시작 국면", "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1"),
    ("이탈리안", "r1bqkbnr/pppp1ppp/2n5/4p3/2B1P3/5N2/PPPP1PPP/RNBQK2R b KQkq - 3 3"),
    ("Kiwipete", "r3k2r/p1ppqpb1/bn2pnp1/3PN3/1p2P3/2N2Q1p/PPPBBPPP/R3K2R w KQkq - 0 1"),
    ("perft 4", "r3k2r/Pppp1ppp/1b3nbN/nP6/BBP1P3/q4N2/Pp1P2PP/R2Q1RK1 w kq - 0 1"),
    ("perft 5", "rnbq1k1r/pp1Pbppp/2p5/8/2B5/8/PPP1NnPP/RNBQK2R w KQ - 1 8"),
    ("perft 6", "r4rk1/1pp1qppp/p1np1n2/2b1p1B1/2B1P1b1/P1NP1N2/1PP1QPPP/R4RK1 w - - 0 10"),
    ("퀸스 갬빗 중반", "r1bq1rk1/pp2bppp/2n1pn2/2pp4/2PP4/2N1PN2/PP2BPPP/R1BQ1RK1 w - - 0 8"),
    ("룩 끝내기", "8/5pk1/6p1/8/2R5/6P1/r4PK1/8 w - - 0 40"),
    ("perft 3 (끝내기)", "8/2p5/3p4/KP5r/1R3p1k/8/4P1P1/8 w - - 0 1"),
    ("킹-폰 끝내기", "8/8/8/4k3/8/3K4/4P3/8 w - - 0 1"),
];

/// 공격 표가 준비됐는지 확인하고, 만드는 데 걸린 시간을 보여 준다 (보통 main에서 이미 만들어져 0 ms)
fn warm_up_tables() {
    let t = Instant::now();
    init_tables();
    println!("(공격 표 준비 {} ms — 측정에서 제외)", t.elapsed().as_millis());
}

/// `bench [깊이]`: 탐색 벤치마크
pub fn run_search_bench(depth: u32) {
    warm_up_tables();
    println!("탐색 벤치 (깊이 {}, TT 2^{}칸, 국면마다 새 TT)", depth, BENCH_TT_BITS);
    let mut total_nodes = 0u64;
    let start = Instant::now();

    for (name, fen) in POSITIONS {
        let mut board = Board::from_fen(fen).unwrap();
        let mut tt = TranspositionTable::new(BENCH_TT_BITS);
        let t = Instant::now();
        let r = iterative_deepening(&mut board, &Limits::depth(depth), &mut tt, |_, _| {});
        let ms = t.elapsed().as_millis();
        let best = r.best.map_or("0000".to_string(), |m| m.to_uci());
        println!("  {:<18} {:>11} 노드 {:>7} ms  최선수 {:<6} 점수 {}", name, r.nodes, ms, best, r.score);
        total_nodes += r.nodes;
    }

    let secs = start.elapsed().as_secs_f64();
    println!("===========================");
    println!("Nodes: {}", total_nodes);
    println!("Time:  {:.2}초", secs);
    println!("NPS:   {:.0}", total_nodes as f64 / secs);
}

/// `bench perft`: 수 생성 속도 (탐색과 상관없이 movegen·make/unmake만)
pub fn run_perft_bench() {
    warm_up_tables();
    let cases = [
        ("시작 국면", POSITIONS[0].1, 5),
        ("Kiwipete", POSITIONS[2].1, 4),
    ];
    for (name, fen, depth) in cases {
        let mut b = Board::from_fen(fen).unwrap();
        let start = Instant::now();
        let nodes = perft(&mut b, depth);
        let secs = start.elapsed().as_secs_f64();
        println!("{} 깊이 {}: {} 노드, {:.2}초, {:.1}M 노드/초",
                 name, depth, nodes, secs, nodes as f64 / secs / 1e6);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bench_positions_are_valid() {
        for (name, fen) in POSITIONS {
            let b = Board::from_fen(fen).unwrap_or_else(|_| panic!("FEN 오류: {}", name));
            assert_eq!(b.to_fen(), fen, "{}", name);
        }
    }
}
