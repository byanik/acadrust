use std::io::Cursor;

use acadrust::entities::{EntityType, Shape};
use acadrust::{CadDocument, DxfReader, DxfVersion, DxfWriter, Vector3};

fn document() -> CadDocument {
    let mut document = CadDocument::with_version(DxfVersion::AC1018);
    let mut shape = Shape::new();
    shape.shape_name = "TEST".to_string();
    shape.insertion_point = Vector3::new(1.0, 2.0, 0.0);
    shape.rotation = 30.0_f64.to_radians();
    shape.oblique_angle = -12.5_f64.to_radians();
    document.add_entity(EntityType::Shape(shape)).unwrap();
    document
}

fn assert_angles(document: &CadDocument) {
    let shape = document.entities().find_map(|entity| match entity {
        EntityType::Shape(shape) => Some(shape),
        _ => None,
    }).expect("SHAPE entity");
    assert!((shape.rotation - 30.0_f64.to_radians()).abs() < 1.0e-12);
    assert!((shape.oblique_angle - (-12.5_f64).to_radians()).abs() < 1.0e-12);
}

#[test]
fn shape_angles_round_trip_from_dxf_degrees() {
    for binary in [false, true] {
        let source = document();
        let writer = if binary {
            DxfWriter::new_binary(&source)
        } else {
            DxfWriter::new(&source)
        };
        let restored = DxfReader::from_reader(Cursor::new(writer.write_to_vec().unwrap()))
            .unwrap()
            .read()
            .unwrap();
        assert_angles(&restored);
    }
}
