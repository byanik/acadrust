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

use acadrust::entities::{Viewport, ViewportSource};

/// Two paper-space viewports: id 2 has the "off" bit 0x20000 (0xE8020), id 3 does not (0xC8020).
const VIEWPORTS_DXF: &str = "0\nSECTION\n2\nHEADER\n9\n$ACADVER\n1\nAC1032\n0\nENDSEC\n\
0\nSECTION\n2\nENTITIES\n\
0\nVIEWPORT\n5\n1F\n100\nAcDbEntity\n67\n1\n8\n0\n100\nAcDbViewport\n10\n5.0\n20\n5.0\n30\n0.0\n40\n10.0\n41\n8.0\n68\n1\n69\n2\n90\n950304\n\
0\nVIEWPORT\n5\n20\n100\nAcDbEntity\n67\n1\n8\n0\n100\nAcDbViewport\n10\n20.0\n20\n5.0\n30\n0.0\n40\n10.0\n41\n8.0\n68\n1\n69\n3\n90\n819232\n\
0\nENDSEC\n0\nEOF\n";

fn viewports(doc: &CadDocument) -> Vec<Viewport> {
    doc.entities()
        .filter_map(|e| if let EntityType::Viewport(v) = e { Some(v.as_ref().clone()) } else { None })
        .collect()
}

#[test]
fn f4_dxf_keeps_the_raw_off_bit() {
    let doc = DxfReader::from_reader(Cursor::new(VIEWPORTS_DXF.as_bytes().to_vec()))
        .expect("dxf reader")
        .read()
        .expect("dxf read");
    let vps = viewports(&doc);
    let off = vps.iter().find(|v| v.id == 2).expect("viewport 2");
    let on = vps.iter().find(|v| v.id == 3).expect("viewport 3");
    assert_eq!(off.status_raw, 0xE8020);
    assert!(off.is_off(ViewportSource::Dxf));
    assert_eq!(on.status_raw, 0xC8020);
    assert!(!on.is_off(ViewportSource::Dxf));
    assert!(off.status.is_on && on.status.is_on, "0x8000 is set on both");
}

#[test]
fn f4_dwg_viewport_is_on_and_keeps_raw_bits() {
    let mut doc = CadDocument::with_version(DxfVersion::AC1032);
    doc.add_paper_space_entity(EntityType::Viewport(Box::new(Viewport::new()))).expect("viewport");
    let bytes = DwgWriter::write_to_vec(&doc).expect("write dwg");
    let back = DwgReader::from_stream(Cursor::new(bytes)).read().expect("read dwg");
    let vps = viewports(&back);
    assert!(!vps.is_empty());
    for v in &vps {
        assert_ne!(v.status_raw & 0x8000, 0, "raw status keeps 0x8000");
        assert!(!v.is_off(ViewportSource::DwgR2000Plus));
    }
}

/// id 4: group 68 == 0 switches it off although group 90 has no off bit; id 5: no group 90 at all.
const VIEWPORTS_68_DXF: &str = "0\nSECTION\n2\nHEADER\n9\n$ACADVER\n1\nAC1032\n0\nENDSEC\n\
0\nSECTION\n2\nENTITIES\n\
0\nVIEWPORT\n5\n1F\n100\nAcDbEntity\n67\n1\n8\n0\n100\nAcDbViewport\n10\n5.0\n20\n5.0\n30\n0.0\n40\n10.0\n41\n8.0\n68\n0\n69\n4\n90\n32768\n\
0\nVIEWPORT\n5\n20\n100\nAcDbEntity\n67\n1\n8\n0\n100\nAcDbViewport\n10\n20.0\n20\n5.0\n30\n0.0\n40\n10.0\n41\n8.0\n68\n1\n69\n5\n\
0\nENDSEC\n0\nEOF\n";

#[test]
fn f4_dxf_group_68_zero_is_off_and_a_missing_group_90_keeps_the_default_word() {
    let doc = DxfReader::from_reader(Cursor::new(VIEWPORTS_68_DXF.as_bytes().to_vec()))
        .expect("dxf reader")
        .read()
        .expect("dxf read");
    let vps = viewports(&doc);
    let by_68 = vps.iter().find(|v| v.id == 4).expect("viewport 4");
    let no_90 = vps.iter().find(|v| v.id == 5).expect("viewport 5");
    assert_eq!(by_68.status_raw, 0x8000);
    assert_eq!(by_68.dxf_status, Some(0));
    assert!(by_68.is_off(ViewportSource::Dxf));
    assert!(!by_68.is_off(ViewportSource::DwgR2000Plus), "the DWG rule looks at the raw word only");
    assert_eq!(no_90.status_raw, 0x8000);
    assert!(!no_90.is_off(ViewportSource::Dxf));
}

#[test]
fn f4_dwg_raw_status_is_the_word_the_file_holds() {
    let mut vp = Viewport::new();
    vp.status.locked = true;
    vp.status.grid_on = true;
    vp.status.non_rectangular_clipping = true;
    let word = vp.status.to_bits();
    assert_eq!(word, 0x1_C200, "non-rectangular 0x10000 | on 0x8000 | locked 0x4000 | grid 0x200");
    for version in [DxfVersion::AC1015, DxfVersion::AC1018, DxfVersion::AC1032] {
        let mut doc = CadDocument::with_version(version);
        doc.add_paper_space_entity(EntityType::Viewport(Box::new(vp.clone()))).expect("viewport");
        let bytes = DwgWriter::write_to_vec(&doc).expect("write dwg");
        let back = DwgReader::from_stream(Cursor::new(bytes)).read().expect("read dwg");
        let found = viewports(&back).into_iter().find(|v| v.status_raw == word);
        let found = found.unwrap_or_else(|| panic!("{version:?}: a viewport carries the stored word"));
        assert_eq!(found.status.to_bits(), word, "{version:?}: decoded flags and raw word agree");
        assert!(found.status.locked && found.status.grid_on && found.status.non_rectangular_clipping);
        assert!(!found.is_off(ViewportSource::DwgR2000Plus), "{version:?}");
    }
}

#[test]
fn f4_dwg_r13_r14_off_comes_from_the_vx_table_and_the_mview_word_keeps_0x8000() {
    let mut on = Viewport::new();
    on.view_height = 111.0;
    let mut off = Viewport::new();
    off.view_height = 222.0;
    off.status.is_on = false;
    let mut doc = CadDocument::with_version(DxfVersion::AC1014);
    doc.add_paper_space_entity(EntityType::Viewport(Box::new(on))).expect("viewport");
    doc.add_paper_space_entity(EntityType::Viewport(Box::new(off))).expect("viewport");
    let bytes = DwgWriter::write_to_vec(&doc).expect("write dwg");
    let back = DwgReader::from_stream(Cursor::new(bytes)).read().expect("read dwg");
    let vps = viewports(&back);
    let on = vps.iter().find(|v| v.view_height == 111.0).expect("the viewport that is on");
    let off = vps.iter().find(|v| v.view_height == 222.0).expect("the viewport that is off");
    assert!(!on.is_off(ViewportSource::DwgR13R14));
    assert!(off.is_off(ViewportSource::DwgR13R14));
    assert_ne!(on.status_raw & 0x8000, 0, "the MVIEW restore keeps the 0x8000 bit in the raw word");
    assert_eq!(on.status_raw & 0x20000, 0);
}
