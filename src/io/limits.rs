//! This fork: byte budget and bounded allocation helpers (patch H6).
//!
//! Every size that comes from an untrusted file goes through one of these
//! helpers before memory is reserved. A size above its format limit is a
//! [`DxfError::LimitExceeded`]; a size the caller's [`AllocBudget`] refuses,
//! or one the allocator cannot reserve, is a [`DxfError::BudgetExceeded`].

use std::sync::Arc;

use crate::error::DxfError;

/// Caller-supplied byte budget. The closure receives the number of bytes about
/// to be reserved and returns `false` to refuse the reservation.
#[derive(Clone)]
pub struct AllocBudget(Arc<dyn Fn(usize) -> bool + Send + Sync>);

impl AllocBudget {
    /// Wrap a predicate that decides whether `bytes` more may be reserved.
    pub fn new(f: impl Fn(usize) -> bool + Send + Sync + 'static) -> Self {
        Self(Arc::new(f))
    }

    /// Ask the budget whether `bytes` more may be reserved.
    pub fn allows(&self, bytes: usize) -> bool {
        (self.0)(bytes)
    }
}

impl core::fmt::Debug for AllocBudget {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str("AllocBudget(..)")
    }
}

/// Largest decompressed AC18 page or system page accepted (typical: 0x7400).
pub const AC18_MAX_PAGE_DECOMPRESSED: usize = 16 << 20;
/// Largest AC21 Reed-Solomon page buffer accepted (upstream's existing limit).
pub const AC21_MAX_PAGE_BUFFER: usize = 100_000_000;
/// A decompressed section may be at most this many times the file size
/// (measured 2.6x to 5.1x on real drawings).
pub const SECTION_EXPANSION_FACTOR: u64 = 64;
/// Lower bound of the per-section cap, so small files keep a generous limit.
pub const SECTION_MIN_CAP: u64 = 64 << 20;

/// `vec![0u8; len]` that checks `len <= cap` and the budget first and reports
/// allocation failure as an error instead of aborting.
pub(crate) fn zeroed_vec(
    len: usize,
    cap: usize,
    budget: Option<&AllocBudget>,
    what: &'static str,
) -> Result<Vec<u8>, DxfError> {
    if len > cap {
        return Err(DxfError::LimitExceeded {
            what,
            value: len as u64,
            limit: cap as u64,
        });
    }
    if let Some(budget) = budget {
        if !budget.allows(len) {
            return Err(DxfError::BudgetExceeded {
                what,
                requested: len as u64,
            });
        }
    }
    let mut out = Vec::new();
    out.try_reserve_exact(len)
        .map_err(|_| DxfError::BudgetExceeded {
            what,
            requested: len as u64,
        })?;
    out.resize(len, 0);
    Ok(out)
}

/// `v.reserve(additional)` that checks the element cap and the budget first.
pub(crate) fn reserve_vec<T>(
    v: &mut Vec<T>,
    additional: usize,
    cap_elems: usize,
    budget: Option<&AllocBudget>,
    what: &'static str,
) -> Result<(), DxfError> {
    let wanted = v.len().saturating_add(additional);
    if wanted > cap_elems {
        return Err(DxfError::LimitExceeded {
            what,
            value: wanted as u64,
            limit: cap_elems as u64,
        });
    }
    let bytes = additional
        .checked_mul(std::mem::size_of::<T>())
        .ok_or(DxfError::BudgetExceeded {
            what,
            requested: u64::MAX,
        })?;
    if let Some(budget) = budget {
        if !budget.allows(bytes) {
            return Err(DxfError::BudgetExceeded {
                what,
                requested: bytes as u64,
            });
        }
    }
    v.try_reserve(additional)
        .map_err(|_| DxfError::BudgetExceeded {
            what,
            requested: bytes as u64,
        })
}

/// Upper bound for one decompressed section: `max(file_len * 64, 64 MiB)`.
pub(crate) fn section_cap(file_len: u64) -> u64 {
    file_len
        .saturating_mul(SECTION_EXPANSION_FACTOR)
        .max(SECTION_MIN_CAP)
}

#[cfg(test)]
mod hardening_tests {
    use super::*;
    use std::alloc::{GlobalAlloc, Layout, System};
    use std::cell::Cell;

    /// Records the largest single allocation request made by the current
    /// thread, so concurrent unit tests cannot disturb the measurement.
    struct ThreadMaxAlloc;

    thread_local! {
        static MAX_REQUEST: Cell<usize> = const { Cell::new(0) };
    }

    fn note(size: usize) {
        let _ = MAX_REQUEST.try_with(|max| {
            if size > max.get() {
                max.set(size);
            }
        });
    }

    unsafe impl GlobalAlloc for ThreadMaxAlloc {
        unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
            note(layout.size());
            System.alloc(layout)
        }
        unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
            note(layout.size());
            System.alloc_zeroed(layout)
        }
        unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
            System.dealloc(ptr, layout)
        }
        unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
            note(new_size);
            System.realloc(ptr, layout, new_size)
        }
    }

    #[global_allocator]
    static PROBE: ThreadMaxAlloc = ThreadMaxAlloc;

    fn largest_request_during<R>(f: impl FnOnce() -> R) -> (R, usize) {
        MAX_REQUEST.with(|max| max.set(0));
        let out = f();
        (out, MAX_REQUEST.with(|max| max.get()))
    }

    #[test]
    fn zeroed_vec_rejects_lengths_above_the_cap() {
        let err = zeroed_vec(10 << 20, 1 << 20, None, "t").unwrap_err();
        assert!(
            matches!(
                err,
                DxfError::LimitExceeded {
                    what: "t",
                    value: 10_485_760,
                    limit: 1_048_576
                }
            ),
            "{err:?}"
        );
    }

    #[test]
    fn zeroed_vec_asks_the_budget_before_allocating() {
        let budget = AllocBudget::new(|bytes| bytes <= 1 << 20);
        let (result, largest) =
            largest_request_during(|| zeroed_vec(2 << 20, 1 << 30, Some(&budget), "t"));
        assert!(matches!(
            result,
            Err(DxfError::BudgetExceeded {
                what: "t",
                requested: 2_097_152
            })
        ));
        assert!(largest < 2 << 20, "a {largest}-byte request reached the allocator");
    }

    #[test]
    fn zeroed_vec_reports_capacity_overflow_without_panicking() {
        let result = std::panic::catch_unwind(|| zeroed_vec(usize::MAX, usize::MAX, None, "t"));
        let result = result.expect("zeroed_vec must not panic");
        assert!(matches!(result, Err(DxfError::BudgetExceeded { .. })));
    }

    #[test]
    fn zeroed_vec_returns_zeroed_memory_within_limits() {
        let v = zeroed_vec(4096, 4096, Some(&AllocBudget::new(|_| true)), "t").unwrap();
        assert_eq!(v.len(), 4096);
        assert!(v.iter().all(|&b| b == 0));
    }

    #[test]
    fn reserve_vec_rejects_element_counts_above_the_cap() {
        let mut v: Vec<u64> = Vec::new();
        let err = reserve_vec(&mut v, 1 << 40, 1 << 20, None, "t").unwrap_err();
        assert!(matches!(err, DxfError::LimitExceeded { what: "t", .. }), "{err:?}");
        assert_eq!(v.capacity(), 0);
    }

    #[test]
    fn reserve_vec_checks_the_budget_in_bytes() {
        let mut v: Vec<u64> = Vec::new();
        let budget = AllocBudget::new(|bytes| bytes <= 1024);
        assert!(reserve_vec(&mut v, 128, 1 << 20, Some(&budget), "t").is_ok());
        let err = reserve_vec(&mut v, 129, 1 << 20, Some(&budget), "t").unwrap_err();
        assert!(matches!(
            err,
            DxfError::BudgetExceeded {
                what: "t",
                requested: 1032
            }
        ));
    }

    #[test]
    fn section_cap_has_a_floor_and_scales_with_the_file() {
        assert_eq!(section_cap(0), 64 << 20);
        assert_eq!(section_cap(1 << 20), 64 << 20);
        assert_eq!(section_cap(10 << 20), 640 << 20);
        assert_eq!(section_cap(u64::MAX), u64::MAX);
    }
}
