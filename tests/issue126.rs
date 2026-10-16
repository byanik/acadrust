use std::collections::BTreeMap;
use std::io::Cursor;

use acadrust::entities::{DimensionLinear, EntityType, MText, Polyline2D, PolylineFlags, Vertex2D};
use acadrust::types::{DxfVersion, LineWeight, Vector3};
use acadrust::{CadDocument, DxfReader, DxfWriter};

fn record_fields(dxf: &[u8], entity_type: &str) -> BTreeMap<i32, String> {
    let source = String::from_utf8(dxf.to_vec()).unwrap();
    let lines: Vec<_> = source.lines().map(str::trim).collect();
    let pairs: Vec<_> = lines
        .chunks_exact(2)
        .map(|p| (p[0].parse::<i32>().unwrap(), p[1]))
        .collect();
    let start = pairs
        .iter()
        .position(|pair| pair == &(0, entity_type))
        .unwrap();
    pairs[start + 1..]
        .iter()
        .take_while(|(code, _)| *code != 0)
        .map(|(code, value)| (*code, (*value).to_string()))
        .collect()
}

#[test]
fn dimension_writes_all_common_entity_properties_and_insertion_point() {
    let mut document = CadDocument::with_version(DxfVersion::AC1032);
    let mut dimension = DimensionLinear::new(Vector3::ZERO, Vector3::new(10.0, 0.0, 0.0));
    dimension.base.common.invisible = true;
    dimension.base.common.linetype = "DASHED".to_string();
    dimension.base.common.linetype_scale = 2.0;
    dimension.base.common.line_weight = LineWeight::Value(50);
    dimension.base.insertion_point = Vector3::new(100.0, 50.0, 0.0);
    document
        .add_entity(EntityType::Dimension(Box::new(
            acadrust::entities::Dimension::Linear(dimension),
        )))
        .unwrap();

    let fields = record_fields(
        &DxfWriter::new(&document).write_to_vec().unwrap(),
        "DIMENSION",
    );
    assert_eq!(fields[&6], "DASHED");
    assert!((fields[&48].parse::<f64>().unwrap() - 2.0).abs() < 1e-12);
    assert_eq!(fields[&60], "1");
    assert_eq!(fields[&370], "50");
    assert!((fields[&12].parse::<f64>().unwrap() - 100.0).abs() < 1e-12);
    assert!((fields[&22].parse::<f64>().unwrap() - 50.0).abs() < 1e-12);

    let restored = DxfReader::from_reader(Cursor::new(
        DxfWriter::new(&document).write_to_vec().unwrap(),
    ))
    .unwrap()
    .read()
    .unwrap();
    let EntityType::Dimension(dimension) = restored
        .entities()
        .find(|entity| matches!(entity, EntityType::Dimension(_)))
        .unwrap()
    else {
        unreachable!()
    };
    assert!(dimension.base().common.invisible);
    assert_eq!(dimension.base().common.linetype, "DASHED");
    assert_eq!(dimension.base().common.linetype_scale, 2.0);
    assert_eq!(dimension.base().common.line_weight, LineWeight::Value(50));
    assert_eq!(
        dimension.base().insertion_point,
        Vector3::new(100.0, 50.0, 0.0)
    );

    let dwg = acadrust::DwgWriter::write_to_vec(&document).unwrap();
    let from_dwg = acadrust::DwgReader::from_stream(Cursor::new(dwg))
        .read()
        .unwrap();
    let dimension = from_dwg
        .entities()
        .find_map(|entity| match entity {
            EntityType::Dimension(dimension) => Some(dimension),
            _ => None,
        })
        .unwrap();
    assert!(dimension.base().common.invisible);
    assert_eq!(
        dimension.base().insertion_point,
        Vector3::new(100.0, 50.0, 0.0)
    );
}

#[test]
fn heavy_polyline_writes_ocs_normal_and_mirrored_mtext_writes_wcs_direction() {
    let mut document = CadDocument::with_version(DxfVersion::AC1032);
    let mut polyline = Polyline2D::new();
    polyline.flags = PolylineFlags::CURVE_FIT;
    polyline.normal = Vector3::new(0.0, 0.0, -1.0);
    for point in [Vector3::new(1.0, 2.0, 0.0), Vector3::new(3.0, 4.0, 0.0)] {
        polyline.add_vertex(Vertex2D::new(point));
    }
    document
        .add_entity(EntityType::Polyline2D(polyline))
        .unwrap();

    let mut mtext = MText::with_value("mirrored", Vector3::ZERO);
    mtext.normal = Vector3::new(0.0, 0.0, -1.0);
    mtext.rotation = 30.0_f64.to_radians();
    document
        .add_entity(EntityType::MText(Box::new(mtext)))
        .unwrap();

    let dxf = DxfWriter::new(&document).write_to_vec().unwrap();
    let polyline_fields = record_fields(&dxf, "POLYLINE");
    assert_eq!(polyline_fields[&70], "2");
    assert!(polyline_fields[&210].parse::<f64>().unwrap().abs() < 1e-12);
    assert!((polyline_fields[&230].parse::<f64>().unwrap() + 1.0).abs() < 1e-12);

    let mtext_fields = record_fields(&dxf, "MTEXT");
    assert!(!mtext_fields.contains_key(&50));
    assert!(
        (mtext_fields[&11].parse::<f64>().unwrap() - 30.0_f64.to_radians().cos()).abs() < 1e-12
    );
    assert!(
        (mtext_fields[&21].parse::<f64>().unwrap() - 30.0_f64.to_radians().sin()).abs() < 1e-12
    );

    let restored = DxfReader::from_reader(Cursor::new(dxf))
        .unwrap()
        .read()
        .unwrap();
    let restored_mtext = restored
        .entities()
        .find_map(|entity| match entity {
            EntityType::MText(mtext) => Some(mtext),
            _ => None,
        })
        .unwrap();
    assert!((restored_mtext.rotation - 30.0_f64.to_radians()).abs() < 1e-12);
    assert_eq!(restored_mtext.normal, Vector3::new(0.0, 0.0, -1.0));

    let binary = DxfWriter::new_binary(&document).write_to_vec().unwrap();
    let binary_restored = DxfReader::from_reader(Cursor::new(binary))
        .unwrap()
        .read()
        .unwrap();
    let binary_mtext = binary_restored
        .entities()
        .find_map(|entity| match entity {
            EntityType::MText(mtext) => Some(mtext),
            _ => None,
        })
        .unwrap();
    assert!((binary_mtext.rotation - 30.0_f64.to_radians()).abs() < 1e-12);

    let dwg = acadrust::DwgWriter::write_to_vec(&document).unwrap();
    let from_dwg = acadrust::DwgReader::from_stream(Cursor::new(dwg))
        .read()
        .unwrap();
    let dxf_from_dwg = DxfWriter::new(&from_dwg).write_to_vec().unwrap();
    let from_dwg_dxf = DxfReader::from_reader(Cursor::new(dxf_from_dwg))
        .unwrap()
        .read()
        .unwrap();
    let dwg_mtext = from_dwg_dxf
        .entities()
        .find_map(|entity| match entity {
            EntityType::MText(mtext) => Some(mtext),
            _ => None,
        })
        .unwrap();
    assert!((dwg_mtext.rotation - 30.0_f64.to_radians()).abs() < 1e-12);
}
