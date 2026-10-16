//! F9 robustness: the auxiliary recovery path re-reads a failed record's
//! framing and common data outside the decoder's own panic guard, so a damaged
//! record must never make it panic, and a record it does keep must stay a raw
//! `Unknown` object. The fixture follows upstream's `tests/issue134.rs`.
mod hardening_common;

use std::io::Cursor;

use acadrust::classes::DxfClass;
use acadrust::entities::{EntityType, Line};
use acadrust::io::dwg::dwg_reference_type::DwgReferenceType;
use acadrust::io::dwg::dwg_stream_writers::DwgMergedWriter;
use acadrust::io::dwg::{DwgReadOptions, DwgVersion};
use acadrust::objects::{Dictionary, ObjectType, Scale};
use acadrust::types::{DxfVersion, Handle};
use acadrust::{CadDocument, DwgReader, DwgWriter};
use hardening_common::{no_panic, serial};

const CONTEXT: &str = "ACDB_MLEADEROBJECTCONTEXTDATA_CLASS";
const CLASS_NUMBER: i16 = 540;

/// A drawing with one LINE and one MLeader context record whose class payload
/// cannot be decoded (a 2^31-1 array count). `damage` flips bits of one byte of
/// the record's merged data before it is written; the writer frames the record
/// (size, handle-stream bits, CRC) around those bytes in every version, so only
/// the data the recovery inspects is damaged. Returns the DWG bytes, the
/// handles of the LINE and the context record, and the length of the merged data.
fn fixture(version: DxfVersion, damage: Option<(usize, u8)>) -> (Vec<u8>, Handle, Handle, usize) {
    let mut document = CadDocument::with_version(version);
    document.dwg_source_version = Some(version);
    document.classes.clear();
    let mut class = DxfClass::new(CONTEXT, "AcDbTestAuxiliary");
    class.class_number = CLASS_NUMBER;
    document.classes.push_preserving(class);
    let line = document
        .add_entity(EntityType::Line(Line::from_coords(
            0.0, 0.0, 0.0, 10.0, 5.0, 0.0,
        )))
        .unwrap();
    let owner = document.header.named_objects_dict_handle;
    let auxiliary = document.allocate_handle();
    let extension = document.allocate_handle();
    let mut dictionary = Dictionary::new();
    dictionary.handle = extension;
    dictionary.owner = auxiliary;
    document
        .objects
        .insert(extension, ObjectType::Dictionary(dictionary));
    let scale_handle = document.allocate_handle();
    let mut scale = Scale::new("1:50", 1.0, 50.0);
    scale.handle = scale_handle;
    document
        .objects
        .insert(scale_handle, ObjectType::Scale(scale));
    let mut writer = DwgMergedWriter::new(DwgVersion::from_dxf_version(version).unwrap(), version);
    writer.write_object_type(CLASS_NUMBER);
    if version < DxfVersion::AC1024 {
        writer.save_position_for_size();
    }
    writer.main_mut().write_handle_undefined(auxiliary.value());
    writer.write_bit_short(3);
    writer
        .main_mut()
        .write_handle_undefined(document.app_ids.get("ACAD").unwrap().handle.value());
    writer.write_bytes(&[70, 42, 0]); // EED integer16 (1070).
    writer.write_bit_short(0);
    writer.write_handle(DwgReferenceType::SoftPointer, owner.value());
    writer.write_bit_long(1);
    writer.write_handle(DwgReferenceType::SoftPointer, owner.value());
    if version >= DxfVersion::AC1018 {
        writer.write_bit(false);
        writer.write_handle(DwgReferenceType::HardOwnership, extension.value());
    } else {
        writer.write_handle(DwgReferenceType::HardOwnership, extension.value());
    }
    if version >= DxfVersion::AC1027 {
        writer.write_bit(true);
    }
    writer.write_bit_short(4);
    writer.write_bit(true);
    writer.write_handle(DwgReferenceType::HardPointer, scale_handle.value());
    writer.write_bit_long(i32::MAX);
    let mut raw = writer.merge();
    let handle_bits = raw.len() as i64 * 8 - writer.handle_start_bits();
    let length = raw.len();
    if let Some((offset, flip)) = damage {
        raw[offset] ^= flip;
    }
    document.objects.insert(
        auxiliary,
        ObjectType::Unknown {
            type_name: CONTEXT.to_string(),
            handle: auxiliary,
            owner,
            raw_dxf_codes: None,
            raw_dwg_data: Some(raw),
            raw_dwg_handle_bits: handle_bits,
            raw_dwg_version: Some(version),
        },
    );
    if let Some(ObjectType::Dictionary(dictionary)) = document.objects.get_mut(&owner) {
        dictionary.add_entry("hardening_f9", auxiliary);
    }
    let bytes = DwgWriter::write_to_vec(&document).unwrap();
    (bytes, line, auxiliary, length)
}

/// What reading one drawing in both modes did with the auxiliary record.
#[derive(Default)]
struct Reads {
    preserved: usize,
    refused: usize,
}

/// Reads `bytes` in both modes; a panic anywhere fails the test. When a
/// recovery diagnostic is present the kept object must be a raw `Unknown`, and
/// a failsafe read that returns must still hold the undamaged LINE.
fn read_both_modes(label: &str, bytes: &[u8], line: Handle, auxiliary: Handle) -> Reads {
    let mut reads = Reads::default();
    for (mode, options) in [
        ("strict", DwgReadOptions::default()),
        ("failsafe", DwgReadOptions::failsafe()),
    ] {
        let result = no_panic(&format!("{label} {mode}"), || {
            DwgReader::from_stream_with_options(Cursor::new(bytes.to_vec()), options)
                .read_with_stats()
        });
        let Ok(outcome) = result else {
            reads.refused += 1;
            continue;
        };
        let kept = outcome.stats.diagnostics.iter().any(|diagnostic| {
            diagnostic.code == "auxiliary-object-preserved"
                && diagnostic.record_handle == Some(auxiliary.value())
        });
        if kept {
            reads.preserved += 1;
            assert!(
                matches!(
                    outcome.document.objects.get(&auxiliary),
                    Some(ObjectType::Unknown {
                        raw_dwg_data: Some(_),
                        ..
                    })
                ),
                "{label} {mode}: a preserved auxiliary object is not raw"
            );
        } else {
            reads.refused += 1;
        }
        if mode == "failsafe" {
            assert!(
                outcome.document.get_entity(line).is_some(),
                "{label} {mode}: the undamaged LINE vanished"
            );
        }
    }
    reads
}

#[test]
fn a_damaged_auxiliary_record_never_panics_the_recovery() {
    let _guard = serial();
    for version in [
        DxfVersion::AC1015,
        DxfVersion::AC1018,
        DxfVersion::AC1021,
        DxfVersion::AC1024,
        DxfVersion::AC1027,
        DxfVersion::AC1032,
    ] {
        let (bytes, line, auxiliary, length) = fixture(version, None);
        // The undamaged file keeps the record in both modes.
        let intact = read_both_modes(&format!("{version:?} intact"), &bytes, line, auxiliary);
        assert_eq!((intact.preserved, intact.refused), (2, 0), "{version:?}");

        // Damage every byte of the record's merged data: object type, common
        // data, extended data, handles and payload.
        let (mut preserved, mut refused) = (0, 0);
        for offset in 0..length {
            for flip in [0x01u8, 0x10, 0x80, 0xFF] {
                let (damaged, ..) = fixture(version, Some((offset, flip)));
                let reads = read_both_modes(
                    &format!("{version:?} record byte {offset} ^ {flip:#04x}"),
                    &damaged,
                    line,
                    auxiliary,
                );
                preserved += reads.preserved;
                refused += reads.refused;
            }
        }
        // Some damage leaves a recoverable record and some does not, so the
        // sweep exercised both the recovery and its refusal.
        assert!(
            preserved > 0,
            "{version:?}: no mutation reached the recovery"
        );
        assert!(refused > 0, "{version:?}: no mutation was refused");
    }
}
