use crate::types::*;

/// (파일 변화, 랭크 변화) 목록대로 한 번씩 이동한 칸들 (도약형 공통 도우미)
fn leaper_attacks(sq: Square, deltas: &[(i8, i8)]) -> Bitboard {
    let mut b = 0u64;

    for vector in deltas {
        let f = file_of(sq) as i8 + vector.0;
        let r = rank_of(sq) as i8 + vector.1;
        if (0..8).contains(&f) && (0..8).contains(&r) {
            b |= bit(square(f as u8, r as u8));
        }
    }

    b
}

/// (파일 변화, 랭크 변화) 방향으로 막힐 때까지 뻗어 간 칸들 (슬라이딩형 공통 도우미)
/// 막은 기물이 있는 칸도 포함한다 (그 기물을 잡을 수 있으므로)
/// TODO:추후에 11단계에서 magic bit board로 갈아끼울 것
fn ray_attacks(sq: Square, df: i8, dr: i8, occ: Bitboard) -> Bitboard {
    let mut b = 0u64;
    let mut f = file_of(sq) as i8;
    let mut r = rank_of(sq) as i8;
    loop {
        f += df;
        r += dr;
        if !(0..8).contains(&f) || !(0..8).contains(&r) {
            break;                        // 보드 밖
        }
        let to = bit(square(f as u8, r as u8));
        b |= to;                          // 칸은 항상 포함
        if occ & to != 0 {
            break;                        // 기물을 만나면 거기서 멈춤
        }
    }
    b
}

pub fn knight_attacks(sq: Square) -> Bitboard {
    let knight_deltas = [(2,1),(-2,1),(-2,-1,),(2,-1),(1,2),(-1,2),(-1,-2),(1,-2)];
    leaper_attacks(sq, &knight_deltas)
}
pub fn king_attacks(sq: Square) -> Bitboard {
    let king_deltas = [(1,1),(-1,1),(-1,-1,),(1,-1),(1,0),(0,1),(-1,0),(0,-1)];
    leaper_attacks(sq, &king_deltas)
}
/// 폰이 '잡을 수 있는' 대각선 칸 (전진 칸이 아님). 백은 위쪽, 흑은 아래쪽
pub fn pawn_attacks(color: Color, sq: Square) -> Bitboard {
    let pawn_deltas = match color {
        Color::Black => [(-1,-1),(1,-1)],
        Color::White => [(-1,1),(1,1)],
    };
    leaper_attacks(sq, &pawn_deltas)
}

pub fn rook_attacks(sq: Square, occ: Bitboard) -> Bitboard {
    let mut b = 0u64;
    let rook_deltas = [(1,0),(0,1),(-1,0),(0,-1)];
    for delta in rook_deltas {
        b |= ray_attacks(sq, delta.0, delta.1, occ);
    }

    b
}
pub fn bishop_attacks(sq: Square, occ: Bitboard) -> Bitboard {
    let mut b = 0u64;
    let bishop_deltas = [(1,1),(-1,1),(-1,-1,),(1,-1)];
    for delta in bishop_deltas {
        b |= ray_attacks(sq, delta.0, delta.1, occ);
    }

    b
}
pub fn queen_attacks(sq: Square, occ: Bitboard) -> Bitboard {
    rook_attacks(sq, occ) | bishop_attacks(sq, occ)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// ["e4", "d5"] → 그 칸들이 켜진 비트보드
    fn bb(names: &[&str]) -> Bitboard {
        names.iter().fold(0, |acc, n| acc | bit(parse_square(n).unwrap()))
    }
    fn sq(name: &str) -> Square {
        parse_square(name).unwrap()
    }

    #[test]
    fn knight() {
        assert_eq!(knight_attacks(sq("e4")), bb(&["d6", "f6", "g5", "g3", "f2", "d2", "c3", "c5"]));
        assert_eq!(knight_attacks(sq("a1")), bb(&["b3", "c2"]));            // 구석
        assert_eq!(knight_attacks(sq("h8")), bb(&["g6", "f7"]));
        assert_eq!(knight_attacks(sq("h4")).count_ones(), 4);               // 오른쪽 가장자리
    }

    #[test]
    fn king() {
        assert_eq!(king_attacks(sq("a1")), bb(&["a2", "b2", "b1"]));
        assert_eq!(king_attacks(sq("e4")).count_ones(), 8);
        assert_eq!(king_attacks(sq("h5")), bb(&["h6", "g6", "g5", "g4", "h4"]));  // a파일로 넘어가면 안 됨
    }

    #[test]
    fn pawn() {
        assert_eq!(pawn_attacks(Color::White, sq("e4")), bb(&["d5", "f5"]));
        assert_eq!(pawn_attacks(Color::Black, sq("e4")), bb(&["d3", "f3"]));
        assert_eq!(pawn_attacks(Color::White, sq("a2")), bb(&["b3"]));      // 가장자리
        assert_eq!(pawn_attacks(Color::Black, sq("h7")), bb(&["g6"]));
        assert_eq!(pawn_attacks(Color::White, sq("e8")), 0);                // 위로 갈 칸 없음
    }

    #[test]
    fn rook_empty_board() {
        assert_eq!(rook_attacks(sq("a1"), 0).count_ones(), 14);             // 빈 보드에서는 항상 14칸
        assert_eq!(rook_attacks(sq("e4"), 0).count_ones(), 14);
        assert_eq!(rook_attacks(sq("a1"), 0) & bit(sq("a1")), 0);           // 자기 칸은 포함하지 않음
    }

    #[test]
    fn rook_blocked() {
        // d4 룩, d6과 f4에 기물 → 막은 칸까지 포함하고 그 뒤로는 못 감
        let occ = bb(&["d4", "d6", "f4"]);
        let expected = bb(&["d5", "d6",                    // 위: d6에서 멈춤
                            "d3", "d2", "d1",              // 아래: 끝까지
                            "e4", "f4",                    // 오른쪽: f4에서 멈춤
                            "c4", "b4", "a4"]);            // 왼쪽: 끝까지
        assert_eq!(rook_attacks(sq("d4"), occ), expected);
    }

    #[test]
    fn bishop() {
        assert_eq!(bishop_attacks(sq("a1"), 0).count_ones(), 7);            // 대각선 하나
        assert_eq!(bishop_attacks(sq("d4"), 0).count_ones(), 13);
        // c1 비숍, d2에 기물 → 오른쪽 위는 d2에서 멈춤
        assert_eq!(bishop_attacks(sq("c1"), bb(&["d2"])), bb(&["d2", "b2", "a3"]));
    }

    #[test]
    fn queen() {
        let occ = bb(&["d4", "d6", "f6", "b2"]);
        assert_eq!(queen_attacks(sq("d4"), occ), rook_attacks(sq("d4"), occ) | bishop_attacks(sq("d4"), occ));
        assert_eq!(queen_attacks(sq("d4"), 0).count_ones(), 27);
    }
}