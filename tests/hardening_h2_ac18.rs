//! H2: the AC18 LZ77 decompressor validates the decompressed size before it
//! allocates and rejects back-references before the output start.
mod hardening_common;

use std::io::Cursor;

use acadrust::entities::{EntityType, Line};
use acadrust::io::dwg::compression::DwgLZ77AC18Compressor;
use acadrust::io::dwg::decompressor_ac18::decompress_ac18;
use acadrust::io::limits::AllocBudget;
use acadrust::types::Vector3;
use acadrust::{CadDocument, DwgReadOptions, DwgReader, DwgWriter, DxfError, DxfVersion};
use hardening_common::{largest_alloc_during, no_panic, serial, XorShift};
use proptest::prelude::*;

#[test]
fn impossible_decompressed_sizes_are_errors_without_allocation() {
    let _guard = serial();
    for size in [usize::MAX, 0xFFFF_FFF0] {
        let (result, largest) =
            largest_alloc_during(|| no_panic("huge size", || decompress_ac18(&[0x11], size, None)));
        assert!(
            matches!(result, Err(DxfError::LimitExceeded { .. })),
            "size {size:#X}: {result:?}"
        );
        assert!(largest < 64 << 20, "size {size:#X}: {largest}-byte request");
    }
}

#[test]
fn the_budget_is_asked_before_the_page_is_reserved() {
    let _guard = serial();
    let refuse = AllocBudget::new(|_| false);
    let (result, largest) =
        largest_alloc_during(|| decompress_ac18(&[0x01, 1, 2, 3, 4, 0x11], 0x7400, Some(&refuse)));
    assert!(matches!(result, Err(DxfError::BudgetExceeded { .. })), "{result:?}");
    assert!(largest < 0x7400, "{largest}-byte request despite the refusal");
}

#[test]
fn back_reference_before_the_output_start_is_an_error() {
    let _guard = serial();
    // Four literal bytes, then a short match (0x40) whose offset is 1021.
    let source = [0x01, 0xAA, 0xBB, 0xCC, 0xDD, 0x40, 0xFF, 0x11];
    let result = no_panic("offset before start", || decompress_ac18(&source, 64, None));
    assert!(matches!(result, Err(DxfError::Compression(_))), "{result:?}");
}

#[test]
fn compressor_roundtrip_is_byte_exact() {
    let _guard = serial();
    let mut rng = XorShift::new(0xAC18);
    for data in [rng.bytes(0x7400), vec![0u8; 0x7400]] {
        let compressed = DwgLZ77AC18Compressor::new().compress(&data, 0, data.len());
        let out = decompress_ac18(&compressed, data.len(), None).unwrap();
        assert!(out == data, "roundtrip differs for a {}-byte page", data.len());
    }
}

proptest! {
    #![proptest_config(ProptestConfig {
        cases: 5_000,
        failure_persistence: None,
        ..ProptestConfig::default()
    })]

    #[test]
    fn random_input_never_panics(
        source in proptest::collection::vec(any::<u8>(), 0..4096),
        size in prop_oneof![any::<usize>(), 0usize..65536],
    ) {
        let _guard = serial();
        let outcome = std::panic::catch_unwind(|| decompress_ac18(&source, size, None));
        prop_assert!(outcome.is_ok(), "decompress_ac18 panicked");
    }
}

// ---------------------------------------------------------------------------
// A corrupt AC1018 data page: strict reads fail, failsafe reads skip the page,
// and a refused budget stops both.
// ---------------------------------------------------------------------------

fn ac18_document_bytes() -> Vec<u8> {
    let mut doc = CadDocument::with_version(DxfVersion::AC1018);
    for i in 0..40 {
        let x = i as f64;
        doc.add_entity(EntityType::Line(Line::from_points(
            Vector3::new(x, 0.0, 0.0),
            Vector3::new(x, 10.0 + x, 0.0),
        )))
        .unwrap();
    }
    DwgWriter::write_to_vec(&doc).unwrap()
}

/// Returns the corrupted file, the page number and the section name of the
/// first page of `AcDb:AcDbObjects`. The page's LZ77 stream (it follows the
/// 32-byte masked page header) is replaced by four literals and a short match
/// that reaches 1021 bytes back, before the output start.
fn corrupt_objects_page(bytes: &[u8]) -> (Vec<u8>, i64, String) {
    const PAGE_HEADER: usize = 32;
    const BAD_STREAM: [u8; 8] = [0x01, 0xAA, 0xBB, 0xCC, 0xDD, 0x40, 0xFF, 0x11];
    let info = DwgReader::from_stream(Cursor::new(bytes.to_vec()))
        .read_file_header()
        .unwrap();
    let section = info
        .section_descriptors
        .iter()
        .find(|s| s.name == "AcDb:AcDbObjects")
        .expect("AcDb:AcDbObjects section");
    assert_eq!(section.encoding, 2, "the objects section is LZ77-compressed");
    let page = section.pages.first().expect("a page");
    let (page_offset, _) = info.page_records[&(page.page_number as i32)];
    assert!(
        page.compressed_size as usize > PAGE_HEADER + BAD_STREAM.len(),
        "the page holds {} bytes",
        page.compressed_size
    );
    let mut corrupted = bytes.to_vec();
    let at = page_offset as usize + PAGE_HEADER;
    corrupted[at..at + BAD_STREAM.len()].copy_from_slice(&BAD_STREAM);
    (corrupted, page.page_number, section.name.clone())
}

#[test]
fn a_corrupt_page_fails_the_strict_section_read_and_is_skipped_in_failsafe() {
    let _guard = serial();
    let bytes = ac18_document_bytes();
    let intact = DwgReader::from_stream(Cursor::new(bytes.clone()))
        .read_with_stats()
        .unwrap();
    assert_eq!(intact.stats.output_entities, 40, "the intact file reads back");
    assert!(intact.stats.diagnostics.is_empty());

    let (corrupted, page, section) = corrupt_objects_page(&bytes);

    // Strict: the page error fails the section read (the document read records
    // it as a diagnostic, so the entities are gone) instead of being skipped.
    let strict = no_panic("strict read", || {
        DwgReader::from_stream(Cursor::new(corrupted.clone())).read_with_stats()
    })
    .unwrap();
    let failure = strict
        .stats
        .diagnostics
        .iter()
        .find(|d| d.code == "objects-extract-failed")
        .expect("strict read reports objects-extract-failed");
    assert!(
        failure.message.contains("AC18 LZ77: back-reference"),
        "unexpected message: {}",
        failure.message
    );
    assert!(!strict.stats.record_stream_read);
    assert_eq!(strict.stats.output_entities, 0);

    // Failsafe: the page is skipped (left zero-filled) and reported, and the
    // section read succeeds.
    let failsafe = no_panic("failsafe read", || {
        DwgReader::from_stream_with_options(Cursor::new(corrupted), DwgReadOptions::failsafe())
            .read_with_stats()
    })
    .unwrap();
    let skipped_note = format!("skipped corrupt page {page} in section '{section}'");
    assert!(
        failsafe
            .document
            .notifications
            .iter()
            .any(|n| n.message.contains(&skipped_note)),
        "the skipped page is reported"
    );
    assert!(
        failsafe
            .stats
            .diagnostics
            .iter()
            .all(|d| d.code != "objects-extract-failed"),
        "failsafe must skip the page, not fail the section"
    );
    assert!(failsafe.stats.record_stream_read);
    assert!(failsafe.stats.recovered_errors >= 1);
}

#[test]
fn a_refused_budget_stops_the_section_read_even_in_failsafe() {
    let _guard = serial();
    let bytes = ac18_document_bytes();
    // The page map (120 bytes) and the section map (1380 bytes) are far smaller
    // than a data page (0x7400 bytes), so this budget lets the system pages
    // through and refuses every data page.
    for options in [DwgReadOptions::default(), DwgReadOptions::failsafe()] {
        let failsafe = options.failsafe;
        let options = DwgReadOptions {
            budget: Some(AllocBudget::new(|bytes| bytes < 0x7400)),
            ..options
        };
        let outcome = no_panic("budget read", || {
            DwgReader::from_stream_with_options(Cursor::new(bytes.clone()), options)
                .read_with_stats()
        })
        .unwrap();
        let refusals: Vec<_> = outcome
            .stats
            .diagnostics
            .iter()
            .filter(|d| {
                d.code.ends_with("-extract-failed")
                    && d.message.contains("Allocation budget exceeded")
            })
            .collect();
        assert!(
            !refusals.is_empty(),
            "failsafe={failsafe}: the refusal was skipped as a corrupt page: {:?}",
            outcome.stats.diagnostics
        );
        assert_eq!(outcome.stats.output_entities, 0, "failsafe={failsafe}");
    }
}
