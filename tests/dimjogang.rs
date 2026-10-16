use std::io::Cursor;
use acadrust::{CadDocument, DxfReader, DxfVersion, DxfWriter};

#[test]
fn dimjogang_writer_emits_bricscad_safe_lower_bound() {
    let mut document = CadDocument::with_version(DxfVersion::AC1015);
    let style = document.dim_styles.get_mut("Standard").unwrap();
    style.dimjogang = 0.0;
    let dxf = String::from_utf8(DxfWriter::new(&document).write_to_vec().unwrap()).unwrap();
    assert!(dxf.contains("5"), "DIMJOGANG was not emitted: {dxf}");
    let restored = DxfReader::from_reader(Cursor::new(dxf.into_bytes())).unwrap().read().unwrap();
    assert!((restored.dim_styles.get("Standard").unwrap().dimjogang - 5.0_f64.to_radians()).abs() < 1e-12);
}
