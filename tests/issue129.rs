use std::io::Cursor;

use acadrust::entities::acis::primitives::build_box;
use acadrust::entities::solid3d::{AcisData, AcisRevision, Body, Region, Solid3D, Wire};
use acadrust::entities::surface::{Surface, SurfaceKind};
use acadrust::entities::EntityType;
use acadrust::io::dwg::dwg_stream_readers::bit_reader::DwgBitReader;
use acadrust::io::dwg::dwg_stream_readers::merged_reader::DwgMergedReader;
use acadrust::io::dwg::dwg_stream_readers::object_reader::entities::{
    read_acis_entity, read_surface,
};
use acadrust::io::dwg::dwg_stream_writers::bit_writer::DwgBitWriter;
use acadrust::io::dwg::DwgVersion;
use acadrust::types::{Color, DxfVersion, Vector3};
use acadrust::{CadDocument, DwgReadOptions, DwgReader, DwgWriter, SabReader, SabWriter};

const VERSIONS: [DxfVersion; 8] = [
    DxfVersion::AC1012,
    DxfVersion::AC1014,
    DxfVersion::AC1015,
    DxfVersion::AC1018,
    DxfVersion::AC1021,
    DxfVersion::AC1024,
    DxfVersion::AC1027,
    DxfVersion::AC1032,
];

fn sab() -> Vec<u8> {
    SabWriter::write(&build_box([10.0, 20.0, -5.0], 2.0, 3.0, 4.0))
}

// Encode the wire layout independently of DwgObjectWriter, particularly its
// BS color field, including both compact and full-width BS representations.
fn modeler_record(
    dxf_version: DxfVersion,
    bit_offset: i64,
    color: i16,
    full_width_color: bool,
    point_count: i32,
    surface: bool,
) -> (DwgMergedReader, Vec<u8>, i64) {
    modeler_record_with_silhouette(
        dxf_version,
        bit_offset,
        color,
        full_width_color,
        point_count,
        surface,
        None,
    )
}

fn modeler_record_with_silhouette(
    dxf_version: DxfVersion,
    bit_offset: i64,
    color: i16,
    full_width_color: bool,
    point_count: i32,
    surface: bool,
    silhouette_viewport: Option<i64>,
) -> (DwgMergedReader, Vec<u8>, i64) {
    let version = DwgVersion::from_dxf_version(dxf_version).unwrap();
    let mut writer = DwgBitWriter::new(version, dxf_version);
    for _ in 0..bit_offset {
        writer.write_bit(true);
    }
    let geometry = sab();
    if !version.r2013_plus(dxf_version) {
        writer.write_bit(false); // nonempty inline ACIS body
        writer.write_bit(false);
        writer.write_bit_short(2);
        writer.write_bytes(&geometry);
    }
    writer.write_bit(true); // wireframe cache
    writer.write_bit(true); // reference point
    writer.write_3bit_double(Vector3::new(10.0, 20.0, -5.0));
    writer.write_bit_long(2); // isolines
    writer.write_bit(true); // wire list
    writer.write_bit_long(1);
    writer.write_byte(2); // visible edge
    writer.write_bit_long(-1); // selection marker
    if full_width_color {
        writer.write_2bits(0);
        writer.write_raw_short(color);
    } else {
        writer.write_bit_short(color);
    }
    writer.write_bit_long(0x80FA); // ACIS index
    writer.write_bit_long(point_count);
    if point_count == 2 {
        writer.write_3bit_double(Vector3::ZERO);
        writer.write_3bit_double(Vector3::new(10.0, 20.0, -5.0));
    }
    writer.write_bit(false); // wire transform
    if let Some(viewport_id) = silhouette_viewport {
        writer.write_bit_long(1); // silhouettes
        if version.r2013_plus(dxf_version) {
            writer.write_bit_long_long(viewport_id);
        } else {
            writer.write_bit_long(viewport_id as i32);
        }
        writer.write_3bit_double(Vector3::ZERO); // target
        writer.write_3bit_double(Vector3::UNIT_Z); // view direction
        writer.write_3bit_double(Vector3::UNIT_Y); // up vector
        writer.write_bit(false); // perspective
        writer.write_bit(false); // no silhouette wires
    } else {
        writer.write_bit_long(0); // silhouettes
    }
    writer.write_bit(true); // no extra ACIS body
    if version.r2007_plus() {
        writer.write_bit_long(0); // materials / AcDs trailing BL
    }
    if version.r2013_plus(dxf_version) {
        writer.write_bit(true);
        writer.write_bit_long(0x12345678);
        writer.write_bit_short(0x1234);
        writer.write_bit_short(0x5678);
        writer.write_bytes(&[1, 2, 3, 4, 5, 6, 7, 8]);
        writer.write_bit_long(0x87654321_u32 as i32);
    }
    if surface {
        writer.write_bit_short(17);
        writer.write_bit_short(29);
    }
    let end = writer.position_in_bits();
    let main = DwgBitReader::new(writer.into_bytes(), version, dxf_version);
    let mut reader = DwgMergedReader::from_readers(main, None, None, dxf_version);
    reader.set_handle_start(end);
    reader.set_main_data_end(end);
    reader.set_position_in_bits(bit_offset);
    (reader, geometry, end)
}

#[test]
fn native_bit_short_wire_colors_preserve_counts_geometry_and_tail() {
    for dxf_version in VERSIONS {
        let version = DwgVersion::from_dxf_version(dxf_version).unwrap();
        for bit_offset in 0..8 {
            for (color, full_width) in [
                (0, false),
                (1, false),
                (255, false),
                (256, false),
                (0, true),
                (1, true),
                (255, true),
                (256, true),
            ] {
                let (mut reader, geometry, end) =
                    modeler_record(dxf_version, bit_offset, color, full_width, 2, false);
                let body = read_acis_entity(
                    &mut reader,
                    version,
                    dxf_version,
                    version.r2013_plus(dxf_version),
                );
                reader.check_error().unwrap_or_else(|error| {
                    panic!("{dxf_version:?}, offset {bit_offset}, color {color}: {error}")
                });
                assert_eq!(reader.position_in_bits(), end);
                if !version.r2013_plus(dxf_version) {
                    assert_eq!(body.sab_data, geometry);
                }
                assert_eq!(body.point, Vector3::new(10.0, 20.0, -5.0));
                assert_eq!(body.isolines, 2);
                assert_eq!(body.wires.len(), 1);
                assert_eq!(body.wires[0].selection_marker, -1);
                assert_eq!(body.wires[0].acis_index, 0x80FA);
                assert_eq!(body.wires[0].points.len(), 2);
                assert_eq!(
                    body.wires[0].color,
                    match color {
                        0 => Color::ByBlock,
                        256 => Color::ByLayer,
                        index => Color::Index(index as u8),
                    }
                );
                if version.r2013_plus(dxf_version) {
                    assert_eq!(
                        body.revision,
                        AcisRevision {
                            has_guid: true,
                            major: 0x12345678,
                            minor1: 0x1234,
                            minor2: 0x5678,
                            bytes: [1, 2, 3, 4, 5, 6, 7, 8],
                            end_marker: 0x87654321,
                        }
                    );
                }
            }
        }
    }
}

#[test]
fn silhouette_viewport_width_matches_the_dwg_generation() {
    for dxf_version in [DxfVersion::AC1018, DxfVersion::AC1032] {
        let version = DwgVersion::from_dxf_version(dxf_version).unwrap();
        let viewport_id = 0x1234_i64;
        let (mut reader, _, end) = modeler_record_with_silhouette(
            dxf_version,
            3,
            256,
            true,
            2,
            false,
            Some(viewport_id),
        );
        let body = read_acis_entity(
            &mut reader,
            version,
            dxf_version,
            version.r2013_plus(dxf_version),
        );
        reader.check_error().unwrap();
        assert_eq!(reader.position_in_bits(), end);
        assert_eq!(body.silhouettes.len(), 1);
        assert_eq!(body.silhouettes[0].viewport_id, viewport_id);
        assert!(!body.silhouettes[0].has_wires);
    }
}

#[test]
fn surface_fields_remain_aligned_after_bylayer_wire() {
    for dxf_version in [DxfVersion::AC1018, DxfVersion::AC1032] {
        let version = DwgVersion::from_dxf_version(dxf_version).unwrap();
        let (mut reader, geometry, end) = modeler_record(dxf_version, 3, 256, true, 2, true);
        let surface = read_surface(
            &mut reader,
            version,
            dxf_version,
            version.r2013_plus(dxf_version),
            SurfaceKind::Generic,
        );
        reader.check_error().unwrap();
        assert_eq!(reader.position_in_bits(), end);
        assert_eq!(surface.u_isolines, 17);
        assert_eq!(surface.v_isolines, 29);
        assert_eq!(surface.acis.wires[0].color, Color::ByLayer);
        if !version.r2013_plus(dxf_version) {
            assert_eq!(surface.acis.sab_data, geometry);
        }
    }
}

#[test]
fn malformed_wire_counts_are_still_rejected() {
    for dxf_version in [DxfVersion::AC1018, DxfVersion::AC1032] {
        let version = DwgVersion::from_dxf_version(dxf_version).unwrap();
        for point_count in [-1, i32::MIN, i32::MAX] {
            let (mut reader, _, _) = modeler_record(dxf_version, 5, 256, true, point_count, false);
            read_acis_entity(
                &mut reader,
                version,
                dxf_version,
                version.r2013_plus(dxf_version),
            );
            let error = reader.check_error().unwrap_err().to_string();
            assert!(error.contains("Invalid DWG array count"), "{error}");
        }
    }
}

#[test]
fn modeler_entities_survive_strict_and_failsafe_file_reading() {
    for dxf_version in VERSIONS {
        let mut document = CadDocument::with_version(dxf_version);
        let geometry = AcisData::from_sab(sab());
        let mut solid = Solid3D::from_sab(geometry.sab_data.clone());
        let mut region = Region::new();
        let mut body = Body::new();
        region.acis_data = geometry.clone();
        body.acis_data = geometry.clone();
        let wire = Wire {
            color: Color::ByLayer,
            acis_index: 0x80FA,
            points: vec![Vector3::ZERO, Vector3::new(10.0, 20.0, -5.0)],
            ..Wire::default()
        };
        solid.wires.push(wire.clone());
        region.wires.push(wire.clone());
        body.wires.push(wire.clone());
        document.add_entity(EntityType::Solid3D(solid)).unwrap();
        document.add_entity(EntityType::Region(region)).unwrap();
        document.add_entity(EntityType::Body(body)).unwrap();
        if dxf_version >= DxfVersion::AC1021 {
            let mut surface = Surface::new(SurfaceKind::Generic);
            surface.acis_data = geometry.clone();
            surface.wires.push(wire);
            surface.u_isolines = 17;
            surface.v_isolines = 29;
            document
                .add_entity(EntityType::Surface(Box::new(surface)))
                .unwrap();
        }
        let bytes = DwgWriter::write_to_vec(&document).unwrap();
        for options in [DwgReadOptions::default(), DwgReadOptions::failsafe()] {
            let mut reader = DwgReader::from_stream_with_options(Cursor::new(&bytes), options);
            let outcome = reader.read_with_stats().unwrap();
            assert!(
                outcome.stats.diagnostics.is_empty(),
                "{dxf_version:?}: {:?}",
                outcome.stats.diagnostics
            );
            assert_eq!(
                outcome.document.entities().count(),
                document.entities().count()
            );
            for entity in outcome.document.entities() {
                let acis = match entity {
                    EntityType::Solid3D(value) => &value.acis_data,
                    EntityType::Region(value) => &value.acis_data,
                    EntityType::Body(value) => &value.acis_data,
                    EntityType::Surface(value) => {
                        assert_eq!(value.u_isolines, 17);
                        assert_eq!(value.v_isolines, 29);
                        &value.acis_data
                    }
                    other => panic!("unexpected entity {other:?}"),
                };
                assert!(acis.has_data(), "{dxf_version:?} lost its modeler body");
                let parsed = if acis.is_binary {
                    SabReader::read(&acis.sab_data).unwrap()
                } else {
                    acis.parse_sat().unwrap()
                };
                assert_eq!(parsed.bodies().len(), 1);
            }
        }
    }
}

#[test]
fn supplied_issue129_samples_keep_modeler_entities() {
    let root = std::path::Path::new("issuesamples");
    if !root.is_dir() {
        return;
    }
    let mut files: Vec<_> = std::fs::read_dir(root)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| path.extension().is_some_and(|extension| extension.eq_ignore_ascii_case("dwg")))
        .collect();
    files.sort();
    assert_eq!(files.len(), 6);
    for path in files {
        let mut strict = DwgReader::from_file(&path).unwrap();
        let document = strict.read().unwrap_or_else(|error| panic!("{}: {error}", path.display()));
        let name = path.file_name().unwrap().to_string_lossy();
        if name == "NECG76A.dwg" {
            let surfaces = document.entities().filter(|entity| matches!(entity, EntityType::Surface(_))).count();
            assert_eq!(surfaces, 28, "{}", path.display());
        } else {
            let expected = match name.as_ref() {
                "NEBE72GL.dwg" => 7,
                "NEBE74GL.dwg" => 9,
                "NECI94GE.dwg" => 5,
                "NEFE73210.dwg" => 14,
                "NEFE74215.dwg" => 15,
                _ => unreachable!(),
            };
            let solids = document.entities().filter(|entity| matches!(entity, EntityType::Solid3D(_))).count();
            assert_eq!(solids, expected, "{}", path.display());
        }
    }
}
