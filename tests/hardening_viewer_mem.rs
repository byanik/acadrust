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

use acadrust::entities::{EntityType, Viewport};
use acadrust::io::limits::ViewerOptions;
use acadrust::objects::{
    KnownXRecordKind, ObjectType, ProxyObject, ProxyPayload, Scale, XRecordEntry, XRecordValue,
};
use acadrust::types::Handle;
use acadrust::DwgReadOptions;

fn read_with(bytes: &[u8], viewer: ViewerOptions) -> CadDocument {
    DwgReader::from_stream_with_options(
        Cursor::new(bytes.to_vec()),
        DwgReadOptions { failsafe: true, viewer, ..Default::default() },
    )
    .read()
    .expect("read dwg")
}

#[test]
fn f2_viewer_mode_drops_bulky_xrecords_but_keeps_scales_and_overrides() {
    let _g = lock();
    let mut doc = CadDocument::with_version(DxfVersion::AC1032);
    let vp = doc.add_paper_space_entity(EntityType::Viewport(Box::new(Viewport::new()))).expect("viewport");
    let scale_h = doc.allocate_handle();
    let mut scale = Scale::new("1:50", 1.0, 50.0);
    scale.handle = scale_h;
    doc.objects.insert(scale_h, ObjectType::Scale(scale));
    doc.set_viewport_annotation_scale(vp, scale_h);
    let layer_h = doc.layers.get("0").expect("layer 0").handle;
    assert!(doc.set_layer_viewport_override(
        layer_h,
        KnownXRecordKind::LayerViewportColorOverride,
        vp,
        XRecordValue::Int32(0x00FF_0000),
    ));
    let junk = doc.ensure_xrecord(layer_h, "HARDENING_JUNK");
    if let Some(ObjectType::XRecord(x)) = doc.objects.get_mut(&junk) {
        for _ in 0..40 {
            x.entries.push(XRecordEntry::string(1, "x".repeat(256)));
        }
    }
    let proxy_h = doc.allocate_handle();
    doc.objects.insert(
        proxy_h,
        ObjectType::ProxyObject(ProxyObject {
            handle: proxy_h,
            owner: Handle::NULL,
            proxy_id: 499,
            class_id: 501,
            dwg_version: 31,
            maintenance_version: 106,
            payload: ProxyPayload::from_bits(&[0xAB, 0xC0], 10),
            ..Default::default()
        }),
    );
    let bytes = DwgWriter::write_to_vec(&doc).expect("write dwg");
    let plain = read_with(&bytes, ViewerOptions::default());
    let viewer = read_with(
        &bytes,
        ViewerOptions { skip_xrecords_over: Some(1024), skip_proxy_objects: true, ..Default::default() },
    );
    assert!(matches!(plain.objects.get(&proxy_h), Some(ObjectType::ProxyObject(_))), "proxy object present without the option");
    assert!(viewer.objects.get(&proxy_h).is_none(), "proxy object skipped with the option");
    assert!(plain.xrecord(layer_h, "HARDENING_JUNK").is_some(), "10 KiB record present without the option");
    assert!(viewer.xrecord(layer_h, "HARDENING_JUNK").is_none(), "10 KiB record skipped with the option");
    assert_eq!(viewer.viewport_annotation_scale(vp), plain.viewport_annotation_scale(vp));
    let overrides = |d: &CadDocument| d.layer_viewport_overrides(layer_h, KnownXRecordKind::LayerViewportColorOverride);
    assert!(!overrides(&plain).is_empty());
    assert_eq!(overrides(&viewer), overrides(&plain));
    assert!(!viewer.objects.values().any(|o| matches!(o, ObjectType::ProxyObject(_))));
}
