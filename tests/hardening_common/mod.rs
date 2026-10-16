//! Shared helpers for this fork's regression tests.
//!
//! Every `tests/hardening_*.rs` binary includes this module with
//! `mod hardening_common;`. It installs [`MaxAllocProbe`] as the binary's global
//! allocator, so every test in such a binary must start with
//! `let _guard = serial();` to keep the measurement to one test at a time.
#![allow(dead_code)]

use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Mutex, MutexGuard};

/// Global allocator that records the largest single allocation request.
pub struct MaxAllocProbe;

static MAX_REQUEST: AtomicUsize = AtomicUsize::new(0);

unsafe impl GlobalAlloc for MaxAllocProbe {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        MAX_REQUEST.fetch_max(layout.size(), Ordering::Relaxed);
        System.alloc(layout)
    }
    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        MAX_REQUEST.fetch_max(layout.size(), Ordering::Relaxed);
        System.alloc_zeroed(layout)
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        System.dealloc(ptr, layout)
    }
    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        MAX_REQUEST.fetch_max(new_size, Ordering::Relaxed);
        System.realloc(ptr, layout, new_size)
    }
}

#[global_allocator]
static PROBE: MaxAllocProbe = MaxAllocProbe;

static SERIAL: Mutex<()> = Mutex::new(());

/// Serialises the tests of one binary (poison-tolerant).
pub fn serial() -> MutexGuard<'static, ()> {
    SERIAL.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// Runs `f` and returns its result together with the largest allocation
/// request any thread made meanwhile. Call it while holding [`serial`].
pub fn largest_alloc_during<R>(f: impl FnOnce() -> R) -> (R, usize) {
    MAX_REQUEST.store(0, Ordering::SeqCst);
    let out = f();
    (out, MAX_REQUEST.load(Ordering::SeqCst))
}

/// Runs `f`; a panic escaping it fails the test with `label`.
pub fn no_panic<R>(label: &str, f: impl FnOnce() -> R) -> R {
    match std::panic::catch_unwind(std::panic::AssertUnwindSafe(f)) {
        Ok(value) => value,
        Err(_) => panic!("panic escaped: {label}"),
    }
}

/// Deterministic xorshift64* generator, so sweeps need no extra crates.
pub struct XorShift(u64);

impl XorShift {
    pub fn new(seed: u64) -> Self {
        Self(seed.max(1))
    }

    pub fn next_u64(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.0 = x;
        x.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }

    /// Uniform-enough value in `0..n` (`n > 0`).
    pub fn below(&mut self, n: usize) -> usize {
        (self.next_u64() % n as u64) as usize
    }

    pub fn bytes(&mut self, len: usize) -> Vec<u8> {
        (0..len).map(|_| self.next_u64() as u8).collect()
    }
}

/// `count` copies of `bytes`, each with one byte changed, from a fixed seed.
/// Yields `(position, mutated_bytes)`.
pub fn single_byte_mutations(
    bytes: &[u8],
    count: usize,
    seed: u64,
) -> impl Iterator<Item = (usize, Vec<u8>)> + '_ {
    let mut rng = XorShift::new(seed);
    (0..count).map(move |_| {
        let position = rng.below(bytes.len());
        let flip = 1 + rng.below(255) as u8;
        let mut mutated = bytes.to_vec();
        mutated[position] ^= flip;
        (position, mutated)
    })
}
