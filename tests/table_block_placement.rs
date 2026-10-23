//! acadrust(T1): a TABLE read from DWG keeps its block reference part's rotation and scale
//! (`Table::dwg_block_placement`); a DXF table has none (ACAD_TABLE writes no rotation or scale).

use std::io::Cursor;

use acadrust::entities::table::Table;
use acadrust::types::{DxfVersion, Vector3};
use acadrust::{CadDocument, DwgReader, DwgWriter, DxfReader, DxfWriter, EntityType};

fn table_of(doc: &CadDocument) -> Table {
    doc.entities()
        .find_map(|e| match e {
            EntityType::Table(t) => Some((**t).clone()),
            _ => None,
        })
        .expect("TABLE missing")
}

#[test]
fn a_dwg_table_keeps_its_block_reference_placement_and_a_dxf_table_has_none() {
    let mut doc = CadDocument::with_version(DxfVersion::AC1032);
    let mut t = Table::new(Vector3::new(5.0, 6.0, 0.0), 2, 2);
    t.horizontal_direction = Vector3::new(0.5, 0.75_f64.sqrt(), 0.0);
    doc.add_entity(EntityType::Table(Box::new(t))).unwrap();
    let dwg = DwgReader::from_stream(Cursor::new(DwgWriter::write_to_vec(&doc).unwrap()))
        .read()
        .unwrap();
    let (rotation, scale) = table_of(&dwg).dwg_block_placement.expect("DWG placement");
    // the writer stores the reference unturned and unscaled; the direction stays the table's own
    assert_eq!(rotation, 0.0);
    assert_eq!((scale.x, scale.y, scale.z), (1.0, 1.0, 1.0));
    assert!((table_of(&dwg).horizontal_direction.x - 0.5).abs() < 1e-12);
    let dxf = DxfReader::from_reader(Cursor::new(DxfWriter::new(&doc).write_to_vec().unwrap()))
        .unwrap()
        .read()
        .unwrap();
    assert_eq!(table_of(&dxf).dwg_block_placement, None);
}
