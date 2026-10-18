//! P3: records of known non-graphical classes (the VBA project, solid-history objects, line-style
//! definitions and the like) that cannot be decoded are reported apart from skipped records:
//! they are preserved raw when their framing and common data are sound, counted and passed over
//! otherwise, and they never make a strict read fail. Records that do draw, and unknown
//! classes, keep being counted as skipped.
mod hardening_common;

use std::collections::HashMap;

use acadrust::classes::DxfClass;
use acadrust::io::dwg::crc::{CRC16_SEED, crc16};
use acadrust::io::dwg::dwg_document_builder::{DwgBuildOutcome, DwgDocumentBuilder};
use acadrust::io::dwg::dwg_reference_type::DwgReferenceType;
use acadrust::io::dwg::dwg_stream_readers::object_reader::DwgObjectReader;
use acadrust::io::dwg::dwg_stream_writers::bit_writer::DwgBitWriter;
use acadrust::io::dwg::dwg_version::DwgVersion;
use acadrust::objects::ObjectType;
use acadrust::{CadDocument, DxfVersion};
use hardening_common::serial;

const VERSION: DxfVersion = DxfVersion::AC1032;
const HANDLE: u64 = 0x2A;

fn writer() -> DwgBitWriter {
    DwgBitWriter::new(DwgVersion::from_dxf_version(VERSION).unwrap(), VERSION)
}

fn bits_of(w: DwgBitWriter) -> Vec<bool> {
    let n = w.position_in_bits() as usize;
    let bytes = w.into_bytes();
    (0..n).map(|i| bytes[i / 8] >> (7 - i % 8) & 1 == 1).collect()
}

/// One R2018 record frame: MS(size), MC(handle-stream bits), merged data, CRC-16. The merged
/// data is the main stream, zero padding, the text-stream flag (0: no text) and the handle
/// stream, which ends the last byte.
fn frame(main: DwgBitWriter, handles: DwgBitWriter) -> Vec<u8> {
    let main = bits_of(main);
    let handles = bits_of(handles);
    let pad = (8 - (main.len() + 1 + handles.len()) % 8) % 8;
    let mut bits = main;
    bits.extend(std::iter::repeat_n(false, pad));
    bits.push(false);
    let handle_bits = handles.len();
    bits.extend(handles);
    let merged: Vec<u8> = bits
        .chunks(8)
        .map(|c| c.iter().fold(0u8, |b, &x| b << 1 | u8::from(x)))
        .collect();
    assert!(handle_bits < 128, "one MC byte");
    let mut frame = (merged.len() as u16).to_le_bytes().to_vec();
    frame.push(handle_bits as u8);
    frame.extend_from_slice(&merged);
    let crc = crc16(CRC16_SEED, &frame);
    frame.extend_from_slice(&crc.to_le_bytes());
    frame
}

/// A non-entity record of `type_code` with sound common data, then `payload` in the main
/// stream. Handle stream: the owner.
fn object_record(type_code: i16, payload: impl FnOnce(&mut DwgBitWriter)) -> Vec<u8> {
    let mut main = writer();
    main.write_object_type(type_code);
    main.write_handle(DwgReferenceType::Undefined, HANDLE);
    main.write_bit_short(0); // no extended data
    main.write_bit_long(0); // no reactors
    main.write_bit(true); // no extension dictionary
    main.write_bit(false); // no data-store data
    payload(&mut main);
    let mut handles = writer();
    handles.write_handle(DwgReferenceType::SoftPointer, 0x1F); // owner
    frame(main, handles)
}

/// A record whose common data cannot be read: the object type and nothing else.
fn stub_record(type_code: i16) -> Vec<u8> {
    let mut main = writer();
    main.write_object_type(type_code);
    frame(main, writer())
}

fn build(record: Vec<u8>, classes: &[(i16, &str)]) -> (DwgBuildOutcome, CadDocument) {
    let mut handles = HashMap::new();
    handles.insert(HANDLE, 0);
    let reader = DwgObjectReader::new(record, VERSION, handles).unwrap();
    let mut builder = DwgDocumentBuilder::new(reader);
    builder.set_failsafe(true);
    let mut document = CadDocument::with_version(VERSION);
    document.classes.clear();
    for (number, name) in classes {
        let mut class = DxfClass::new(*name, *name);
        class.class_number = *number;
        document.classes.push_preserving(class);
    }
    let outcome = builder.build_with_stats(&mut document);
    (outcome, document)
}

fn codes(outcome: &DwgBuildOutcome) -> Vec<&str> {
    outcome.diagnostics.iter().map(|d| d.code.as_str()).collect()
}

#[test]
fn a_vba_project_with_a_short_payload_is_preserved_not_skipped() {
    let _guard = serial();
    // BL announcing 7,168 bytes of macro data where none follow: the corpus failure.
    let record = object_record(81, |w| w.write_bit_long(7168));
    let (outcome, document) = build(record, &[]);
    assert_eq!(outcome.skipped_records, 0);
    assert_eq!(outcome.known_nongraphical_records, 1);
    assert_eq!(codes(&outcome), ["known-nongraphical-preserved"], "{:?}", outcome.diagnostics);
    let message = &outcome.diagnostics[0].message;
    assert!(message.contains("VBA_PROJECT"), "{message}");
    assert_eq!(outcome.diagnostics[0].record_handle, Some(HANDLE));
    // kept as an opaque object with its raw record
    let object = document.objects.get(&acadrust::types::Handle::from(HANDLE));
    assert!(
        matches!(object, Some(ObjectType::Unknown { raw_dwg_data: Some(_), .. })),
        "{object:?}"
    );
    // not an error: a failsafe read reports it as a warning
    assert_eq!(outcome.notifications.error_count(), 0);
}

#[test]
fn a_vba_project_with_broken_common_data_is_counted_not_skipped() {
    let _guard = serial();
    let (outcome, document) = build(stub_record(81), &[]);
    assert_eq!(outcome.skipped_records, 0);
    assert_eq!(outcome.known_nongraphical_records, 1);
    assert_eq!(codes(&outcome), ["known-nongraphical-skipped"], "{:?}", outcome.diagnostics);
    assert!(
        !document.objects.contains_key(&acadrust::types::Handle::from(HANDLE)),
        "nothing is kept of a record that cannot be framed"
    );
    assert_eq!(outcome.notifications.error_count(), 0);
}

#[test]
fn known_classes_that_cannot_be_decoded_are_counted_apart_from_skipped() {
    let _guard = serial();
    for (number, name) in [
        (500, "LSDEFINITION"),
        (501, "LSSTROKEPATTERNCOMPONENT"),
        (502, "ACSH_EXTRUSION_CLASS"),
        (503, "ACSH_SWEEP_CLASS"),
        (504, "ACSH_REVOLVE_CLASS"),
        (505, "ACSH_BOOLEAN_CLASS"),
        (506, "BREAKDATA"),
        (507, "ACDBASSOC2DCONSTRAINTGROUP"),
        (508, "ACDBASSOCPLANESURFACEACTIONBODY"),
        // the names as the class table spells them: any case, stray characters
        (509, "AcDbAssoc2dConstraintGroup"),
        (510, "AcSh_Loft_Class"),
    ] {
        let (outcome, _) = build(stub_record(number), &[(number, name)]);
        assert_eq!(outcome.skipped_records, 0, "{name}");
        assert_eq!(outcome.known_nongraphical_records, 1, "{name}");
        assert_eq!(codes(&outcome), ["known-nongraphical-skipped"], "{name}");
        assert_eq!(outcome.notifications.error_count(), 0, "{name}");
    }
}

#[test]
fn everything_else_keeps_being_skipped() {
    let _guard = serial();
    // an unknown class, a drawing class, one name that merely starts like a known one, and an
    // unused fixed type
    for (record, classes) in [
        (stub_record(500), vec![(500, "SOMEOTHERCLASS")]),
        (stub_record(500), vec![(500, "LSDEFINITIONS")]),
        (stub_record(500), vec![(500, "XACSH_BOX_CLASS")]),
        (stub_record(500), vec![]),
        (stub_record(54), vec![]),
    ] {
        let (outcome, _) = build(record, &classes);
        assert_eq!(outcome.skipped_records, 1, "{classes:?}");
        assert_eq!(outcome.known_nongraphical_records, 0, "{classes:?}");
        assert_eq!(codes(&outcome), ["record-decode-failed"], "{classes:?}");
    }
}

#[test]
fn an_entity_class_with_a_known_looking_name_is_still_skipped() {
    let _guard = serial();
    // The class table says the class is an entity: whatever its name, it draws.
    let mut handles = HashMap::new();
    handles.insert(HANDLE, 0);
    let reader = DwgObjectReader::new(stub_record(500), VERSION, handles).unwrap();
    let mut builder = DwgDocumentBuilder::new(reader);
    builder.set_failsafe(true);
    let mut document = CadDocument::with_version(VERSION);
    document.classes.clear();
    let mut class = DxfClass::new("ACSH_BOX_CLASS", "ACSH_BOX_CLASS");
    class.class_number = 500;
    class.is_an_entity = true;
    document.classes.push_preserving(class);
    let outcome = builder.build_with_stats(&mut document);
    assert_eq!(outcome.skipped_records, 1);
    assert_eq!(outcome.known_nongraphical_records, 0);
}
