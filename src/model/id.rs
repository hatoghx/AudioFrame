use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicU64, Ordering};


#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Id(pub u64);

static NEXT: AtomicU64 = AtomicU64::new(1);

impl Id {
    pub fn new() -> Self {
        Id(NEXT.fetch_add(1, Ordering::Relaxed))
    }

    
    pub fn bump_past(observed: Id) {
        let want = observed.0.saturating_add(1);
        let mut cur = NEXT.load(Ordering::Relaxed);
        while cur < want {
            match NEXT.compare_exchange_weak(cur, want, Ordering::Relaxed, Ordering::Relaxed) {
                Ok(_) => break,
                Err(actual) => cur = actual,
            }
        }
    }
}

impl Default for Id {
    fn default() -> Self {
        Self::new()
    }
}
