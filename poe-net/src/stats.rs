//! Process-wide byte counters of every [`crate::Connection`] (TCP payload as read from and
//! written to the socket). The server's metrics sampler turns them into rates.

use std::sync::atomic::{AtomicU64, Ordering};

static BYTES_IN: AtomicU64 = AtomicU64::new(0);
static BYTES_OUT: AtomicU64 = AtomicU64::new(0);

/// Bytes read from all connections since the process started.
pub fn bytes_in() -> u64 {
    BYTES_IN.load(Ordering::Relaxed)
}

/// Bytes written to all connections since the process started.
pub fn bytes_out() -> u64 {
    BYTES_OUT.load(Ordering::Relaxed)
}

pub fn add_in(n: usize) {
    BYTES_IN.fetch_add(n as u64, Ordering::Relaxed);
}

pub fn add_out(n: usize) {
    BYTES_OUT.fetch_add(n as u64, Ordering::Relaxed);
}

#[cfg(test)]
mod tests {
    #[test]
    fn counters_add_up() {
        let (i, o) = (super::bytes_in(), super::bytes_out());
        super::add_in(5);
        super::add_out(7);
        assert!(super::bytes_in() >= i + 5);
        assert!(super::bytes_out() >= o + 7);
    }
}
