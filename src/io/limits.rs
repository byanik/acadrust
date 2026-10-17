//! This fork: byte budget and bounded allocation helpers (patch H6) and
//! the cancellation token (patch C1).
//!
//! Every size that comes from an untrusted file goes through one of these
//! helpers before memory is reserved. A size above its format limit is a
//! [`DxfError::LimitExceeded`]; a size the caller's [`AllocBudget`] refuses,
//! or one the allocator cannot reserve, is a [`DxfError::BudgetExceeded`].

use std::sync::atomic::{AtomicBool, Ordering};
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

/// Caller-supplied cancellation check, polled at stage, page and record-batch
/// boundaries. Once it returns `true` the read ends with
/// [`DxfError::Cancelled`], even in failsafe mode.
#[derive(Clone)]
pub struct CancelToken(Arc<dyn Fn() -> bool + Send + Sync>);

impl CancelToken {
    /// Wrap a predicate that returns `true` once the read must stop.
    pub fn new(f: impl Fn() -> bool + Send + Sync + 'static) -> Self {
        Self(Arc::new(f))
    }

    /// Token that fires when `flag` becomes `true`.
    pub fn from_flag(flag: Arc<AtomicBool>) -> Self {
        Self::new(move || flag.load(Ordering::Relaxed))
    }

    /// Ask whether the read must stop now.
    pub fn is_cancelled(&self) -> bool {
        (self.0)()
    }
}

impl core::fmt::Debug for CancelToken {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str("CancelToken(..)")
    }
}

/// `Err(Cancelled)` when the token has fired.
pub(crate) fn check_cancel(token: Option<&CancelToken>) -> Result<(), DxfError> {
    match token {
        Some(token) if token.is_cancelled() => Err(DxfError::Cancelled),
        _ => Ok(()),
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

/// Viewer-only memory options (fork patches F2/F13). Defaults keep upstream behaviour.
#[derive(Clone, Debug, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct ViewerOptions {
    /// Do not store XRecords whose payload is larger than this many bytes. Small XRecords stay,
    /// because `viewport_annotation_scale` and `layer_viewport_overrides` read them.
    pub skip_xrecords_over: Option<usize>,
    /// Do not store ProxyObject (non-graphical proxy) objects.
    pub skip_proxy_objects: bool,
    /// Do not keep `EntityCommon::raw_record` (verbatim record bytes used only for write
    /// passthrough). `graphic_data` is kept: the proxy-graphics fallback needs it.
    pub drop_raw_records: bool,
}

/// Approximate payload of an XRecord in bytes (strings and chunks by length, scalars by width).
/// A record whose payload the reader could not decode completely (`entries_complete` is false)
/// keeps the rest only in `raw_data`, so that length counts too.
pub(crate) fn xrecord_payload_bytes(x: &crate::objects::XRecord) -> usize {
    use crate::objects::XRecordValue as V;
    let decoded: usize = x
        .entries
        .iter()
        .map(|e| {
            4 + match &e.value {
                V::String(s) => s.len(),
                V::Chunk(c) => c.len(),
                V::Point3D(..) => 24,
                V::Double(_) | V::Int64(_) | V::Handle(_) => 8,
                V::Int32(_) => 4,
                V::Int16(_) => 2,
                V::Byte(_) | V::Bool(_) => 1,
            }
        })
        .sum();
    if x.entries_complete {
        decoded
    } else {
        decoded.max(x.raw_data.len())
    }
}

impl ViewerOptions {
    pub(crate) fn keeps(&self, o: &crate::objects::ObjectType) -> bool {
        use crate::objects::ObjectType;
        match o {
            ObjectType::ProxyObject(_) => !self.skip_proxy_objects,
            ObjectType::XRecord(x) => self.skip_xrecords_over.is_none_or(|max| xrecord_payload_bytes(x) <= max),
            _ => true,
        }
    }
}

#[cfg(test)]
pub(crate) mod hardening_tests {
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

    /// Largest allocation request made by this thread while `f` ran.
    pub(crate) fn largest_request_during<R>(f: impl FnOnce() -> R) -> (R, usize) {
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

    fn record_of(entries: Vec<crate::objects::XRecordEntry>) -> crate::objects::XRecord {
        let mut x = crate::objects::XRecord::new();
        x.entries = entries;
        x
    }

    #[test]
    fn xrecord_payload_counts_four_bytes_per_entry_plus_the_value_width() {
        use crate::objects::XRecordEntry as E;
        let x = record_of(vec![
            E::string(1, "abcde"),
            E::new(310, crate::objects::XRecordValue::Chunk(vec![0; 7])),
            E::point3d(10, 1.0, 2.0, 3.0),
            E::double(40, 1.0),
            E::new(160, crate::objects::XRecordValue::Int64(1)),
            E::handle(330, crate::types::Handle::from(5u64)),
            E::int32(90, 1),
            E::int16(70, 1),
            E::new(280, crate::objects::XRecordValue::Byte(1)),
            E::bool(290, true),
        ]);
        let values = 5 + 7 + 24 + 8 + 8 + 8 + 4 + 2 + 1 + 1;
        assert_eq!(xrecord_payload_bytes(&x), 10 * 4 + values);
        assert_eq!(xrecord_payload_bytes(&crate::objects::XRecord::new()), 0);
    }

    #[test]
    fn an_incompletely_decoded_xrecord_counts_its_raw_bytes() {
        let mut x = record_of(vec![crate::objects::XRecordEntry::int16(70, 1)]);
        x.raw_data = vec![0; 5000];
        assert_eq!(xrecord_payload_bytes(&x), 6, "complete entries: the raw copy is not counted");
        x.entries_complete = false;
        assert_eq!(xrecord_payload_bytes(&x), 5000);
        x.raw_data.clear();
        assert_eq!(xrecord_payload_bytes(&x), 6);
    }

    #[test]
    fn the_viewer_options_keep_everything_by_default() {
        use crate::objects::{ObjectType, ProxyObject};
        let big = record_of(vec![crate::objects::XRecordEntry::string(1, "x".repeat(1 << 20))]);
        let v = ViewerOptions::default();
        assert!(v.keeps(&ObjectType::XRecord(big)));
        assert!(v.keeps(&ObjectType::ProxyObject(ProxyObject::default())));
    }

    #[test]
    fn the_xrecord_threshold_keeps_exactly_the_records_up_to_it() {
        use crate::objects::{ObjectType, XRecordEntry as E};
        let v = ViewerOptions {
            skip_xrecords_over: Some(1024),
            ..Default::default()
        };
        // One string entry weighs 4 + length bytes.
        let at = record_of(vec![E::string(1, "x".repeat(1020))]);
        let over = record_of(vec![E::string(1, "x".repeat(1021))]);
        assert_eq!(xrecord_payload_bytes(&at), 1024);
        assert_eq!(xrecord_payload_bytes(&over), 1025);
        assert!(v.keeps(&ObjectType::XRecord(at)));
        assert!(!v.keeps(&ObjectType::XRecord(over)));
        let mut hidden = record_of(vec![E::int16(70, 1)]);
        hidden.entries_complete = false;
        hidden.raw_data = vec![0; 4096];
        assert!(!v.keeps(&ObjectType::XRecord(hidden)));
    }

    #[test]
    fn proxy_objects_are_dropped_only_when_asked_and_other_objects_are_never_dropped() {
        use crate::objects::{ObjectType, ProxyObject, Scale};
        let skip = ViewerOptions {
            skip_proxy_objects: true,
            skip_xrecords_over: Some(0),
            ..Default::default()
        };
        assert!(!skip.keeps(&ObjectType::ProxyObject(ProxyObject::default())));
        assert!(skip.keeps(&ObjectType::Scale(Scale::new("1:1", 1.0, 1.0))));
        let keep = ViewerOptions {
            skip_xrecords_over: Some(0),
            ..Default::default()
        };
        assert!(keep.keeps(&ObjectType::ProxyObject(ProxyObject::default())));
    }
}
