//! 邮件状态位图：一次 UPDATE 即可改多个状态。

use serde::{Deserialize, Serialize};

pub const FLAG_SEEN: i64 = 1 << 0;
pub const FLAG_ANSWERED: i64 = 1 << 1;
pub const FLAG_FLAGGED: i64 = 1 << 2;
pub const FLAG_DRAFT: i64 = 1 << 3;
pub const FLAG_DELETED: i64 = 1 << 4;

/// 邮件 flags 的位图封装。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Flags(pub i64);

impl Flags {
    pub const fn new(bits: i64) -> Self {
        Self(bits)
    }

    pub const fn has(self, flag: i64) -> bool {
        self.0 & flag != 0
    }

    pub fn set(&mut self, flag: i64) {
        self.0 |= flag;
    }

    pub fn clear(&mut self, flag: i64) {
        self.0 &= !flag;
    }

    pub fn toggle(&mut self, flag: i64) {
        self.0 ^= flag;
    }

    pub const fn is_seen(self) -> bool {
        self.has(FLAG_SEEN)
    }
    pub const fn is_starred(self) -> bool {
        self.has(FLAG_FLAGGED)
    }
    pub const fn is_draft(self) -> bool {
        self.has(FLAG_DRAFT)
    }
    pub const fn is_deleted(self) -> bool {
        self.has(FLAG_DELETED)
    }

    pub const fn bits(self) -> i64 {
        self.0
    }
}

impl From<i64> for Flags {
    fn from(bits: i64) -> Self {
        Self(bits)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flags_set_clear_toggle() {
        let mut f = Flags::default();
        assert!(!f.is_seen());
        f.set(FLAG_SEEN);
        assert!(f.is_seen());
        f.set(FLAG_FLAGGED);
        assert!(f.is_starred());
        assert_eq!(f.bits(), FLAG_SEEN | FLAG_FLAGGED);
        f.clear(FLAG_SEEN);
        assert!(!f.is_seen());
        assert!(f.is_starred());
        f.toggle(FLAG_FLAGGED);
        assert!(!f.is_starred());
    }

    #[test]
    fn flags_independent() {
        let mut f = Flags::new(FLAG_SEEN);
        f.set(FLAG_ANSWERED);
        assert!(f.is_seen() && f.has(FLAG_ANSWERED) && !f.is_draft());
    }
}
