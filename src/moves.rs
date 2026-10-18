use crate::types::*;

// flag 상수
pub const QUIET: u16 = 0;
pub const DOUBLE_PUSH: u16 = 1;
pub const KING_CASTLE: u16 = 2;
pub const QUEEN_CASTLE: u16 = 3;
pub const CAPTURE: u16 = 4;
pub const EP_CAPTURE: u16 = 5;
pub const PROMO_N: u16 = 8;
pub const PROMO_B: u16 = 9;
pub const PROMO_R: u16 = 10;
pub const PROMO_Q: u16 = 11;
pub const PROMO_CAPTURE_N: u16 = 12;
pub const PROMO_CAPTURE_B: u16 = 13;
pub const PROMO_CAPTURE_R: u16 = 14;
pub const PROMO_CAPTURE_Q: u16 = 15;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Move(u16);

impl Move {
    pub fn new(from: Square, to: Square, flag: u16) -> Move {
        Move(flag << 12 | (to as u16) << 6 | from as u16)
    }

    pub fn from(self) -> Square { (self.0 & 0b_111_111) as u8 }
    pub fn to(self) -> Square { (self.0 >> 6 & 0b_111_111) as u8 }
    pub fn flag(self) -> u16 { self.0 >> 12 }

    pub fn is_capture(self) -> bool { self.flag() & 0b_0100 != 0 }
    pub fn is_promotion(self) -> bool { self.flag() & 0b_1000 != 0 }
    /// 승격 수면 승격 기물, 아니면 None
    pub fn promotion_piece(self) -> Option<Piece> {
        if self.is_promotion() {
            match self.flag() & 0b_11 {
                0 => Some(Piece::Knight),
                1 => Some(Piece::Bishop),
                2 => Some(Piece::Rook),
                3 => Some(Piece::Queen),
                _ => unreachable!("마스킹을 했으므로 나올 수 없는 경우의 수"),
            }
        }else{
            None
        }
    }

    /// UCI 표기: "e2e4", 승격은 "e7e8q" (승격 기물은 항상 소문자)
    pub fn to_uci(self) -> String {
        let from_sq = square_name(self.from());
        let to_sq = square_name(self.to());
        if let Some(promo_piece) = self.promotion_piece(){
            format!("{}{}{}", from_sq, to_sq, piece_to_char(Color::Black, promo_piece))
        }else{
            format!("{}{}", from_sq, to_sq)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sq(name: &str) -> Square {
        parse_square(name).unwrap()
    }

    #[test]
    fn encode_decode() {
        let m = Move::new(sq("e2"), sq("e4"), DOUBLE_PUSH);
        assert_eq!(m.from(), sq("e2"));
        assert_eq!(m.to(), sq("e4"));
        assert_eq!(m.flag(), DOUBLE_PUSH);
        // 모든 칸과 flag 조합이 섞이지 않고 되돌아와야 함
        for from in 0..64 {
            for to in 0..64 {
                for flag in 0..16 {
                    let m = Move::new(from, to, flag);
                    assert_eq!((m.from(), m.to(), m.flag()), (from, to, flag));
                }
            }
        }
    }

    #[test]
    fn kinds() {
        assert!(!Move::new(0, 1, QUIET).is_capture());
        assert!(Move::new(0, 1, CAPTURE).is_capture());
        assert!(Move::new(0, 1, EP_CAPTURE).is_capture());
        assert!(Move::new(0, 1, PROMO_CAPTURE_Q).is_capture());
        assert!(!Move::new(0, 1, PROMO_Q).is_capture());
        assert!(!Move::new(0, 1, KING_CASTLE).is_capture());

        assert!(Move::new(0, 1, PROMO_N).is_promotion());
        assert!(Move::new(0, 1, PROMO_CAPTURE_R).is_promotion());
        assert!(!Move::new(0, 1, CAPTURE).is_promotion());
        assert!(!Move::new(0, 1, DOUBLE_PUSH).is_promotion());
    }

    #[test]
    fn promotion_pieces() {
        assert_eq!(Move::new(0, 1, PROMO_N).promotion_piece(), Some(Piece::Knight));
        assert_eq!(Move::new(0, 1, PROMO_B).promotion_piece(), Some(Piece::Bishop));
        assert_eq!(Move::new(0, 1, PROMO_R).promotion_piece(), Some(Piece::Rook));
        assert_eq!(Move::new(0, 1, PROMO_Q).promotion_piece(), Some(Piece::Queen));
        assert_eq!(Move::new(0, 1, PROMO_CAPTURE_N).promotion_piece(), Some(Piece::Knight));
        assert_eq!(Move::new(0, 1, PROMO_CAPTURE_Q).promotion_piece(), Some(Piece::Queen));
        assert_eq!(Move::new(0, 1, QUIET).promotion_piece(), None);
        assert_eq!(Move::new(0, 1, CAPTURE).promotion_piece(), None);
    }

    #[test]
    fn uci() {
        assert_eq!(Move::new(sq("e2"), sq("e4"), DOUBLE_PUSH).to_uci(), "e2e4");
        assert_eq!(Move::new(sq("e1"), sq("g1"), KING_CASTLE).to_uci(), "e1g1");
        assert_eq!(Move::new(sq("e7"), sq("e8"), PROMO_Q).to_uci(), "e7e8q");
        assert_eq!(Move::new(sq("b2"), sq("a1"), PROMO_CAPTURE_N).to_uci(), "b2a1n");
    }
}