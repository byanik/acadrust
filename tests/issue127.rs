//! Regression coverage for issue #127: drawing settings survive DXF output.

use std::io::Cursor;

use acadrust::{CadDocument, DwgReader, DwgWriter, DxfReader, DxfVersion, DxfWriter};

fn assert_settings(document: &CadDocument) {
    let header = &document.header;
    assert!((header.angle_base - std::f64::consts::FRAC_PI_2).abs() < 1e-12);
    assert_eq!(header.angle_direction, 1);
    assert_eq!(header.fillet_radius, 2.5);
    assert_eq!(header.chamfer_distance_a, 1.0);
    assert_eq!(header.chamfer_distance_b, 2.0);
    assert_eq!(header.elevation, -3.0);
    assert_eq!(header.thickness, 4.0);
}

#[test]
fn drawing_settings_survive_ascii_binary_and_dwg_to_dxf() {
    for version in [
        DxfVersion::AC1009,
        DxfVersion::AC1012,
        DxfVersion::AC1014,
        DxfVersion::AC1015,
        DxfVersion::AC1018,
        DxfVersion::AC1021,
        DxfVersion::AC1024,
        DxfVersion::AC1027,
        DxfVersion::AC1032,
    ] {
        let mut document = CadDocument::with_version(version);
        let header = &mut document.header;
        header.angle_base = std::f64::consts::FRAC_PI_2;
        header.angle_direction = 1;
        header.fillet_radius = 2.5;
        header.chamfer_distance_a = 1.0;
        header.chamfer_distance_b = 2.0;
        header.elevation = -3.0;
        header.thickness = 4.0;
        let ascii = DxfWriter::new(&document).write_to_vec().unwrap();
        let source = String::from_utf8(ascii.clone()).unwrap();
        let lines: Vec<_> = source.lines().map(str::trim).collect();
        for (name, code, value) in [
            ("$ANGBASE", "50", 90.0),
            ("$ANGDIR", "70", 1.0),
            ("$FILLETRAD", "40", 2.5),
            ("$CHAMFERA", "40", 1.0),
            ("$CHAMFERB", "40", 2.0),
            ("$ELEVATION", "40", -3.0),
            ("$THICKNESS", "40", 4.0),
        ] {
            let index = lines.iter().position(|line| *line == name).unwrap();
            assert_eq!(lines[index + 1], code, "{version:?} {name}");
            assert_eq!(lines[index + 2].parse::<f64>().unwrap(), value);
        }
        for bytes in [
            ascii,
            DxfWriter::new_binary(&document).write_to_vec().unwrap(),
        ] {
            let restored = DxfReader::from_reader(Cursor::new(bytes))
                .unwrap()
                .read()
                .unwrap();
            assert_settings(&restored);
        }
        let dwg = DwgWriter::write_to_vec(&document).unwrap();
        let restored = DwgReader::from_stream(Cursor::new(dwg)).read().unwrap();
        let dxf = DxfWriter::new(&restored).write_to_vec().unwrap();
        let converted = DxfReader::from_reader(Cursor::new(dxf))
            .unwrap()
            .read()
            .unwrap();
        assert_settings(&converted);
    }
}

#[test]
fn external_dxf_angle_base_is_in_degrees() {
    let dxf =
        b"0\nSECTION\n2\nHEADER\n9\n$ACADVER\n1\nAC1015\n9\n$ANGBASE\n50\n90\n0\nENDSEC\n0\nEOF\n";
    let document = DxfReader::from_reader(Cursor::new(dxf))
        .unwrap()
        .read()
        .unwrap();
    assert!((document.header.angle_base - std::f64::consts::FRAC_PI_2).abs() < 1e-12);
}

#[test]
fn active_viewport_does_not_mask_header_elevation_in_cad_hosts() {
    for version in [
        DxfVersion::AC1015,
        DxfVersion::AC1018,
        DxfVersion::AC1024,
        DxfVersion::AC1032,
    ] {
        for viewport_elevation in [0.0, 7.0] {
            let mut document = CadDocument::with_version(version);
            document.header.elevation = -3.0;
            document.vports.get_mut("*ACTIVE").unwrap().ucs_elevation = viewport_elevation;
            let expected = if viewport_elevation == 0.0 { -3.0 } else { 7.0 };
            let dwg =
                DwgReader::from_stream(Cursor::new(DwgWriter::write_to_vec(&document).unwrap()))
                    .read()
                    .unwrap();
            assert_eq!(dwg.vports.get("*ACTIVE").unwrap().ucs_elevation, expected);
            for writer in [DxfWriter::new(&document), DxfWriter::new_binary(&document)] {
                let loaded = DxfReader::from_reader(Cursor::new(writer.write_to_vec().unwrap()))
                    .unwrap()
                    .read()
                    .unwrap();
                assert_eq!(
                    loaded.vports.get("*ACTIVE").unwrap().ucs_elevation,
                    expected
                );
            }
            assert_eq!(
                document.vports.get("*ACTIVE").unwrap().ucs_elevation,
                viewport_elevation
            );
        }
    }
}
