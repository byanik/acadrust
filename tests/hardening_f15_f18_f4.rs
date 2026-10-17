//! Fork patches F15 (visit with stats), F18 (canonical block membership) and
//! F4 (raw viewport status bits). Inputs come from the fork's own writer.

use std::io::Cursor;

use acadrust::entities::{Circle, EntityType, Line};
use acadrust::types::DxfVersion;
use acadrust::{CadDocument, DwgReader, DwgWriter};

fn flat_sample(version: DxfVersion) -> Vec<u8> {
    let mut doc = CadDocument::with_version(version);
    for i in 0..10 {
        let x = f64::from(i);
        doc.add_entity(EntityType::Line(Line::from_coords(x, 0.0, 0.0, x, 5.0, 0.0))).expect("line");
    }
    doc.add_entity(EntityType::Circle(Circle::from_coords(3.0, 3.0, 0.0, 1.0))).expect("circle");
    DwgWriter::write_to_vec(&doc).expect("write dwg")
}

#[test]
fn f15_visit_with_stats_matches_full_read_stats() {
    for version in [DxfVersion::AC1018, DxfVersion::AC1032] {
        let bytes = flat_sample(version);
        let full = DwgReader::from_stream(Cursor::new(bytes.clone())).read_with_stats().expect("full read");
        let mut seen = 0usize;
        let visit = DwgReader::from_stream(Cursor::new(bytes))
            .read_visiting_with_stats(|_, e| {
                if !matches!(e, EntityType::Block(_) | EntityType::BlockEnd(_)) {
                    seen += 1;
                }
                Some(e)
            })
            .expect("visit read");
        assert!(full.stats.source_records > 0, "{version:?}: the sample has records");
        assert_eq!(visit.stats.source_records, full.stats.source_records, "{version:?}");
        assert_eq!(visit.stats.decoded_source_records, full.stats.decoded_source_records, "{version:?}");
        assert_eq!(visit.stats.skipped_source_records, full.stats.skipped_source_records, "{version:?}");
        assert_eq!(seen, full.document.entities().count(), "visitor sees every entity once");
    }
}

use acadrust::entities::Insert;
use acadrust::tables::BlockRecord;
use acadrust::types::{Handle, Vector3};

fn block(doc: &mut CadDocument, name: &str) -> Handle {
    let mut br = BlockRecord::new(name);
    br.handle = doc.allocate_handle();
    br.block_entity_handle = doc.allocate_handle();
    br.block_end_handle = doc.allocate_handle();
    let h = br.handle;
    doc.block_records.add(br).expect("block record");
    h
}

/// Model space → 4 × INSERT OUTER + LINE; OUTER → 2 LINE + INSERT INNER; INNER → 3 CIRCLE.
fn nested_sample(version: DxfVersion) -> Vec<u8> {
    let mut doc = CadDocument::with_version(version);
    let inner = block(&mut doc, "INNER");
    let outer = block(&mut doc, "OUTER");
    for i in 0..3 {
        let mut e = EntityType::Circle(Circle::from_coords(f64::from(i), 0.0, 0.0, 0.5));
        e.common_mut().owner_handle = inner;
        doc.add_entity(e).expect("circle");
    }
    for i in 0..2 {
        let y = f64::from(i);
        let mut e = EntityType::Line(Line::from_coords(0.0, y, 0.0, 1.0, y, 0.0));
        e.common_mut().owner_handle = outer;
        doc.add_entity(e).expect("line");
    }
    let mut ins = EntityType::Insert(Insert::new("INNER", Vector3::new(5.0, 0.0, 0.0)));
    ins.common_mut().owner_handle = outer;
    doc.add_entity(ins).expect("insert");
    for i in 0..4 {
        let p = Vector3::new(0.0, 10.0 * f64::from(i), 0.0);
        doc.add_entity(EntityType::Insert(Insert::new("OUTER", p))).expect("insert");
    }
    doc.add_entity(EntityType::Line(Line::from_coords(-1.0, -1.0, 0.0, -2.0, -2.0, 0.0))).expect("line");
    DwgWriter::write_to_vec(&doc).expect("write dwg")
}

#[test]
fn f18_canonical_membership_survives_a_consuming_visitor() {
    for version in [DxfVersion::AC1018, DxfVersion::AC1032] {
        let bytes = nested_sample(version);
        let full = DwgReader::from_stream(Cursor::new(bytes.clone())).read().expect("full read");
        let visit = DwgReader::from_stream(Cursor::new(bytes))
            .read_visiting_with_stats(|_, e| match e {
                EntityType::Block(_) | EntityType::BlockEnd(_) => Some(e),
                _ => None,
            })
            .expect("visit read")
            .document;
        assert_eq!(visit.entities().count(), 0, "visitor consumed every entity");
        let mut checked = 0;
        for br in full.block_records.iter() {
            let canon_full = full.canonical_block_entities(br.handle);
            let canon_visit = visit.canonical_block_entities(br.handle);
            assert_eq!(canon_full, canon_visit, "{version:?} {}", br.name);
            let vbr = visit.block_records.iter().find(|b| b.handle == br.handle).expect("same block records");
            assert!(vbr.entity_handles.is_empty(), "visit mode loses entity_handles (V1)");
            let Some(canon) = canon_visit.filter(|c| !c.is_empty()) else { continue };
            let members: Vec<Handle> = canon
                .iter()
                .copied()
                .filter(|h| {
                    full.get_entity(*h).is_some_and(|e| {
                        !matches!(e, EntityType::Block(_) | EntityType::BlockEnd(_) | EntityType::AttributeEntity(_))
                    })
                })
                .collect();
            assert_eq!(members, br.entity_handles, "{version:?} {}: same set and order", br.name);
            if !members.is_empty() {
                checked += 1;
            }
        }
        assert!(checked >= 2, "{version:?}: OUTER and INNER must have canonical members");
    }
}

use acadrust::DxfReader;

const LINE_DXF: &str = "0\nSECTION\n2\nHEADER\n9\n$ACADVER\n1\nAC1032\n0\nENDSEC\n\
0\nSECTION\n2\nENTITIES\n\
0\nLINE\n5\n1F\n100\nAcDbEntity\n8\n0\n100\nAcDbLine\n10\n0.0\n20\n0.0\n30\n0.0\n11\n1.0\n21\n1.0\n31\n0.0\n\
0\nENDSEC\n0\nEOF\n";

#[test]
fn f18_is_none_for_documents_not_read_from_dwg() {
    let fresh = CadDocument::with_version(DxfVersion::AC1032);
    let dxf = DxfReader::from_reader(Cursor::new(LINE_DXF.as_bytes().to_vec()))
        .expect("dxf reader")
        .read()
        .expect("dxf read");
    assert_eq!(dxf.entities().count(), 1, "the DXF sample has its LINE");
    for (name, doc) in [("new", &fresh), ("dxf", &dxf)] {
        assert!(doc.block_records.iter().count() >= 2, "{name}: model and paper space records exist");
        for br in doc.block_records.iter() {
            assert!(doc.canonical_block_entities(br.handle).is_none(), "{name} {}", br.name);
        }
    }
    assert!(fresh.canonical_block_entities(Handle::new(0xDEAD_BEEF)).is_none(), "an unknown handle");
}
