//! Fork patches F1 (boxed ParsedEntry), F2' (viewer XRecord/ProxyObject skipping) and
//! F13 (drop raw_record). A local allocator records the largest single allocation request.

use std::alloc::{GlobalAlloc, Layout, System};
use std::io::Cursor;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Mutex, MutexGuard, PoisonError};

use acadrust::tables::AppId;
use acadrust::types::DxfVersion;
use acadrust::{CadDocument, DwgReader, DwgWriter};

struct MaxProbe;

static MAX: AtomicUsize = AtomicUsize::new(0);

unsafe impl GlobalAlloc for MaxProbe {
    unsafe fn alloc(&self, l: Layout) -> *mut u8 {
        MAX.fetch_max(l.size(), Ordering::Relaxed);
        unsafe { System.alloc(l) }
    }
    unsafe fn alloc_zeroed(&self, l: Layout) -> *mut u8 {
        MAX.fetch_max(l.size(), Ordering::Relaxed);
        unsafe { System.alloc_zeroed(l) }
    }
    unsafe fn realloc(&self, p: *mut u8, l: Layout, new: usize) -> *mut u8 {
        MAX.fetch_max(new, Ordering::Relaxed);
        unsafe { System.realloc(p, l, new) }
    }
    unsafe fn dealloc(&self, p: *mut u8, l: Layout) {
        unsafe { System.dealloc(p, l) }
    }
}

#[global_allocator]
static ALLOC: MaxProbe = MaxProbe;

static LOCK: Mutex<()> = Mutex::new(());

fn lock() -> MutexGuard<'static, ()> {
    LOCK.lock().unwrap_or_else(PoisonError::into_inner)
}

#[test]
fn f1_appid_heavy_read_has_no_huge_single_allocation() {
    let _g = lock();
    let mut doc = CadDocument::with_version(DxfVersion::AC1032);
    for i in 0..120_000 {
        let _ = doc.app_ids.add(AppId::new(format!("CYF_APP_{i}")));
    }
    let bytes = DwgWriter::write_to_vec(&doc).expect("write dwg");
    MAX.store(0, Ordering::SeqCst);
    let back = DwgReader::from_stream(Cursor::new(bytes)).read().expect("read dwg");
    let max = MAX.load(Ordering::SeqCst);
    assert!(back.app_ids.len() >= 120_000);
    // Unboxed, `parsed_entries` alone is one ≥ 120 000 × 568 B (≈ 68 MB) buffer; boxed it is ≈ 6 MB.
    assert!(max < 120_000 * 568 / 2, "largest single allocation was {max} B");
}
