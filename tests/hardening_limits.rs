//! H6: byte budget, bounded allocation limits and the new error variants.
mod hardening_common;

use std::io::Cursor;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use acadrust::entities::{EntityType, Line};
use acadrust::io::limits::{
    AllocBudget, AC18_MAX_PAGE_DECOMPRESSED, AC21_MAX_PAGE_BUFFER, SECTION_EXPANSION_FACTOR,
    SECTION_MIN_CAP,
};
use acadrust::types::Vector3;
use acadrust::{CadDocument, DwgReadOptions, DwgReader, DwgWriter, DxfError, DxfVersion};
use hardening_common::serial;

#[test]
fn alloc_budget_delegates_and_hides_its_closure_in_debug() {
    let _guard = serial();
    let calls = Arc::new(AtomicUsize::new(0));
    let seen = Arc::clone(&calls);
    let budget = AllocBudget::new(move |bytes| {
        seen.fetch_add(1, Ordering::SeqCst);
        bytes <= 100
    });
    assert!(budget.allows(100));
    assert!(!budget.allows(101));
    let clone = budget.clone();
    assert!(clone.allows(1));
    assert_eq!(calls.load(Ordering::SeqCst), 3);
    assert_eq!(format!("{budget:?}"), "AllocBudget(..)");
}

#[test]
fn read_options_default_to_no_budget() {
    let _guard = serial();
    let default = DwgReadOptions::default();
    assert!(!default.failsafe);
    assert!(default.budget.is_none());
    let failsafe = DwgReadOptions::failsafe();
    assert!(failsafe.failsafe);
    assert!(failsafe.budget.is_none());
    let budgeted = DwgReadOptions {
        failsafe: true,
        budget: Some(AllocBudget::new(|_| true)),
        ..DwgReadOptions::default()
    };
    assert!(format!("{budgeted:?}").contains("AllocBudget(..)"));
}

#[test]
fn limit_constants_match_the_design() {
    let _guard = serial();
    assert_eq!(AC18_MAX_PAGE_DECOMPRESSED, 16 << 20);
    assert_eq!(AC21_MAX_PAGE_BUFFER, 100_000_000);
    assert_eq!(SECTION_EXPANSION_FACTOR, 64);
    assert_eq!(SECTION_MIN_CAP, 64 << 20);
}

#[test]
fn new_errors_display_and_classify() {
    let _guard = serial();
    assert_eq!(DxfError::Cancelled.to_string(), "Read cancelled");
    let budget = DxfError::BudgetExceeded {
        what: "page",
        requested: 7,
    };
    assert_eq!(
        budget.to_string(),
        "Allocation budget exceeded: page requested 7 bytes"
    );
    let limit = DxfError::LimitExceeded {
        what: "page",
        value: 9,
        limit: 8,
    };
    assert_eq!(limit.to_string(), "Limit exceeded: page is 9, limit 8");
    assert!(DxfError::Cancelled.is_resource_stop());
    assert!(budget.is_resource_stop());
    assert!(!limit.is_resource_stop());
    assert!(!DxfError::Parse("x".into()).is_resource_stop());
}

#[test]
fn a_permissive_budget_reads_like_no_budget() {
    let _guard = serial();
    let mut doc = CadDocument::with_version(DxfVersion::AC1018);
    for i in 0..10 {
        let x = i as f64;
        doc.add_entity(EntityType::Line(Line::from_points(
            Vector3::new(x, 0.0, 0.0),
            Vector3::new(x, 10.0, 0.0),
        )))
        .unwrap();
    }
    let bytes = DwgWriter::write_to_vec(&doc).unwrap();
    let plain = DwgReader::from_stream_with_options(Cursor::new(bytes.clone()), DwgReadOptions::failsafe())
        .read()
        .unwrap();
    let options = DwgReadOptions {
        failsafe: true,
        budget: Some(AllocBudget::new(|_| true)),
        ..DwgReadOptions::default()
    };
    let budgeted = DwgReader::from_stream_with_options(Cursor::new(bytes), options)
        .read()
        .unwrap();
    assert_eq!(plain.entities().count(), budgeted.entities().count());
    let lines = budgeted
        .entities()
        .filter(|e| matches!(e, EntityType::Line(_)))
        .count();
    assert_eq!(lines, 10);
}
