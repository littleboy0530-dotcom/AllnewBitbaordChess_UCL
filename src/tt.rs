use crate::moves::Move;
use crate::search::MATE;

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Bound { Exact, Lower, Upper }

#[derive(Clone, Copy, Debug)]
pub struct TtEntry {
    pub key: u64,
    pub best: Option<Move>,
    pub score: i32,
    pub depth: u32,
    pub bound: Bound,
}

pub struct TranspositionTable {
    entries: Vec<Option<TtEntry>>,
    mask: u64,
}

impl TranspositionTable {
    /// 칸 2^bits개 (bits 20 = 약 100만 칸)
    pub fn new(bits: u32) -> TranspositionTable {
        let entries = vec![None;1<<bits];
        let mask = (1 << bits) -1;

        TranspositionTable { entries, mask }
    }
    /// 해시가 일치하는 기록이 있으면 돌려준다
    pub fn probe(&self, key: u64) -> Option<TtEntry> {
        if let Some(entry) = self.entries[(key & self.mask) as usize]{
            if entry.key == key {
                return Some(entry)
            }
        }
        None
    }
    /// 항상 덮어쓴다
    pub fn store(&mut self, key: u64, depth: u32, score: i32, bound: Bound, best: Option<Move>) {
        self.entries[(key & self.mask) as usize] = Some(TtEntry{key, best, score, depth, bound});
    }
    pub fn clear(&mut self) {
        for e in self.entries.iter_mut() {
            *e = None;
        }
    }
}

/// 루트 기준 메이트 점수 → 이 국면 기준 (저장할 때)
pub fn score_to_tt(score: i32, ply: i32) -> i32 {
    if score >= MATE -1000 {
        score + ply
    }else if score <= -MATE +1000 {
        score -ply
    }else{
        score
    }
}
/// 이 국면 기준 메이트 점수 → 루트 기준 (꺼낼 때)
pub fn score_from_tt(score: i32, ply: i32) -> i32 {
    if score >= MATE - 1000 {
        score - ply
    } else if score <= -MATE + 1000 {
        score + ply
    } else {
        score
    }
}