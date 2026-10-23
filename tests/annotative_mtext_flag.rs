//! acadrust(A5): an MTEXT with annotation scale contexts (its extension dictionary holds an
//! `AcDbContextDataManager`) is written to an R2018+ DWG as annotative, whatever `is_annotative`
//! says (AutoCAD 2025 read such an MTEXT written as "not annotative" with its height divided by the
//! default context's scale).

use std::io::Cursor;

use acadrust::entities::MText;
use acadrust::objects::{Dictionary, ObjectType};
use acadrust::types::{DxfVersion, Handle, Vector3};
use acadrust::{CadDocument, DwgReader, DwgWriter, EntityType};

fn dictionary(doc: &mut CadDocument, entries: &[(&str, Handle)]) -> Handle {
    let h = doc.allocate_handle();
    let mut d = Dictionary::new();
    d.handle = h;
    for (k, v) in entries {
        d.add_entry(*k, *v);
    }
    doc.objects.insert(h, ObjectType::Dictionary(d));
    h
}

fn mtext_flag(doc: &CadDocument) -> Vec<(String, bool, f64)> {
    let back = DwgReader::from_stream(Cursor::new(DwgWriter::write_to_vec(doc).unwrap()))
        .read()
        .unwrap();
    let mut v: Vec<_> = back
        .entities()
        .filter_map(|e| match e {
            EntityType::MText(m) => Some((m.value.clone(), m.is_annotative, m.height)),
            _ => None,
        })
        .collect();
    v.sort_by(|a, b| a.0.cmp(&b.0));
    v
}

#[test]
fn an_mtext_with_a_context_manager_is_written_annotative() {
    let mut doc = CadDocument::with_version(DxfVersion::AC1032);
    let mut plain = MText::with_value("A", Vector3::new(0.0, 0.0, 0.0));
    plain.height = 6.0;
    doc.add_entity(EntityType::MText(Box::new(plain))).unwrap();
    let mut anno = MText::with_value("B", Vector3::new(0.0, 10.0, 0.0));
    anno.height = 6.0;
    let h = doc.add_entity(EntityType::MText(Box::new(anno))).unwrap();
    let scales = dictionary(&mut doc, &[]);
    let manager = dictionary(&mut doc, &[("ACDB_ANNOTATIONSCALES", scales)]);
    let xdict = dictionary(&mut doc, &[("AcDbContextDataManager", manager)]);
    doc.get_entity_mut(h).unwrap().common_mut().xdictionary_handle = Some(xdict);
    let got = mtext_flag(&doc);
    assert_eq!(got, vec![("A".into(), false, 6.0), ("B".into(), true, 6.0)]);
}
