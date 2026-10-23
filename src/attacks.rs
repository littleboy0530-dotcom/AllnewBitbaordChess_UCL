use crate::types::*;
use crate::zobrist::splitmix64;
use std::sync::LazyLock;

const FILE_H:Bitboard = 0x_80_80_80_80__80_80_80_80;
const FILE_A:Bitboard = 0x_01_01_01_01__01_01_01_01;
const RANK_1:Bitboard = 0x_00_00_00_00__00_00_00_FF;
const RANK_8:Bitboard = 0x_FF_00_00_00__00_00_00_00;

const KNIGHT_DELTAS:[(i8, i8);8] = [(2,1),(-2,1),(-2,-1,),(2,-1),(1,2),(-1,2),(-1,-2),(1,-2)];
const KING_DELTAS:[(i8, i8);8] = [(1,1),(-1,1),(-1,-1,),(1,-1),(1,0),(0,1),(-1,0),(0,-1)];
const PAWN_DELTAS_BLACK:[(i8, i8);2] = [(-1,-1),(1,-1)];
const PAWN_DELTAS_WHITE:[(i8, i8);2] = [(-1,1),(1,1)];

struct LeaperTables {
    knight: [Bitboard; 64],
    king: [Bitboard; 64],
    pawn: [[Bitboard; 64]; 2],   // [색][칸]
}
static LEAPERS: LazyLock<LeaperTables> = LazyLock::new(|| {
    let mut knight = [0u64;64];
    let mut king = [0u64;64];
    let mut pawn = [[0u64;64];2];
    for sq in 0..64 {
        knight[sq] = leaper_attacks(sq as u8, &KNIGHT_DELTAS);
        king[sq] = leaper_attacks(sq as u8, &KING_DELTAS);
        pawn[Color::White as usize][sq as usize] = leaper_attacks(sq as u8, &PAWN_DELTAS_WHITE);
        pawn[Color::Black as usize][sq as usize] = leaper_attacks(sq as u8, &PAWN_DELTAS_BLACK);
    }

    LeaperTables { knight, king, pawn }
});

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
#[derive(Clone, Copy, Default)]
struct Magic {
    mask: Bitboard,   // 끝 칸을 뺀 관련 칸
    magic: u64,
    shift: u32,       // 64 - mask의 비트 수
    offset: usize,    // 공용 표에서 이 칸의 시작 위치
}

struct SliderTables {
    rook: [Magic; 64],
    bishop: [Magic; 64],
    table: Vec<Bitboard>,   // 모든 칸의 답을 이어 붙인 표
}
static SLIDERS: LazyLock<SliderTables> = LazyLock::new(build_sliders);

/// 미리 찾아 둔 매직 (테스트 print_magics로 다시 만들 수 있음). 시작할 때 매직을 찾지 않고 표만 채운다
const ROOK_MAGICS: [u64; 64] = [
    0x8a80004002801024, 0x0340400020001000, 0x0200081020804204, 0x0100082005001000,
    0x0100020800050010, 0x0480010200440080, 0x2100040082006100, 0x2280102080044100,
    0x4021800a20804000, 0x0004401000402000, 0x4401004100102000, 0x00a0040082004100,
    0x1801800400832800, 0x080280801a000400, 0x2849010002000401, 0x08010001001148a2,
    0x00c0618000804003, 0x0080808040002008, 0x8000260012004480, 0x5204808010040800,
    0x2028008008040080, 0x0080808004000200, 0x0808040008010210, 0x220152000d004084,
    0x108a400680008028, 0x0000200080804000, 0x0810004101002000, 0x0880102200420008,
    0x0068002980040080, 0x0004010040400200, 0x2000820400285001, 0x2300004200088409,
    0x0020204000800080, 0x0c00200040401000, 0x3004401202002084, 0x0d01002209001002,
    0x5000080101001004, 0x6004800200800400, 0x6028581014000201, 0x0400800040800100,
    0x0200804000208000, 0x0410004020004009, 0x0000200104110040, 0x812200401022000a,
    0x1a30040008008080, 0x0101000400490002, 0x0812002108820054, 0x004010a041020014,
    0x4080004001200240, 0x0002008021004200, 0x8960100080200180, 0x0050008048001180,
    0x1081000412080100, 0x4044020080040080, 0x0021900208018400, 0x2b89001200a04100,
    0x00018003041060c1, 0x01020120c1021482, 0x000012a0000900c1, 0x2004100101200409,
    0x000a000460081016, 0x000b000802040001, 0x0002480201209004, 0x0002850400448432,
];
const BISHOP_MAGICS: [u64; 64] = [
    0x8008420808010010, 0x0304140404202880, 0x0412040042041804, 0x00080a0028000003,
    0x0004242040081080, 0x2002080504800023, 0x0283210820240080, 0x0082020504012414,
    0x0400380a88080104, 0x4022441808051820, 0x4010310806114120, 0x0800e44100200000,
    0x0000040504045022, 0x0000020804040030, 0x00000a0101084002, 0x1000208088080200,
    0x0010000410301100, 0x049100041012004a, 0x0810020d004a0043, 0x2008000082024048,
    0x482c902404200008, 0x0202802410088800, 0x0802100c00820890, 0x0800480080441000,
    0x8044212940032c00, 0x8041209004480200, 0x0008020041220200, 0x0018080011220120,
    0x0472840002802020, 0x1002040802008200, 0x3428044f01050840, 0x5124822840820800,
    0x0c08040500522004, 0x0488080424080113, 0x1002005000011100, 0x0002020080080080,
    0x0020008400108020, 0x4002280200204050, 0x0002842102040080, 0x0001020208008040,
    0x0a08040404002083, 0x0000a20120401010, 0x0002020224002a00, 0x004c00c010400a08,
    0x004002204a000300, 0x0002103004810108, 0x4308210828802200, 0x803800810a000852,
    0x0000641004100000, 0x0802808410923000, 0x0020002108088040, 0x001002084202080d,
    0x800200a0020c8000, 0x06600a1001020121, 0x802004410819040a, 0x1410440104002401,
    0x2400840080900800, 0x000324410401a010, 0x0020012100411002, 0x0000120200460800,
    0x4040850020034400, 0x2425002508083840, 0x0c00200244090410, 0x0008010802005200,
];

// next_random·find_magic은 이제 매직을 새로 찾을 때(테스트 print_magics)만 쓴다
#[cfg_attr(not(test), allow(dead_code))]
fn next_random(rng: &mut u64) -> u64 {
    let (s, r) = splitmix64(*rng);
    *rng = s;
    r
}

/// 끝 칸을 뺀 관련 칸
fn rook_mask(sq: Square) -> Bitboard {
    let edges = match (file_of(sq), rank_of(sq)) {
        (0,0) => RANK_8 | FILE_H,
        (0,7) => RANK_1 | FILE_H,
        (7,0) => RANK_8 | FILE_A,
        (7,7) => RANK_1 | FILE_A,
        (0,_) => RANK_1 | RANK_8 | FILE_H,
        (7,_) => RANK_1 | RANK_8 | FILE_A,
        (_,0) => RANK_8 | FILE_A | FILE_H,
        (_,7) => RANK_1 | FILE_A | FILE_H,
        _ => RANK_1 | RANK_8 | FILE_A | FILE_H,
    };

    slow_rook_attacks(sq, 0) & ! edges
}
fn bishop_mask(sq: Square) -> Bitboard {
    let edges = FILE_A | FILE_H | RANK_1 | RANK_8;
    slow_bishop_attacks(sq, 0) & !edges
}

/// 느리지만 확실한 기준 답 (지금의 rook_attacks/bishop_attacks를 이름만 바꿔서)
fn slow_rook_attacks(sq: Square, occ: Bitboard) -> Bitboard{
    let mut b = 0u64;
    let rook_deltas = [(1,0),(0,1),(-1,0),(0,-1)];
    for delta in rook_deltas {
        b |= ray_attacks(sq, delta.0, delta.1, occ);
    }

    b
}
fn slow_bishop_attacks(sq: Square, occ: Bitboard) -> Bitboard{
    let mut b = 0u64;
    let bishop_deltas = [(1,1),(-1,1),(-1,-1,),(1,-1)];
    for delta in bishop_deltas {
        b |= ray_attacks(sq, delta.0, delta.1, occ);
    }

    b
}

/// 이 칸에 맞는 매직을 찾아서, 표 조각(크기 1<<bits)과 함께 돌려줌
#[cfg_attr(not(test), allow(dead_code))]
fn find_magic(sq: Square, mask: Bitboard, slow: fn(Square, Bitboard) -> Bitboard, rng: &mut u64) -> (u64, Vec<Bitboard>) {
    let bits = mask.count_ones();
    let shift = 64 - bits;
    let size = 1<<bits;

    let mut occ = Vec::with_capacity(size);//지금의 마스크
    let mut answer = Vec::with_capacity(size);//정답지
    let mut sub = 0u64;
    loop{
        occ.push(sub);
        answer.push(slow(sq, sub));
        sub = sub.wrapping_sub(mask) & mask;
        if sub == 0 {break}
    }
    debug_assert_eq!(occ.len(), size);

    loop{
        let magic = next_random(rng) & next_random(rng) & next_random(rng);//후보 뽑기
        if (mask.wrapping_mul(magic) >>56).count_ones() < 6 {continue;}

        let mut table = vec![0u64; size];
        let mut used = vec![false; size];
        let mut ok = true;
        for i in 0..size {//후보 검증
            let idx = (occ[i].wrapping_mul(magic) >> shift) as usize;//후보 매직이 골라준 인덱스
            if !used[idx] {//해당 인덱스가 사용된 적이 없는가?
                table[idx] = answer[i];
                used[idx] = true;
            }else if table[idx] != answer[i] {
                ok = false;
                break;
            }
        }

        if ok {
            return (magic, table)
        }
    }
}

/// 이미 아는 매직으로 표 조각을 채운다 (find_magic의 "후보 시험"을 이 매직 하나로 한 번만 하는 것)
/// 매직이 틀렸으면(다른 답이 같은 칸에 들어가면) 패닉
fn fill_table(sq: Square, mask: Bitboard, magic: u64, slow: fn(Square, Bitboard) -> Bitboard) -> Vec<Bitboard> {
    let shift = 64 - mask.count_ones();
    let mut table = vec![0u64; 1 << mask.count_ones()];
    let mut sub = 0u64;
    loop {
        let idx = (sub.wrapping_mul(magic) >> shift) as usize;
        let answer = slow(sq, sub);
        // 슬라이딩 공격은 0일 수 없으므로 0 = 빈 칸
        assert!(table[idx] == 0 || table[idx] == answer, "칸 {}의 매직이 틀림", sq);
        table[idx] = answer;
        sub = sub.wrapping_sub(mask) & mask;
        if sub == 0 { break; }
    }
    table
}

fn build_sliders() -> SliderTables {
    let mut table: Vec<Bitboard> = Vec::new();
    let mut rook = [Magic::default(); 64];
    let mut bishop = [Magic::default(); 64];

    for sq in 0..64 {
        let mask = rook_mask(sq);
        let magic = ROOK_MAGICS[sq as usize];
        rook[sq as usize] = Magic{mask, magic, shift: 64 -mask.count_ones(), offset: table.len()};
        table.extend(fill_table(sq, mask, magic, slow_rook_attacks));
    }

    for sq in 0..64 {
        let mask = bishop_mask(sq);
        let magic = BISHOP_MAGICS[sq as usize];
        bishop[sq as usize] = Magic{mask, magic, shift: 64 -mask.count_ones(), offset: table.len()};
        table.extend(fill_table(sq, mask, magic, slow_bishop_attacks));
    }

    SliderTables { rook, bishop, table }
}

/// 공격 표를 지금 만들어 둔다 (처음 쓸 때 만들면 그 시간이 첫 탐색에 섞이므로, 엔진 시작 시 부른다)
pub fn init_tables() {
    LazyLock::force(&LEAPERS);
    LazyLock::force(&SLIDERS);
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
    LEAPERS.knight[sq as usize]
}
pub fn king_attacks(sq: Square) -> Bitboard {
    LEAPERS.king[sq as usize]
}
/// 폰이 '잡을 수 있는' 대각선 칸 (전진 칸이 아님). 백은 위쪽, 흑은 아래쪽
pub fn pawn_attacks(color: Color, sq: Square) -> Bitboard {
    LEAPERS.pawn[color as usize][sq as usize]
}

pub fn rook_attacks(sq: Square, occ: Bitboard) -> Bitboard {
    let m = SLIDERS.rook[sq as usize];
    let idx = ((occ & m.mask).wrapping_mul(m.magic) >> m.shift) as usize;

    SLIDERS.table[idx + m.offset]
}
pub fn bishop_attacks(sq: Square, occ: Bitboard) -> Bitboard {
    let m = SLIDERS.bishop[sq as usize];
    let idx = ((occ & m.mask).wrapping_mul(m.magic) >> m.shift) as usize;

    SLIDERS.table[idx + m.offset]
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

    /// 매직을 새로 찾아서 ROOK_MAGICS / BISHOP_MAGICS에 붙여 넣을 형식으로 출력한다 (필요할 때만)
    /// cargo test --release print_magics -- --ignored --nocapture
    #[test]
    #[ignore]
    fn print_magics() {
        let mut rng = 0x84cb_49dfu64;
        for (name, mask_fn, slow) in [
            ("ROOK_MAGICS", rook_mask as fn(Square) -> Bitboard, slow_rook_attacks as fn(Square, Bitboard) -> Bitboard),
            ("BISHOP_MAGICS", bishop_mask, slow_bishop_attacks),
        ] {
            println!("const {}: [u64; 64] = [", name);
            for row in 0..16u8 {
                let magics: Vec<String> = (0..4u8)
                    .map(|c| {
                        let s = row * 4 + c;
                        format!("0x{:016x}", find_magic(s, mask_fn(s), slow, &mut rng).0)
                    })
                    .collect();
                println!("    {},", magics.join(", "));
            }
            println!("];");
        }
    }

    #[test]
    fn masks_exclude_edges() {
        assert_eq!(rook_mask(sq("a1")).count_ones(), 12);
        assert_eq!(rook_mask(sq("d4")).count_ones(), 10);
        assert_eq!(bishop_mask(sq("d4")).count_ones(), 9);
        assert_eq!(bishop_mask(sq("a1")).count_ones(), 6);
    }

    #[test]
    fn found_magic_maps_every_occupancy_correctly() {
        // find_magic 하나만 확인: 모든 점유 상태를 표로 조회한 값 == 느린 계산
        let mut rng = 0x1234_5678u64;
        for (name, is_rook) in [("d4", true), ("a1", true), ("d4", false), ("a1", false)] {
            let s = sq(name);
            let (mask, slow): (Bitboard, fn(Square, Bitboard) -> Bitboard) = if is_rook {
                (rook_mask(s), slow_rook_attacks)
            } else {
                (bishop_mask(s), slow_bishop_attacks)
            };
            let (magic, table) = find_magic(s, mask, slow, &mut rng);
            let shift = 64 - mask.count_ones();
            assert_eq!(table.len(), 1 << mask.count_ones());

            let mut sub = 0u64;
            loop {
                let idx = (sub.wrapping_mul(magic) >> shift) as usize;
                assert_eq!(table[idx], slow(s, sub), "{} {} 점유 {:x}",
                           name, if is_rook { "룩" } else { "비숍" }, sub);
                sub = sub.wrapping_sub(mask) & mask;
                if sub == 0 { break; }
            }
        }
    }

    #[test]
    fn magic_matches_slow_for_random_boards() {
        // rook_attacks/bishop_attacks를 표 조회로 바꾼 뒤의 확인용: 64칸 × 무작위 보드 200개
        let mut rng = 0x9abc_def0u64;
        for s in 0..64u8 {
            for _ in 0..200 {
                let occ = next_random(&mut rng) & next_random(&mut rng);   // 기물이 듬성듬성
                assert_eq!(rook_attacks(s, occ), slow_rook_attacks(s, occ), "룩 {} {:x}", s, occ);
                assert_eq!(bishop_attacks(s, occ), slow_bishop_attacks(s, occ), "비숍 {} {:x}", s, occ);
            }
        }
    }
}
