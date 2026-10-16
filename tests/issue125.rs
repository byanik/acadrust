use std::collections::BTreeMap;
use std::io::Cursor;

use acadrust::entities::{EntityType, Text};
use acadrust::{CadDocument, DwgReader, DwgWriter, DxfReader, DxfVersion, DxfWriter, Vector3};

const VERSIONS: [DxfVersion; 9] = [
    DxfVersion::AC1009,
    DxfVersion::AC1012,
    DxfVersion::AC1014,
    DxfVersion::AC1015,
    DxfVersion::AC1018,
    DxfVersion::AC1021,
    DxfVersion::AC1024,
    DxfVersion::AC1027,
    DxfVersion::AC1032,
];
const DWG_VERSIONS: [DxfVersion; 6] = [
    DxfVersion::AC1015,
    DxfVersion::AC1018,
    DxfVersion::AC1021,
    DxfVersion::AC1024,
    DxfVersion::AC1027,
    DxfVersion::AC1032,
];
const ANGLES: [(&str, f64); 3] = [("positive", 10.0), ("negative", -22.5), ("zero", 0.0)];

fn drawing(version: DxfVersion) -> CadDocument {
    let mut document = CadDocument::with_version(version);
    for (index, (label, degrees)) in ANGLES.iter().enumerate() {
        let mut text =
            Text::with_value(*label, Vector3::new(0.0, index as f64 * 5.0, 0.0)).with_height(2.5);
        text.oblique_angle = degrees.to_radians();
        document.add_entity(EntityType::Text(text)).unwrap();
    }
    document
}

fn assert_angles(document: &CadDocument) {
    let angles: BTreeMap<_, _> = document
        .entities()
        .filter_map(|entity| match entity {
            EntityType::Text(text) => Some((text.value.as_str(), text.oblique_angle)),
            _ => None,
        })
        .collect();
    assert_eq!(angles.len(), ANGLES.len(), "{:?}: missing TEXT entities", document.version);
    for (label, degrees) in ANGLES {
        assert!(
            (angles[label] - degrees.to_radians()).abs() < 1e-12,
            "{label}: expected {degrees} degrees, got {} radians",
            angles[label]
        );
    }
}

#[test]
fn text_oblique_group_51_uses_degrees_in_all_dxf_versions() {
    for version in VERSIONS {
        let document = drawing(version);
        let bytes = DxfWriter::new(&document).write_to_vec().unwrap();
        let text = String::from_utf8(bytes).unwrap();
        let lines: Vec<_> = text.lines().map(str::trim).collect();
        let pairs: Vec<_> = lines.chunks_exact(2).map(|p| (p[0], p[1])).collect();
        let mut records = BTreeMap::new();
        for (index, pair) in pairs.iter().enumerate() {
            if *pair != ("0", "TEXT") {
                continue;
            }
            let fields: BTreeMap<_, _> = pairs[index + 1..]
                .iter()
                .copied()
                .take_while(|(code, _)| *code != "0")
                .collect();
            records.insert(
                fields["1"],
                fields.get("51").map(|value| value.parse::<f64>().unwrap()),
            );
        }
        assert_eq!(records.len(), ANGLES.len());
        for (label, degrees) in ANGLES {
            if degrees == 0.0 {
                assert_eq!(
                    records[label], None,
                    "{version:?}: omit default oblique angle"
                );
            } else {
                assert!(
                    (records[label].unwrap() - degrees).abs() < 1e-12,
                    "{version:?} {label}: DXF group 51 must be in degrees"
                );
            }
        }
        assert_angles(&document);
    }
}

#[test]
fn text_oblique_survives_repeated_ascii_and_binary_dxf_roundtrips() {
    for version in VERSIONS {
        for binary in [false, true] {
            let mut document = drawing(version);
            for _ in 0..2 {
                let writer = if binary {
                    DxfWriter::new_binary(&document)
                } else {
                    DxfWriter::new(&document)
                };
                document = DxfReader::from_reader(Cursor::new(writer.write_to_vec().unwrap()))
                    .unwrap()
                    .read()
                    .unwrap();
                assert_angles(&document);
            }
        }
    }
}

#[test]
fn text_oblique_survives_dwg_to_dxf_conversion() {
    for version in DWG_VERSIONS {
        let bytes = DwgWriter::write_to_vec(&drawing(version)).unwrap();
        let document = DwgReader::from_stream(Cursor::new(bytes)).read().unwrap();
        assert_angles(&document);
        for writer in [DxfWriter::new(&document), DxfWriter::new_binary(&document)] {
            let restored = DxfReader::from_reader(Cursor::new(writer.write_to_vec().unwrap()))
                .unwrap()
                .read()
                .unwrap();
            assert_angles(&restored);
        }
    }
}
