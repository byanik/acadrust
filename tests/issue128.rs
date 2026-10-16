//! Regression coverage for issue #128: plain DWG dashes stay simple in DXF.

use std::io::Cursor;

use acadrust::tables::LineType;
use acadrust::{CadDocument, DwgReader, DwgWriter, DxfReader, DxfVersion, DxfWriter};

#[test]
fn plain_dash_dot_and_gap_elements_survive_dwg_and_dxf() {
    for version in [
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
        let mut linetype = LineType::dashed();
        linetype
            .elements
            .push(acadrust::tables::linetype::LineTypeElement::dot());
        document.line_types.add(linetype.clone()).unwrap();
        let dwg = DwgWriter::write_to_vec(&document).unwrap();
        let restored = DwgReader::from_stream(Cursor::new(dwg)).read().unwrap();
        let dashes = restored.line_types.get("Dashed").unwrap();
        assert_eq!(dashes.elements, linetype.elements, "{version:?}");
        let dxf = DxfWriter::new(&restored).write_to_vec().unwrap();
        let source = String::from_utf8(dxf.clone()).unwrap();
        let lines: Vec<_> = source.lines().map(str::trim).collect();
        let pairs: Vec<_> = lines.chunks_exact(2).map(|p| (p[0], p[1])).collect();
        let start = pairs.iter().position(|p| *p == ("2", "Dashed")).unwrap();
        let fields: Vec<_> = pairs[start..].iter().take_while(|p| p.0 != "0").collect();
        assert_eq!(fields.iter().filter(|p| p.0 == "74").count(), 3);
        assert!(fields.iter().filter(|p| p.0 == "74").all(|p| p.1 == "0"));
        assert!(!fields.iter().any(|p| matches!(p.0, "75" | "340" | "46")));
        let converted = DxfReader::from_reader(Cursor::new(dxf))
            .unwrap()
            .read()
            .unwrap();
        assert_eq!(
            converted.line_types.get("Dashed").unwrap().elements,
            linetype.elements
        );
    }
}
