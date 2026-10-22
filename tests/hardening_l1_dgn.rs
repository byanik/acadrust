//! Fork patch L1: the DGN line-style definition (`LSDEFINITION`) and its components decode with
//! the layouts the public drawings' records hold (bit traces, aggregates only).
//!
//! The records here are built bit by bit with the layout written out field by field, so that the
//! reader is checked against the layout, not against the fork's writer; a writer round trip and a
//! full read of the builder follow.
mod hardening_common;

use std::collections::HashMap;

use acadrust::classes::DxfClass;
use acadrust::io::dwg::crc::{CRC16_SEED, crc16};
use acadrust::io::dwg::dwg_document_builder::{DwgBuildOutcome, DwgDocumentBuilder};
use acadrust::io::dwg::dwg_reference_type::DwgReferenceType;
use acadrust::io::dwg::dwg_stream_readers::object_reader::DwgObjectReader;
use acadrust::io::dwg::dwg_stream_writers::bit_writer::DwgBitWriter;
use acadrust::io::dwg::dwg_version::DwgVersion;
use acadrust::objects::{
    DgnLineStyleData, DgnLineStyleObject, DgnLsComponentData, DgnLsComponentType,
    DgnLsCompoundComponent, DgnLsCompoundEntry, DgnLsInternalComponent, DgnLsPhaseMode,
    DgnLsPointComponent, DgnLsStroke, DgnLsStrokePattern, DgnLsSymbolComponent,
    DgnLsSymbolReference, ObjectType,
};
use acadrust::types::Handle;
use acadrust::{CadDocument, DxfVersion};
use hardening_common::serial;

const VERSION: DxfVersion = DxfVersion::AC1032;
const HANDLE: u64 = 0x2A;
const OWNER: u64 = 0x1F;

const DEFINITION: i16 = 500;
const SYMBOL: i16 = 501;
const COMPOUND: i16 = 502;
const STROKE: i16 = 503;
const POINT: i16 = 504;
const INTERNAL: i16 = 505;
const CLASSES: &[(i16, &str)] = &[
    (DEFINITION, "LSDEFINITION"),
    (SYMBOL, "LSSYMBOLCOMPONENT"),
    (COMPOUND, "LSCOMPOUNDCOMPONENT"),
    (STROKE, "LSSTROKEPATTERNCOMPONENT"),
    (POINT, "LSPOINTCOMPONENT"),
    (INTERNAL, "LSINTERNALCOMPONENT"),
];

/// A version 1 GUID as the records store it (version nibble 1 in byte 7, variant 10 in byte 8).
const UID: [u8; 16] = [
    0x0D, 0x1C, 0x41, 0x5E, 0x33, 0x3F, 0xE2, 0x11, 0x96, 0xB7, 0x00, 0x11, 0x22, 0x33, 0x44, 0x55,
];

fn writer() -> DwgBitWriter {
    DwgBitWriter::new(DwgVersion::from_dxf_version(VERSION).unwrap(), VERSION)
}

fn bits_of(w: DwgBitWriter) -> Vec<bool> {
    let n = w.position_in_bits() as usize;
    let bytes = w.into_bytes();
    (0..n)
        .map(|i| bytes[i / 8] >> (7 - i % 8) & 1 == 1)
        .collect()
}

/// One R2018 record frame: MS(size), MC(handle-stream bits), merged data, CRC-16. The merged
/// data is the main stream, zero padding, the text stream (BS character count and UTF-16LE),
/// its size in bits (RS), the text-stream flag 1 and the handle stream, which ends the last byte.
fn frame(main: DwgBitWriter, text: &str, handles: DwgBitWriter) -> Vec<u8> {
    let mut t = writer();
    let units: Vec<u16> = text.encode_utf16().collect();
    t.write_bit_short(units.len() as i16);
    for u in units {
        t.write_bytes(&u.to_le_bytes());
    }
    let text = bits_of(t);
    let mut size = writer();
    size.write_raw_ushort(text.len() as u16);
    let size = bits_of(size);
    let main = bits_of(main);
    let handles = bits_of(handles);
    let tail = text.len() + size.len() + 1 + handles.len();
    let pad = (8 - (main.len() + tail) % 8) % 8;
    let mut bits = main;
    bits.extend(std::iter::repeat_n(false, pad));
    bits.extend(text);
    bits.extend(size);
    bits.push(true);
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

/// An object record of class `type_code` with sound common data, then `payload` in the main
/// stream, the description `DESC` in the text stream and the owner and `refs` (hard pointers)
/// in the handle stream.
fn record(type_code: i16, payload: impl FnOnce(&mut DwgBitWriter), refs: &[u64]) -> Vec<u8> {
    let mut main = writer();
    main.write_object_type(type_code);
    main.write_handle(DwgReferenceType::Undefined, HANDLE);
    main.write_bit_short(0); // no extended data
    main.write_bit_long(0); // no reactors
    main.write_bit(true); // no extension dictionary
    main.write_bit(false); // no data-store data
    payload(&mut main);
    let mut handles = writer();
    handles.write_handle(DwgReferenceType::SoftPointer, OWNER);
    for &r in refs {
        handles.write_handle(DwgReferenceType::HardPointer, r);
    }
    frame(main, DESC, handles)
}

const DESC: &str = "{ CF Dash }";

fn build(record: Vec<u8>) -> (DwgBuildOutcome, CadDocument) {
    let mut handles = HashMap::new();
    handles.insert(HANDLE, 0);
    let reader = DwgObjectReader::new(record, VERSION, handles).unwrap();
    let mut builder = DwgDocumentBuilder::new(reader);
    builder.set_failsafe(true);
    let mut document = CadDocument::with_version(VERSION);
    document.classes.clear();
    for (number, name) in CLASSES {
        let mut class = DxfClass::new(*name, *name);
        class.class_number = *number;
        document.classes.push_preserving(class);
    }
    let outcome = builder.build_with_stats(&mut document);
    (outcome, document)
}

/// The decoded line-style data of a one-record build that reported nothing.
fn decoded(record: Vec<u8>) -> DgnLineStyleData {
    let (outcome, document) = build(record);
    assert!(outcome.diagnostics.is_empty(), "{:?}", outcome.diagnostics);
    assert_eq!(outcome.known_nongraphical_records, 0);
    assert_eq!(outcome.skipped_records, 0);
    match document.objects.get(&Handle::from(HANDLE)) {
        Some(ObjectType::DgnLineStyle(o)) => o.data.clone(),
        other => panic!("not a DGN line-style object: {other:?}"),
    }
}

/// The header every component record starts with: BL version 1, BL kind, the uid, BD scale,
/// RC flags.
fn component_header(w: &mut DwgBitWriter, kind: i32, scale: f64) {
    w.write_bit_long(1);
    w.write_bit_long(kind);
    w.write_bytes(&UID);
    w.write_bit_double(scale);
    w.write_byte(1);
}

/// A stroke pattern as the records hold it: B iteration-limit flag, B single segment, BL phase
/// mode, BL iteration limit, BD phase, BD auto phase, BL count, then per stroke five B, BD
/// length, BD start width, BD end width, BL cap mode, BL width mode.
fn pattern_bits(w: &mut DwgBitWriter) {
    w.write_bit(true); // iteration limit
    w.write_bit(false); // single segment
    w.write_bit_long(1); // phase mode: fraction
    w.write_bit_long(3); // iteration limit
    w.write_bit_double(0.5); // phase
    w.write_bit_double(0.25); // auto phase
    w.write_bit_long(3);
    for (dash, len, sw, ew, cap, width) in [
        (true, 4.0, 1.0, 1.0, 0, 3),
        (false, 2.0, 0.0, 0.0, 0, 0),
        (true, 0.0, 0.0, 2.0, 1, 2),
    ] {
        w.write_bit(dash);
        w.write_bit(true); // bypass corner
        w.write_bit(dash); // can be scaled
        w.write_bit(false); // invert at origin
        w.write_bit(!dash); // invert at end
        w.write_bit_double(len);
        w.write_bit_double(sw);
        w.write_bit_double(ew);
        w.write_bit_long(cap);
        w.write_bit_long(width);
    }
}

fn expected_pattern() -> DgnLsStrokePattern {
    let stroke =
        |is_dash: bool, length: f64, start_width: f64, end_width: f64, cap_mode, width_mode| {
            DgnLsStroke {
                is_dash,
                bypass_corner: true,
                can_be_scaled: is_dash,
                invert_at_origin: false,
                invert_at_end: !is_dash,
                length,
                start_width,
                end_width,
                width_mode,
                cap_mode,
            }
        };
    DgnLsStrokePattern {
        has_iteration_limit: true,
        is_single_segment: false,
        iteration_limit: 3,
        auto_phase: 0.25,
        phase: 0.5,
        phase_mode: DgnLsPhaseMode::Fraction,
        strokes: vec![
            stroke(true, 4.0, 1.0, 1.0, 0, 3),
            stroke(false, 2.0, 0.0, 0.0, 0, 0),
            stroke(true, 0.0, 0.0, 2.0, 1, 2),
        ],
    }
}

#[test]
fn l1_a_definition_holds_its_units_and_flags_before_the_root_components_uid() {
    let _guard = serial();
    // BL version, BL 0, BD unit definition, BL units type, BD unit scale, five B, the root
    // component's uid; the root component in the handle stream.
    let rec = record(
        DEFINITION,
        |w| {
            w.write_bit_long(1);
            w.write_bit_long(0);
            w.write_bit_double(0.001);
            w.write_bit_long(1);
            w.write_bit_double(2.5);
            for b in [false, true, false, true, true] {
                w.write_bit(b);
            }
            w.write_bytes(&UID);
        },
        &[0x77],
    );
    let (outcome, document) = build(rec.clone());
    assert!(outcome.diagnostics.is_empty(), "{:?}", outcome.diagnostics);
    let root = document
        .dgn_ls_definitions
        .get(&Handle::from(HANDLE))
        .map(|d| d.root_component);
    assert_eq!(root, Some(Handle::from(0x77)));
    match decoded(rec) {
        DgnLineStyleData::Definition {
            description,
            version,
            style_number,
            component_uid,
            is_continuous,
            unit_definition,
            unit_scale,
            units_type,
            is_element,
            is_physical,
            is_scale_independent,
            is_snappable,
            root_component,
            ..
        } => {
            assert_eq!(description, DESC);
            assert_eq!((version, style_number), (1, 0));
            assert_eq!(unit_definition, 0.001);
            assert_eq!(units_type, 1);
            assert_eq!(unit_scale, 2.5);
            assert_eq!(
                [
                    is_continuous,
                    is_element,
                    is_physical,
                    is_scale_independent,
                    is_snappable
                ],
                [false, true, false, true, true]
            );
            assert_eq!(component_uid, UID);
            assert_eq!(root_component, Handle::from(0x77));
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn l1_a_stroke_pattern_reads_its_phase_mode_before_the_limit_and_the_phases() {
    let _guard = serial();
    let rec = record(
        STROKE,
        |w| {
            component_header(w, 3, 1.0);
            pattern_bits(w);
        },
        &[],
    );
    match decoded(rec) {
        DgnLineStyleData::Component {
            kind: DgnLsComponentType::Stroke,
            description,
            version: 1,
            component_uid,
            scale,
            property_flags: 1,
            component: DgnLsComponentData::Stroke(p),
            ..
        } => {
            assert_eq!(description, DESC);
            assert_eq!(component_uid, UID);
            assert_eq!(scale, 1.0);
            assert_eq!(p, expected_pattern());
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn l1_every_phase_mode_and_a_mode_out_of_range() {
    let _guard = serial();
    for (code, mode) in [
        (0, DgnLsPhaseMode::Distance),
        (1, DgnLsPhaseMode::Fraction),
        (2, DgnLsPhaseMode::Centered),
        (3, DgnLsPhaseMode::Reserved),
        (9, DgnLsPhaseMode::Reserved),
    ] {
        let rec = record(
            STROKE,
            |w| {
                component_header(w, 3, 1.0);
                w.write_bit(false);
                w.write_bit(true);
                w.write_bit_long(code);
                w.write_bit_long(0);
                w.write_bit_double(4.0);
                w.write_bit_double(0.0);
                w.write_bit_long(0);
            },
            &[],
        );
        match decoded(rec) {
            DgnLineStyleData::Component {
                component: DgnLsComponentData::Stroke(p),
                ..
            } => {
                assert_eq!(p.phase_mode, mode, "{code}");
                assert_eq!(p.phase, 4.0, "{code}");
                assert!(p.is_single_segment && !p.has_iteration_limit);
                assert!(p.strokes.is_empty());
            }
            other => panic!("{other:?}"),
        }
    }
}

#[test]
fn l1_an_internal_component_is_a_stroke_pattern_and_its_line_code() {
    let _guard = serial();
    let rec = record(
        INTERNAL,
        |w| {
            component_header(w, 6, 1.0);
            pattern_bits(w);
            w.write_bit_long(1); // internal version
            w.write_bit_long(2); // hardware style
            w.write_bit(false);
            w.write_bit_long(2); // line code
        },
        &[],
    );
    match decoded(rec) {
        DgnLineStyleData::Component {
            kind: DgnLsComponentType::Internal,
            component: DgnLsComponentData::Internal(i),
            ..
        } => assert_eq!(
            i,
            DgnLsInternalComponent {
                pattern: expected_pattern(),
                internal_version: 1,
                hardware_style: 2,
                is_hardware_style: false,
                line_code: 2,
            }
        ),
        other => panic!("{other:?}"),
    }
}

#[test]
fn l1_a_compound_holds_its_offsets_then_its_strands() {
    let _guard = serial();
    let rec = record(
        COMPOUND,
        |w| {
            component_header(w, 2, 1.0);
            w.write_bit_long(3);
            for o in [0.0, -2.0, 2.0] {
                w.write_bit_double(o);
            }
        },
        &[0x51, 0x52, 0x53],
    );
    match decoded(rec) {
        DgnLineStyleData::Component {
            kind: DgnLsComponentType::Compound,
            component: DgnLsComponentData::Compound(c),
            ..
        } => assert_eq!(
            c,
            DgnLsCompoundComponent {
                entries: vec![
                    DgnLsCompoundEntry {
                        component: Handle::from(0x51),
                        offset: 0.0
                    },
                    DgnLsCompoundEntry {
                        component: Handle::from(0x52),
                        offset: -2.0
                    },
                    DgnLsCompoundEntry {
                        component: Handle::from(0x53),
                        offset: 2.0
                    },
                ]
            }
        ),
        other => panic!("{other:?}"),
    }
}

#[test]
fn l1_a_symbol_holds_its_scales_and_its_block() {
    let _guard = serial();
    let rec = record(
        SYMBOL,
        |w| {
            component_header(w, 1, 1.0);
            w.write_bit_double(96000.0);
            w.write_bit_double(12.5);
            w.write_bit(false);
            w.write_bit(true);
        },
        &[0x61],
    );
    match decoded(rec) {
        DgnLineStyleData::Component {
            kind: DgnLsComponentType::Symbol,
            component: DgnLsComponentData::Symbol(s),
            ..
        } => assert_eq!(
            s,
            DgnLsSymbolComponent {
                stored_unit_scale: 96000.0,
                unit_scale: 12.5,
                has_unit_scale: false,
                is_3d: true,
                block: Handle::from(0x61),
            }
        ),
        other => panic!("{other:?}"),
    }
}

#[test]
fn l1_a_point_component_holds_its_symbols_then_its_stroke_and_symbol_handles() {
    let _guard = serial();
    let rec = record(
        POINT,
        |w| {
            component_header(w, 4, 1.0);
            w.write_bit_long(2);
            for (stroke, y) in [(1, 0.2708), (4, -0.5)] {
                for b in [true, false, true, false, false, true] {
                    w.write_bit(b);
                }
                w.write_bit_long(3); // justify
                w.write_bit_long(1); // rotation
                w.write_bit_long(0); // vertex mask
                w.write_bit_double(0.0);
                w.write_bit_double(y);
                w.write_bit_double(0.5);
                w.write_bit_long(stroke);
            }
        },
        &[0x41, 0x42, 0x43],
    );
    let symbol = |symbol_component: u64, y_offset: f64, stroke_number: i32| DgnLsSymbolReference {
        symbol_component: Handle::from(symbol_component),
        partial_strokes: true,
        clip_partial: false,
        allow_stretch: true,
        partial_projected: false,
        use_symbol_color: false,
        use_symbol_lineweight: true,
        justify: 3,
        rotation_type: 1,
        vertex_mask: 0,
        x_offset: 0.0,
        y_offset,
        angle: 0.5,
        stroke_number,
    };
    match decoded(rec) {
        DgnLineStyleData::Component {
            kind: DgnLsComponentType::Point,
            component: DgnLsComponentData::Point(p),
            ..
        } => assert_eq!(
            p,
            DgnLsPointComponent {
                stroke_component: Handle::from(0x41),
                symbols: vec![symbol(0x42, 0.2708, 1), symbol(0x43, -0.5, 4)],
            }
        ),
        other => panic!("{other:?}"),
    }
}

#[test]
fn l1_a_definition_in_the_upstream_order_is_not_read_as_a_definition() {
    let _guard = serial();
    // The order the reader had before L1 (uid right after the kind, then the flags and units):
    // the record does not hold enough bits, a known non-graphical record, nothing kept.
    let rec = record(
        DEFINITION,
        |w| {
            w.write_bit_long(1);
            w.write_bit_long(0);
            w.write_bytes(&UID);
            w.write_bit(false);
        },
        &[0x77],
    );
    let (outcome, document) = build(rec);
    assert_eq!(outcome.known_nongraphical_records, 1);
    assert_eq!(outcome.skipped_records, 0);
    assert!(document.dgn_ls_definitions.is_empty());
}

fn ls(doc: &mut CadDocument, data: DgnLineStyleData) -> Handle {
    let handle = doc.allocate_handle();
    doc.objects.insert(
        handle,
        ObjectType::DgnLineStyle(DgnLineStyleObject {
            handle,
            owner: Handle::NULL,
            reactors: Vec::new(),
            xdictionary_handle: None,
            data,
        }),
    );
    handle
}

fn component(kind: DgnLsComponentType, component: DgnLsComponentData) -> DgnLineStyleData {
    DgnLineStyleData::Component {
        kind,
        description: format!("CF {kind:?}"),
        version: 1,
        component_uid: [kind.code() as u8; 16],
        scale: 1.0,
        property_flags: 1,
        component,
        properties: Vec::new(),
    }
}

#[test]
fn l1_the_writer_and_the_reader_agree_on_every_kind() {
    let _guard = serial();
    for version in [DxfVersion::AC1015, DxfVersion::AC1018, DxfVersion::AC1032] {
        let mut doc = CadDocument::with_version(version);
        let stroke = ls(
            &mut doc,
            component(
                DgnLsComponentType::Stroke,
                DgnLsComponentData::Stroke(expected_pattern()),
            ),
        );
        let internal = ls(
            &mut doc,
            component(
                DgnLsComponentType::Internal,
                DgnLsComponentData::Internal(DgnLsInternalComponent {
                    pattern: expected_pattern(),
                    internal_version: 1,
                    hardware_style: 3,
                    is_hardware_style: true,
                    line_code: 3,
                }),
            ),
        );
        let block = doc.block_records.iter().next().map(|b| b.handle).unwrap();
        let symbol = ls(
            &mut doc,
            component(
                DgnLsComponentType::Symbol,
                DgnLsComponentData::Symbol(DgnLsSymbolComponent {
                    stored_unit_scale: 2.0,
                    unit_scale: 2.0,
                    has_unit_scale: true,
                    is_3d: false,
                    block,
                }),
            ),
        );
        let point = ls(
            &mut doc,
            component(
                DgnLsComponentType::Point,
                DgnLsComponentData::Point(DgnLsPointComponent {
                    stroke_component: stroke,
                    symbols: vec![DgnLsSymbolReference {
                        symbol_component: symbol,
                        partial_strokes: false,
                        clip_partial: true,
                        allow_stretch: false,
                        partial_projected: true,
                        use_symbol_color: true,
                        use_symbol_lineweight: false,
                        justify: 2,
                        rotation_type: 0,
                        vertex_mask: 1,
                        x_offset: 0.5,
                        y_offset: -0.5,
                        angle: 1.0,
                        stroke_number: 2,
                    }],
                }),
            ),
        );
        let compound = ls(
            &mut doc,
            component(
                DgnLsComponentType::Compound,
                DgnLsComponentData::Compound(DgnLsCompoundComponent {
                    entries: vec![
                        DgnLsCompoundEntry {
                            component: stroke,
                            offset: 0.0,
                        },
                        DgnLsCompoundEntry {
                            component: point,
                            offset: 0.0,
                        },
                        DgnLsCompoundEntry {
                            component: internal,
                            offset: 1.5,
                        },
                    ],
                }),
            ),
        );
        let definition = ls(
            &mut doc,
            DgnLineStyleData::Definition {
                description: "CF DEF".into(),
                version: 1,
                style_number: 0,
                component_uid: [2; 16],
                is_continuous: false,
                unit_definition: 3.280839895013123e-06,
                unit_scale: 1.0,
                units_type: 1,
                is_element: true,
                is_physical: false,
                is_scale_independent: true,
                is_snappable: false,
                root_component: compound,
                properties: Vec::new(),
            },
        );
        let expected: Vec<(Handle, DgnLineStyleData)> =
            [stroke, internal, symbol, point, compound, definition]
                .iter()
                .map(|h| match doc.objects.get(h) {
                    Some(ObjectType::DgnLineStyle(o)) => (*h, o.data.clone()),
                    _ => unreachable!(),
                })
                .collect();
        let bytes = acadrust::DwgWriter::write_to_vec(&doc).unwrap();
        let mut reader = acadrust::DwgReader::from_stream(std::io::Cursor::new(bytes));
        let outcome = reader.read_with_stats().unwrap();
        assert_eq!(outcome.stats.known_nongraphical_records, 0, "{version:?}");
        let back = outcome.document;
        assert_eq!(back.dgn_ls_definitions.len(), 1, "{version:?}");
        assert_eq!(back.dgn_ls_components.len(), 5, "{version:?}");
        for (handle, data) in expected {
            match back.objects.get(&handle) {
                Some(ObjectType::DgnLineStyle(o)) => assert_eq!(o.data, data, "{version:?}"),
                other => panic!("{version:?} {handle:?}: {other:?}"),
            }
        }
    }
}
