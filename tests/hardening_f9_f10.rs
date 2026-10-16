//! F10: CRC-valid empty records of the unused fixed type 55 are skipped
//! silently. (F9 is covered by upstream's `tests/issue134.rs`.)
mod hardening_common;

use std::collections::HashMap;

use acadrust::io::dwg::crc::{crc16, CRC16_SEED};
use acadrust::io::dwg::dwg_document_builder::{DwgBuildOutcome, DwgDocumentBuilder};
use acadrust::io::dwg::dwg_stream_readers::object_reader::DwgObjectReader;
use acadrust::{CadDocument, DxfVersion};
use hardening_common::serial;

/// Merged data of an R2010+ type-55 record without payload: the object type
/// is two bits `00` and the byte 0x37, followed by zero bits.
const STUB_55: [u8; 6] = [0x0D, 0xC0, 0x00, 0x00, 0x00, 0x00];

/// R2010+ record frame: MS(size), MC(handle-stream bits), data, CRC-16 over
/// everything before the CRC.
fn frame(merged: &[u8]) -> Vec<u8> {
    let mut frame = (merged.len() as u16).to_le_bytes().to_vec();
    frame.push(0); // MC: no handle-stream bits
    frame.extend_from_slice(merged);
    let crc = crc16(CRC16_SEED, &frame);
    frame.extend_from_slice(&crc.to_le_bytes());
    frame
}

fn build(record: Vec<u8>) -> DwgBuildOutcome {
    let mut handles = HashMap::new();
    handles.insert(0x2A, 0);
    let reader = DwgObjectReader::new(record, DxfVersion::AC1032, handles).unwrap();
    let mut builder = DwgDocumentBuilder::new(reader);
    builder.set_failsafe(true);
    let mut document = CadDocument::with_version(DxfVersion::AC1032);
    builder.build_with_stats(&mut document)
}

#[test]
fn a_crc_valid_type_55_stub_is_skipped_silently() {
    let _guard = serial();
    let record = frame(&STUB_55);
    assert_eq!(record.len(), 11);
    let outcome = build(record);
    assert_eq!(outcome.skipped_records, 0);
    assert!(outcome.diagnostics.is_empty(), "{:?}", outcome.diagnostics);
}

#[test]
fn a_type_55_stub_with_a_bad_crc_is_still_counted() {
    let _guard = serial();
    let mut record = frame(&STUB_55);
    let last = record.len() - 1;
    record[last] ^= 0xFF;
    let outcome = build(record);
    assert_eq!(outcome.skipped_records, 1);
    assert_eq!(outcome.diagnostics[0].code, "record-decode-failed");
}

#[test]
fn a_large_type_55_record_is_still_counted() {
    let _guard = serial();
    let mut merged = STUB_55.to_vec();
    merged.extend([0u8; 40]);
    let outcome = build(frame(&merged));
    assert_eq!(outcome.skipped_records, 1);
    assert_eq!(outcome.diagnostics[0].code, "record-decode-failed");
}

/// Same shape as `STUB_55` but for the neighbouring unused fixed type 54
/// (`00` then 0x36): only type 55 is skipped silently.
const STUB_54: [u8; 6] = [0x0D, 0x80, 0x00, 0x00, 0x00, 0x00];

#[test]
fn a_crc_valid_stub_of_another_type_is_still_counted() {
    let _guard = serial();
    let record = frame(&STUB_54);
    assert_eq!(record.len(), 11);
    let outcome = build(record);
    assert_eq!(outcome.skipped_records, 1);
    assert_eq!(outcome.diagnostics[0].code, "record-decode-failed");
}

#[test]
fn the_silent_skip_ends_at_a_frame_of_sixteen_bytes() {
    let _guard = serial();
    // MS (2) + MC (1) + data + CRC (2): 11 data bytes make a 16-byte frame.
    let mut merged = STUB_55.to_vec();
    merged.extend([0u8; 5]);
    let at_limit = frame(&merged);
    assert_eq!(at_limit.len(), 16);
    let outcome = build(at_limit);
    assert_eq!(outcome.skipped_records, 0);
    assert!(outcome.diagnostics.is_empty(), "{:?}", outcome.diagnostics);

    merged.push(0);
    let over_limit = frame(&merged);
    assert_eq!(over_limit.len(), 17);
    let outcome = build(over_limit);
    assert_eq!(outcome.skipped_records, 1);
    assert_eq!(outcome.diagnostics[0].code, "record-decode-failed");
}
