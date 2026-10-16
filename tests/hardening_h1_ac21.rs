//! H1: the AC21 (R2007) LZ77 decompressor never panics on malformed input.
mod hardening_common;

use std::io::Cursor;

use acadrust::entities::{EntityType, Line};
use acadrust::io::dwg::compressor_ac21::compress_ac21;
use acadrust::io::dwg::decompressor_ac21::decompress_ac21;
use acadrust::types::Vector3;
use acadrust::{CadDocument, DwgReadOptions, DwgReader, DwgWriter, DxfError, DxfVersion};
use hardening_common::{no_panic, serial, XorShift};
use proptest::prelude::*;

fn expect_compression_error(label: &str, source: &[u8], offset: u32, length: u32, out_len: usize) {
    let mut buffer = vec![0u8; out_len];
    let result = no_panic(label, || decompress_ac21(source, offset, length, &mut buffer));
    assert!(
        matches!(result, Err(DxfError::Compression(_))),
        "{label}: expected Err(Compression), got {result:?}"
    );
}

#[test]
fn empty_source_is_an_error() {
    let _guard = serial();
    expect_compression_error("empty source", &[], 0, 1, 64);
}

#[test]
fn initial_offset_outside_the_source_is_an_error() {
    let _guard = serial();
    expect_compression_error("offset == len", &[0u8; 4], 4, 1, 64);
    expect_compression_error("offset > len", &[0u8; 4], 900, 1, 64);
}

#[test]
fn huge_length_is_an_error() {
    let _guard = serial();
    expect_compression_error("length u32::MAX", &[0u8; 16], 1, u32::MAX, 64);
}

#[test]
fn literal_past_the_end_of_the_source_is_an_error() {
    let _guard = serial();
    // Opcode 0x05 asks for a 13-byte literal; only 4 bytes follow.
    expect_compression_error("short literal", &[0x05, 1, 2, 3, 4], 0, 5, 64);
}

#[test]
fn back_reference_before_the_output_start_is_an_error() {
    let _guard = serial();
    // 8-byte literal, then a compact match (0xF0) whose offset is 0x1F1 > 8.
    let source = [0x00, 1, 2, 3, 4, 5, 6, 7, 8, 0xF0, 0xF8];
    expect_compression_error("offset before start", &source, 0, source.len() as u32, 64);
}

#[test]
fn back_reference_past_the_output_end_is_an_error() {
    let _guard = serial();
    // 8-byte literal, then a long match (0x0F) of 34 bytes into a 32-byte buffer.
    let source = [0x00, 1, 2, 3, 4, 5, 6, 7, 8, 0x0F, 0x00, 0x00];
    expect_compression_error("match overflows output", &source, 0, source.len() as u32, 32);
}

#[test]
fn endless_literal_length_extension_is_an_error() {
    let _guard = serial();
    // Opcode 0x0F (length 0x17) followed only by 0xFF extension bytes.
    let mut source = vec![0x0F];
    source.extend(std::iter::repeat(0xFF).take(65));
    let length = source.len() as u32;
    expect_compression_error("endless 0xFFFF extension", &source, 0, length, 64);
}

#[test]
fn compressed_length_beyond_the_metadata_buffer_is_an_error() {
    let _guard = serial();
    // The AC1021 header decodes 0x110 metadata bytes from a 717-byte buffer.
    expect_compression_error("compr_len 0x7FFF_FFFF", &[0u8; 717], 32, 0x7FFF_FFFF, 0x110);
}

proptest! {
    #![proptest_config(ProptestConfig {
        cases: 20_000,
        failure_persistence: None,
        ..ProptestConfig::default()
    })]

    #[test]
    fn random_input_never_panics(
        source in proptest::collection::vec(any::<u8>(), 0..4096),
        offset_seed in any::<u32>(),
        length in prop_oneof![any::<u32>(), 0u32..8192],
        out_len in 0usize..8192,
    ) {
        let _guard = serial();
        let offset = offset_seed % (source.len() as u32 + 8);
        let mut buffer = vec![0u8; out_len];
        let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            decompress_ac21(&source, offset, length, &mut buffer)
        }));
        prop_assert!(outcome.is_ok(), "decompress_ac21 panicked");
    }
}

#[test]
fn compressor_roundtrip_is_byte_exact() {
    let _guard = serial();
    let mut rng = XorShift::new(0xAC21);
    for size in [1usize, 31, 32, 0x7400, 1 << 20] {
        let data = rng.bytes(size);
        let compressed = compress_ac21(&data);
        let mut out = vec![0u8; size];
        decompress_ac21(&compressed, 0, compressed.len() as u32, &mut out)
            .unwrap_or_else(|e| panic!("size {size}: {e}"));
        assert!(out == data, "size {size}: roundtrip differs");
    }
}

// ---------------------------------------------------------------------------
// A corrupt AC1021 data page: strict reads fail, failsafe reads skip the page.
// ---------------------------------------------------------------------------

fn ac21_document_bytes() -> Vec<u8> {
    let mut doc = CadDocument::with_version(DxfVersion::AC1021);
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
/// first LZ77-compressed page of `AcDb:AcDbObjects`.
fn corrupt_objects_page(bytes: &[u8]) -> (Vec<u8>, i64, String) {
    const BLOCK_SIZE: u64 = 251; // RS(255,251) data bytes per block
    const HEADER_SIZE: u64 = 0x480;
    let info = DwgReader::from_stream(Cursor::new(bytes.to_vec()))
        .read_file_header()
        .unwrap();
    let section = info
        .section_descriptors
        .iter()
        .find(|s| s.name == "AcDb:AcDbObjects")
        .expect("AcDb:AcDbObjects section");
    let page = section
        .pages
        .iter()
        .find(|p| p.compressed_size != p.decompressed_size)
        .expect("a compressed page");
    let (page_offset, _) = info.page_records[&(page.page_number as i32)];
    let aligned = (page.compressed_size + 7) & !7;
    let factor = (aligned + BLOCK_SIZE - 1) / BLOCK_SIZE;
    let mut corrupted = bytes.to_vec();
    // Stream byte k of the first block sits at `k * factor` in the page.
    // 0x0F announces a literal run of 0x17 + extension bytes; 0xFF extension
    // bytes ask for 0xFFFF more each, far beyond the page.
    for k in 0..24u64 {
        let at = (HEADER_SIZE + page_offset as u64 + k * factor) as usize;
        corrupted[at] = if k == 0 { 0x0F } else { 0xFF };
    }
    (corrupted, page.page_number, section.name.clone())
}

#[test]
fn a_corrupt_page_fails_the_strict_section_read_and_is_skipped_in_failsafe() {
    let _guard = serial();
    let bytes = ac21_document_bytes();
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
        failure.message.contains(&format!("page {page} in section '{section}'"))
            && failure.message.contains("AC21 LZ77"),
        "unexpected message: {}",
        failure.message
    );
    assert!(!strict.stats.record_stream_read);
    assert_eq!(strict.stats.output_entities, 0);

    // Failsafe: the page is zero-filled and reported, the section read succeeds.
    let failsafe = no_panic("failsafe read", || {
        DwgReader::from_stream_with_options(Cursor::new(corrupted), DwgReadOptions::failsafe())
            .read_with_stats()
    })
    .unwrap();
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
