use crate::board::Board;
use crate::types::*;

/// SplitMix64: 시드에서 품질 좋은 난수열을 만드는 간단한 생성기
pub const fn splitmix64(state: u64) -> (u64, u64) {
    let s = state.wrapping_add(0x9E37_79B9_7F4A_7C15);
    let mut z = s;
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    (s, z ^ (z >> 31))
}

pub struct ZobristKeys {
    pub pieces: [[[u64; 64]; 6]; 2],   // [색][기물][칸]
    pub castling: [u64; 16],           // 캐슬링 권리 비트 4개의 조합
    pub ep_file: [u64; 8],             // 앙파상 칸의 파일
    pub side: u64,                     // 흑 차례일 때 XOR
}

/// 컴파일할 때 한 번 계산되는 고정 키
pub const KEYS: ZobristKeys = {//각 해싱값을 컴파일 타임에 생성
    let mut state = 0x1234_5678_9ABC_DEF0u64;   // 고정 시드
    let mut keys = ZobristKeys { pieces: [[[0; 64]; 6]; 2], castling: [0; 16], ep_file: [0; 8], side: 0 };
    let mut c = 0;
    while c < 2 {
        let mut p = 0;
        while p < 6 {
            let mut sq = 0;
            while sq < 64 {
                let (s, r) = splitmix64(state);
                state = s;
                keys.pieces[c][p][sq] = r;
                sq += 1;
            }
            p += 1;
        }
        c += 1;
    }
    let mut i = 0;
    while i < 16 {
        let (s, r) = splitmix64(state);
        state = s;
        keys.castling[i] = r;
        i += 1;
    }
    let mut i = 0;
    while i < 8 {
        let (s, r) = splitmix64(state);
        state = s;
        keys.ep_file[i] = r;
        i += 1;
    }
    let (_, r) = splitmix64(state);
    keys.side = r;
    keys
};

impl Board {
    /// 국면 전체를 처음부터 계산한 해시 (검증과 초기화용)
    pub fn compute_hash(&self) -> u64 {
        let mut hash = 0;
        for side in 0..=1 {
            for p in 0..6 {
                let mut p_board = self.pieces[side][p];
                while p_board != 0 {
                    let sq = pop_lsb(&mut p_board);
                    let piece_key = KEYS.pieces[side][p][sq as usize];
                    hash ^= piece_key;
                }
            }
        }

        hash ^= KEYS.castling[self.castling as usize];

        if let Some(ep_sq) = self.ep{
            hash ^= KEYS.ep_file[file_of(ep_sq) as usize];
        }

        if self.side == Color::Black {
            hash ^= KEYS.side;
        }

        hash
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::movegen::generate_legal_moves;

    const START: &str = "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1";

    fn play(b: &mut Board, uci: &str) {
        let mv = generate_legal_moves(b).into_iter().find(|m| m.to_uci() == uci).expect("없는 수");
        b.make_move(mv);
    }

    #[test]
    fn keys_are_distinct() {
        // 키가 겹치면 서로 다른 국면이 같은 해시를 가질 수 있음
        let mut all: Vec<u64> = KEYS.pieces.iter().flatten().flatten().copied().collect();
        all.extend(KEYS.castling);
        all.extend(KEYS.ep_file);
        all.push(KEYS.side);
        let n = all.len();
        all.sort();
        all.dedup();
        assert_eq!(all.len(), n);
        assert!(!all.contains(&0));
    }

    #[test]
    fn fen_hash_matches_compute() {
        let b = Board::from_fen(START).unwrap();
        assert_eq!(b.hash, b.compute_hash());
        assert_ne!(b.hash, 0);
    }

    #[test]
    fn state_changes_hash() {
        let base = Board::from_fen(START).unwrap().hash;
        // 차례만 다름
        assert_ne!(base, Board::from_fen("rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR b KQkq - 0 1").unwrap().hash);
        // 캐슬링만 다름
        assert_ne!(base, Board::from_fen("rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w Kkq - 0 1").unwrap().hash);
        // 카운터는 해시에 들어가지 않음 (같은 국면으로 봄)
        assert_eq!(base, Board::from_fen("rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 7 30").unwrap().hash);
    }

    #[test]
    fn transposition_same_hash() {
        // 다른 순서로 같은 국면에 도착하면 해시가 같아야 함
        let mut a = Board::from_fen(START).unwrap();
        for m in ["g1f3", "g8f6", "b1c3", "b8c6"] { play(&mut a, m); }
        let mut b = Board::from_fen(START).unwrap();
        for m in ["b1c3", "b8c6", "g1f3", "g8f6"] { play(&mut b, m); }
        assert_eq!(a.hash, b.hash);
        assert_eq!(a.to_fen(), b.to_fen());
    }

    /// 모든 수순에서 증분 해시 == 처음부터 계산한 해시인지 확인
    fn walk(b: &mut Board, depth: u32) {
        assert_eq!(b.hash, b.compute_hash(), "해시 불일치: {}", b.to_fen());
        if depth == 0 { return; }
        for mv in generate_legal_moves(b) {
            let before = b.hash;
            b.make_move(mv);
            walk(b, depth - 1);
            b.unmake_move();
            assert_eq!(b.hash, before, "{} 되돌린 뒤 해시가 다름", mv.to_uci());
        }
    }

    #[test]
    fn incremental_matches_full() {
        for fen in [
            START,
            "r3k2r/p1ppqpb1/bn2pnp1/3PN3/1p2P3/2N2Q1p/PPPBBPPP/R3K2R w KQkq - 0 1",   // 캐슬링, 앙파상
            "r3k2r/Pppp1ppp/1b3nbN/nP6/BBP1P3/q4N2/Pp1P2PP/R2Q1RK1 w kq - 0 1",       // 승격
            "8/2p5/3p4/KP5r/1R3p1k/8/4P1P1/8 w - - 0 1",                              // 앙파상 핀
        ] {
            let mut b = Board::from_fen(fen).unwrap();
            walk(&mut b, 3);
        }
    }
}