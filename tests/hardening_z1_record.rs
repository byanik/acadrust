//! Fork patch Z1: the single-record decode entry for fuzzing (feature `fuzzing`).
//!
//! Every kind in `fuzzing::SCOPE` has records in the committed fixtures `tests/fixtures/z1/`
//! (written by this fork's own writer, see `regenerate_z1_fixtures`); each decodes `true`, and
//! every prefix of it and every single-bit flip of its payload return without a panic.
mod hardening_common;

use std::collections::BTreeMap;

use acadrust::fuzzing::{decode_object, frame_record, scope_records, ScopeRecord, SCOPE, VERSIONS};
use acadrust::types::DxfVersion;
use hardening_common::{no_panic, serial};

const FIXTURE_DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/z1");

/// The fixture versions (one file each): R14, R2000, R2004, R2007, R2018.
const FIXTURE_VERSIONS: &[(DxfVersion, &str)] = &[
    (DxfVersion::AC1014, "ac1014"),
    (DxfVersion::AC1015, "ac1015"),
    (DxfVersion::AC1018, "ac1018"),
    (DxfVersion::AC1021, "ac1021"),
    (DxfVersion::AC1032, "ac1032"),
];

fn fixture_records() -> Vec<ScopeRecord> {
    let mut all = Vec::new();
    for (version, tag) in FIXTURE_VERSIONS {
        let path = format!("{FIXTURE_DIR}/records.{tag}.dwg");
        let bytes = std::fs::read(&path).unwrap_or_else(|e| panic!("{path}: {e}"));
        let records = scope_records(&bytes).unwrap_or_else(|e| panic!("{path}: {e}"));
        assert!(!records.is_empty(), "{path}: no scope record");
        for r in &records {
            assert_eq!(r.version, *version, "{path}");
        }
        all.extend(records);
    }
    all
}

fn kind_name(type_code: i16) -> &'static str {
    SCOPE
        .iter()
        .find(|k| k.type_code == type_code)
        .map(|k| k.name)
        .unwrap_or("?")
}

#[test]
fn z1_scope_lists_each_kind_once() {
    let _guard = serial();
    let names: Vec<&str> = SCOPE.iter().map(|k| k.name).collect();
    assert_eq!(
        names,
        [
            "LEADER",
            "TOLERANCE",
            "MLINE",
            "HATCH",
            "ACAD_PROXY_ENTITY",
            "MULTILEADER",
            "SPATIAL_FILTER",
            "LSDEFINITION",
            "LSSYMBOLCOMPONENT",
            "LSCOMPOUNDCOMPONENT",
            "LSSTROKEPATTERNCOMPONENT",
            "LSPOINTCOMPONENT",
            "LSINTERNALCOMPONENT",
        ]
    );
    let mut codes: Vec<i16> = SCOPE.iter().map(|k| k.type_code).collect();
    codes.sort_unstable();
    codes.dedup();
    assert_eq!(codes.len(), SCOPE.len(), "type codes are distinct");
}

#[test]
fn z1_every_scope_kind_has_a_fixture_record_that_decodes() {
    let _guard = serial();
    let records = fixture_records();
    let mut by_kind: BTreeMap<&str, (usize, usize)> = BTreeMap::new();
    for r in &records {
        let ok = no_panic(kind_name(r.type_code), || {
            decode_object(r.type_code, r.version, &r.bytes)
        });
        let slot = by_kind.entry(kind_name(r.type_code)).or_default();
        slot.0 += 1;
        if ok {
            slot.1 += 1;
        }
        assert!(
            ok,
            "{} {:?}: a fixture record does not decode",
            kind_name(r.type_code),
            r.version
        );
    }
    eprintln!("records by kind (seen, decoded): {by_kind:?}");
    for kind in SCOPE {
        let (seen, ok) = by_kind.get(kind.name).copied().unwrap_or((0, 0));
        assert!(
            seen > 0 && ok == seen,
            "{}: {ok} of {seen} records decode",
            kind.name
        );
    }
}

#[test]
fn z1_every_prefix_and_bit_flip_returns_without_a_panic() {
    let _guard = serial();
    let records = fixture_records();
    // One record per kind and version keeps the sweep short.
    let mut seen = std::collections::BTreeSet::new();
    let mut prefixes = 0usize;
    let mut accepted_prefixes = 0usize;
    for r in &records {
        if !seen.insert((r.type_code, r.version as i32)) {
            continue;
        }
        for len in 0..r.bytes.len() {
            let label = format!("{} {:?} prefix {len}", kind_name(r.type_code), r.version);
            prefixes += 1;
            if no_panic(&label, || {
                decode_object(r.type_code, r.version, &r.bytes[..len])
            }) {
                accepted_prefixes += 1;
            }
        }
        for bit in 0..r.bytes.len() * 8 {
            let mut flipped = r.bytes.clone();
            flipped[bit / 8] ^= 1 << (bit % 8);
            let label = format!("{} {:?} bit {bit}", kind_name(r.type_code), r.version);
            no_panic(&label, || decode_object(r.type_code, r.version, &flipped));
        }
    }
    assert!(prefixes > 0);
    // A record cut before its last byte loses its CRC, not necessarily its data, so some prefixes
    // may still decode; the cut-off framing must turn most of them away.
    assert!(
        accepted_prefixes * 2 < prefixes,
        "{accepted_prefixes} of {prefixes} prefixes decode"
    );
}

#[test]
fn z1_a_prefix_without_the_whole_payload_is_refused() {
    let _guard = serial();
    for r in fixture_records() {
        // Size prefix and handle bits only: no payload at all.
        let head = if r.version >= DxfVersion::AC1024 {
            3
        } else {
            2
        };
        for len in 0..=head.min(r.bytes.len()) {
            assert!(
                !decode_object(r.type_code, r.version, &r.bytes[..len]),
                "{} {:?}: {len} bytes decode",
                kind_name(r.type_code),
                r.version
            );
        }
    }
}

#[test]
fn z1_frames_carry_multi_group_sizes_and_handle_bits() {
    let _guard = serial();
    let data = vec![0u8; 40_000];
    // 40,000 = 0x9C40: low 15 bits 0x1C40 with the continuation bit, then 1; 300 handle bits =
    // 0x2C with the continuation bit, then 2. The CRC-16 follows the data.
    let r2018 = frame_record(DxfVersion::AC1032, &data, 300);
    assert_eq!(&r2018[..6], &[0x40, 0x9C, 0x01, 0x00, 0xAC, 0x02]);
    assert_eq!(r2018.len(), 6 + data.len() + 2);
    let r2004 = frame_record(DxfVersion::AC1018, &data[..100], 300);
    assert_eq!(&r2004[..2], &[100, 0]);
    assert_eq!(r2004.len(), 2 + 100 + 2);
    assert_ne!(r2004[102..], [0, 0], "a CRC over the size and the data");
}

#[test]
fn z1_codes_outside_the_scope_are_refused() {
    let _guard = serial();
    // LINE, TEXT, DICTIONARY, the proxy object, a class number past the table, the internal
    // MULTILEADER code and the ends of the range: refused for every record, whatever it holds.
    let outside = [19, 1, 42, 499, 508, 1000, -3, 0, i16::MIN, i16::MAX];
    for r in fixture_records() {
        for code in outside {
            assert!(
                !decode_object(code, r.version, &r.bytes),
                "{} {:?} under type code {code}",
                kind_name(r.type_code),
                r.version
            );
        }
    }
}

#[test]
fn z1_versions_without_an_object_stream_are_refused() {
    let _guard = serial();
    let records = fixture_records();
    let leader = records
        .iter()
        .find(|r| kind_name(r.type_code) == "LEADER" && r.version == DxfVersion::AC1014)
        .expect("an R14 LEADER record");
    assert!(decode_object(
        leader.type_code,
        leader.version,
        &leader.bytes
    ));
    // R12 has no object stream; an R14 record framed alike is still refused under it.
    for version in [DxfVersion::AC1009, DxfVersion::Unknown] {
        assert!(!VERSIONS.contains(&version));
        assert!(
            !decode_object(leader.type_code, version, &leader.bytes),
            "{version:?}"
        );
    }
}

#[test]
fn z1_a_record_whose_handle_stream_is_cut_away_is_refused() {
    let _guard = serial();
    // R2010+ frames say where the handle stream starts. With no handle bits the handle reads run
    // past the data: the reader flags the record, even when a decoder already built the entity.
    let mut refused = 0;
    for r in fixture_records()
        .iter()
        .filter(|r| r.version == DxfVersion::AC1032)
    {
        assert!(decode_object(r.type_code, r.version, &r.bytes));
        let cut = frame_record(r.version, &r.data, 0);
        assert!(
            !decode_object(r.type_code, r.version, &cut),
            "{}: decodes without its handle stream",
            kind_name(r.type_code)
        );
        refused += 1;
    }
    assert!(refused >= SCOPE.len());
}

#[test]
fn z1_a_record_read_under_another_kind_does_not_decode_as_that_kind() {
    let _guard = serial();
    let records = fixture_records();
    let find = |name: &str| {
        records
            .iter()
            .find(|r| kind_name(r.type_code) == name && r.version == DxfVersion::AC1032)
            .unwrap_or_else(|| panic!("an AC1032 {name} record"))
    };
    // An object record is not an entity and an entity record is not an object: the entity
    // preamble and the object preamble differ, and the dispatch must follow the given code.
    let filter = find("SPATIAL_FILTER");
    let hatch = find("HATCH");
    assert!(!decode_object(
        hatch.type_code,
        filter.version,
        &filter.bytes
    ));
    assert!(!decode_object(
        filter.type_code,
        hatch.version,
        &hatch.bytes
    ));
    // A DGN line-style definition is not a component and a component is not a definition (the
    // class decides; a component's own kind comes from its data, so any component class reads
    // any component record).
    let definition = find("LSDEFINITION");
    let stroke = find("LSSTROKEPATTERNCOMPONENT");
    assert!(!decode_object(
        stroke.type_code,
        definition.version,
        &definition.bytes
    ));
    assert!(!decode_object(
        definition.type_code,
        stroke.version,
        &stroke.bytes
    ));
    assert!(decode_object(
        find("LSPOINTCOMPONENT").type_code,
        stroke.version,
        &stroke.bytes
    ));
}

/// Bits `at..at + n` of a frame, most significant first (the DWG bit order).
fn bits(bytes: &[u8], at: usize, n: usize) -> u32 {
    (at..at + n).fold(0, |v, i| {
        (v << 1) | u32::from(bytes[i / 8] >> (7 - i % 8) & 1)
    })
}

fn set_bits(bytes: &mut [u8], at: usize, n: usize, value: u32) {
    for (k, i) in (at..at + n).enumerate() {
        let bit = (value >> (n - 1 - k)) & 1;
        bytes[i / 8] = (bytes[i / 8] & !(0x80 >> (i % 8))) | ((bit as u8) << (7 - i % 8));
    }
}

#[test]
fn z1_a_record_the_builder_keeps_nothing_of_is_refused() {
    let _guard = serial();
    let records = fixture_records();
    // In the R2018 component records the version (BL 1: code 01, byte 1) sits at bits 56..66 of
    // the frame and the component kind (BL: code 01 at 66, byte at 68): 3 stroke, 6 internal.
    // Kind 5 does not exist: the reader stops there without an error, and the builder keeps no
    // object of the record.
    for (name, code) in [("LSSTROKEPATTERNCOMPONENT", 3), ("LSINTERNALCOMPONENT", 6)] {
        let r = records
            .iter()
            .find(|r| kind_name(r.type_code) == name && r.version == DxfVersion::AC1032)
            .unwrap_or_else(|| panic!("an AC1032 {name} record"));
        assert_eq!(
            bits(&r.bytes, 56, 10),
            0b01_0000_0001,
            "{name}: the version"
        );
        assert_eq!(bits(&r.bytes, 66, 2), 0b01, "{name}: the kind's BL code");
        assert_eq!(bits(&r.bytes, 68, 8), code, "{name}: the kind byte");
        let mut no_kind = r.bytes.clone();
        set_bits(&mut no_kind, 68, 8, 5);
        assert!(
            !decode_object(r.type_code, r.version, &no_kind),
            "{name}: kind 5 decodes"
        );
        set_bits(&mut no_kind, 68, 8, code);
        assert!(decode_object(r.type_code, r.version, &no_kind), "{name}");
    }
}

/// Writes `tests/fixtures/z1/records.<version>.dwg` with this fork's writer: one or more
/// records of every kind in `fuzzing::SCOPE` (MULTILEADER from R2007 on). The writer is not
/// byte-deterministic: the committed files are the fixtures, a rerun writes equivalent ones. Run with
/// `cargo test -p acadrust --features fuzzing,writers --test hardening_z1_record -- --ignored`.
#[cfg(feature = "writers")]
#[test]
#[ignore]
fn regenerate_z1_fixtures() {
    for (version, tag) in FIXTURE_VERSIONS {
        let doc = z1_scene::build(*version);
        let bytes = acadrust::DwgWriter::write_to_vec(&doc).expect("write dwg");
        std::fs::create_dir_all(FIXTURE_DIR).unwrap();
        std::fs::write(format!("{FIXTURE_DIR}/records.{tag}.dwg"), bytes).unwrap();
    }
}

#[cfg(feature = "writers")]
mod z1_scene {
    use acadrust::entities::{
        BoundaryEdge, BoundaryPath, EntityType, ExtendedEntity, ExtendedEntityData, Hatch, Insert,
        Leader, LineEdge, MLine, MultiLeader, ProxyEntityData, Tolerance,
    };
    use acadrust::objects::{
        DgnLineStyleData, DgnLineStyleObject, DgnLsComponentData, DgnLsComponentType,
        DgnLsCompoundComponent, DgnLsCompoundEntry, DgnLsInternalComponent, DgnLsPhaseMode,
        DgnLsPointComponent, DgnLsStroke, DgnLsStrokePattern, DgnLsSymbolComponent,
        DgnLsSymbolReference, ObjectType, ProxyPayload, SpatialFilter,
    };
    use acadrust::tables::BlockRecord;
    use acadrust::types::{DxfVersion, Handle, Matrix4, Vector2, Vector3};
    use acadrust::CadDocument;

    fn p(x: f64, y: f64) -> Vector3 {
        Vector3::new(x, y, 0.0)
    }

    fn stroke_pattern(lengths: &[f64]) -> DgnLsStrokePattern {
        DgnLsStrokePattern {
            has_iteration_limit: false,
            is_single_segment: false,
            iteration_limit: 0,
            auto_phase: 0.0,
            phase: 0.0,
            phase_mode: DgnLsPhaseMode::Distance,
            strokes: lengths
                .iter()
                .enumerate()
                .map(|(i, &length)| DgnLsStroke {
                    is_dash: i % 2 == 0,
                    bypass_corner: false,
                    can_be_scaled: true,
                    invert_at_origin: false,
                    invert_at_end: false,
                    length,
                    start_width: 0.0,
                    end_width: 0.0,
                    width_mode: 0,
                    cap_mode: 0,
                })
                .collect(),
        }
    }

    fn ls_object(doc: &mut CadDocument, data: DgnLineStyleData) -> Handle {
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

    fn component(
        doc: &mut CadDocument,
        kind: DgnLsComponentType,
        description: &str,
        component: DgnLsComponentData,
    ) -> Handle {
        ls_object(
            doc,
            DgnLineStyleData::Component {
                kind,
                description: description.to_string(),
                version: 1,
                component_uid: [kind.code() as u8; 16],
                scale: 1.0,
                property_flags: 0,
                component,
                properties: Vec::new(),
            },
        )
    }

    pub fn build(v: DxfVersion) -> CadDocument {
        let mut doc = CadDocument::with_version(v);

        let mut leader = Leader::from_vertices(vec![p(0.0, 0.0), p(10.0, 10.0), p(20.0, 10.0)]);
        leader.common.layer = "0".into();
        doc.add_entity(EntityType::Leader(leader)).unwrap();
        doc.add_entity(EntityType::Tolerance(Tolerance::with_text(
            p(0.0, 20.0),
            "{\\Fgdt;p}%%v0.5",
        )))
        .unwrap();
        doc.add_entity(EntityType::MLine(MLine::from_points(&[
            p(30.0, 0.0),
            p(40.0, 0.0),
            p(45.0, 8.0),
        ])))
        .unwrap();
        let mut hatch = Hatch::solid();
        let mut path = BoundaryPath::new();
        for (a, b) in [
            ((50.0, 0.0), (60.0, 0.0)),
            ((60.0, 0.0), (60.0, 10.0)),
            ((60.0, 10.0), (50.0, 0.0)),
        ] {
            path.add_edge(BoundaryEdge::Line(LineEdge {
                start: Vector2::new(a.0, a.1),
                end: Vector2::new(b.0, b.1),
            }));
        }
        hatch.add_path(path);
        doc.add_entity(EntityType::Hatch(Box::new(hatch))).unwrap();

        if v >= DxfVersion::AC1021 {
            let mut ml =
                MultiLeader::with_text("Note", p(80.0, 20.0), vec![p(70.0, 0.0), p(78.0, 20.0)]);
            ml.style_handle = doc
                .objects
                .iter()
                .find_map(|(h, o)| matches!(o, ObjectType::MultiLeaderStyle(_)).then_some(*h));
            ml.text_style_handle = Some(doc.header.current_text_style_handle);
            ml.context.text_style_handle = Some(doc.header.current_text_style_handle);
            doc.add_entity(EntityType::MultiLeader(Box::new(ml)))
                .unwrap();
        }

        // A proxy entity with a small payload and no graphics.
        let mut proxy = ExtendedEntity {
            common: Default::default(),
            data: ExtendedEntityData::Proxy(ProxyEntityData {
                proxy_id: 498,
                class_id: 499,
                dxf_subclass: "Z1_PROXY".into(),
                version: 0,
                dwg_version: 0,
                maintenance_version: 0,
                from_dxf: false,
                graphics: ProxyPayload::default(),
                payload: ProxyPayload::from_bytes(&[1, 2, 3, 4]),
                text_payload: ProxyPayload::default(),
                object_ids: Vec::new(),
            }),
        };
        proxy.common.layer = "0".into();
        doc.add_entity(EntityType::Extended(Box::new(proxy)))
            .unwrap();

        // An INSERT clipped by a SPATIAL_FILTER (rectangle in the block's frame).
        let mut block = BlockRecord::new("Z1_CLIPPED");
        block.handle = doc.allocate_handle();
        let block_name = block.name.clone();
        doc.block_records.add(block).unwrap();
        let insert = doc
            .add_entity(EntityType::Insert(Insert::new(&block_name, p(100.0, 0.0))))
            .unwrap();
        let filter = doc.allocate_handle();
        let mut sf = SpatialFilter::new();
        sf.handle = filter;
        sf.owner = insert;
        sf.boundary_points = vec![Vector2::new(-1.0, -1.0), Vector2::new(5.0, 4.0)];
        sf.normal = Vector3::new(0.0, 0.0, 1.0);
        sf.display_enabled = true;
        sf.inverse_block_transform = Matrix4::identity();
        sf.clip_bound_transform = Matrix4::identity();
        doc.objects.insert(filter, ObjectType::SpatialFilter(sf));

        // A DGN line style: a definition over a compound of every component kind.
        let stroke = component(
            &mut doc,
            DgnLsComponentType::Stroke,
            "z1 stroke",
            DgnLsComponentData::Stroke(stroke_pattern(&[1.0, 0.5, 0.25, 0.5])),
        );
        let internal = component(
            &mut doc,
            DgnLsComponentType::Internal,
            "z1 internal",
            DgnLsComponentData::Internal(DgnLsInternalComponent {
                pattern: stroke_pattern(&[2.0, 1.0]),
                internal_version: 1,
                hardware_style: 0,
                is_hardware_style: false,
                line_code: 2,
            }),
        );
        let symbol_block = doc.header.model_space_block_handle;
        let symbol = component(
            &mut doc,
            DgnLsComponentType::Symbol,
            "z1 symbol",
            DgnLsComponentData::Symbol(DgnLsSymbolComponent {
                stored_unit_scale: 1.0,
                unit_scale: 1.0,
                has_unit_scale: true,
                is_3d: false,
                block: symbol_block,
            }),
        );
        let point = component(
            &mut doc,
            DgnLsComponentType::Point,
            "z1 point",
            DgnLsComponentData::Point(DgnLsPointComponent {
                stroke_component: stroke,
                symbols: vec![DgnLsSymbolReference {
                    symbol_component: symbol,
                    partial_strokes: false,
                    clip_partial: false,
                    allow_stretch: false,
                    partial_projected: false,
                    use_symbol_color: true,
                    use_symbol_lineweight: true,
                    justify: 0,
                    rotation_type: 0,
                    vertex_mask: 0,
                    x_offset: 0.0,
                    y_offset: 0.0,
                    angle: 0.0,
                    stroke_number: 0,
                }],
            }),
        );
        let compound = component(
            &mut doc,
            DgnLsComponentType::Compound,
            "z1 compound",
            DgnLsComponentData::Compound(DgnLsCompoundComponent {
                entries: [stroke, internal, point]
                    .into_iter()
                    .map(|component| DgnLsCompoundEntry {
                        component,
                        offset: 0.0,
                    })
                    .collect(),
            }),
        );
        ls_object(
            &mut doc,
            DgnLineStyleData::Definition {
                description: "z1 definition".into(),
                version: 1,
                style_number: 1,
                component_uid: [9; 16],
                is_continuous: false,
                unit_definition: 1.0,
                unit_scale: 1.0,
                units_type: 0,
                is_element: false,
                is_physical: false,
                is_scale_independent: false,
                is_snappable: false,
                root_component: compound,
                properties: Vec::new(),
            },
        );
        doc
    }
}
