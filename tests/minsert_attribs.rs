//! acadrust(I1): an attributed MINSERT read from an R2004+ DWG keeps its grid. The record stores the
//! owned object count (`BL`) after the has-attributes bit and before the column and row counts and
//! spacings; the reader left it out, so every R2004+ attributed MINSERT read its counts and spacings
//! shifted (spacing 0: all cells on one spot).

use std::io::Cursor;

use acadrust::entities::{AttributeEntity, Insert};
use acadrust::tables::BlockRecord;
use acadrust::types::{DxfVersion, Vector3};
use acadrust::{CadDocument, DwgReader, DwgWriter, EntityType};

fn grid(version: DxfVersion, attributed: bool) -> (u16, u16, f64, f64) {
    let mut doc = CadDocument::with_version(version);
    let mut block = BlockRecord::new("CELL");
    block.handle = doc.allocate_handle();
    doc.block_records.add(block).unwrap();
    let mut m = Insert::new("CELL", Vector3::new(300.0, 0.0, 0.0));
    m.column_count = 3;
    m.row_count = 2;
    m.column_spacing = 30.0;
    m.row_spacing = 20.0;
    m.mark_as_minsert();
    if attributed {
        m.attributes.push(AttributeEntity::simple("TAG", "value"));
    }
    doc.add_entity(EntityType::Insert(m)).unwrap();
    let back = DwgReader::from_stream(Cursor::new(DwgWriter::write_to_vec(&doc).unwrap()))
        .read()
        .unwrap();
    let found = back
        .entities()
        .find_map(|e| match e {
            EntityType::Insert(i) if i.block_name == "CELL" => {
                Some((i.column_count, i.row_count, i.column_spacing, i.row_spacing))
            }
            _ => None,
        })
        .expect("MINSERT read back");
    found
}

#[test]
fn an_attributed_minsert_keeps_its_grid_in_every_version() {
    for version in [
        DxfVersion::AC1015,
        DxfVersion::AC1018,
        DxfVersion::AC1024,
        DxfVersion::AC1027,
        DxfVersion::AC1032,
    ] {
        for attributed in [false, true] {
            assert_eq!(
                grid(version, attributed),
                (3, 2, 30.0, 20.0),
                "{version:?} attributed {attributed}"
            );
        }
    }
}
