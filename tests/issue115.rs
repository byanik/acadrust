use std::io::Cursor;

use acadrust::tables::Layer;
use acadrust::types::{Color, DxfVersion};
use acadrust::{CadDocument, DwgReader, DwgWriter, DxfWriter};

const RGB: Color = Color::from_rgb(200, 100, 50);

fn document() -> CadDocument {
    let mut document = CadDocument::with_version(DxfVersion::AC1032);
    document.layers.add(Layer::with_color("L1", RGB)).unwrap();
    document
}

fn assert_layer_color(dxf: &[u8]) {
    let text = String::from_utf8_lossy(dxf);
    let lines: Vec<_> = text.lines().map(str::trim).collect();
    let groups: Vec<_> = lines
        .chunks_exact(2)
        .map(|pair| (pair[0], pair[1]))
        .collect();
    let layer = groups
        .split(|pair| *pair == ("0", "LAYER"))
        .map(|record| {
            record
                .iter()
                .take_while(|(code, _)| *code != "0")
                .copied()
                .collect::<Vec<_>>()
        })
        .find(|record| record.contains(&("2", "L1")))
        .expect("L1 layer record");
    let aci: i16 = layer
        .iter()
        .find(|(code, _)| *code == "62")
        .and_then(|(_, value)| value.parse().ok())
        .expect("layer group 62");
    let true_color: u32 = layer
        .iter()
        .find(|(code, _)| *code == "420")
        .and_then(|(_, value)| value.parse().ok())
        .expect("layer group 420");
    assert_eq!(aci, RGB.approximate_index());
    assert_eq!(true_color, 13_132_850);
}

#[test]
fn rgb_layer_writes_nearest_aci_and_true_color() {
    let document = document();
    let dxf = DxfWriter::new(&document).write_to_vec().unwrap();
    assert_layer_color(&dxf);
}

#[test]
fn rgb_layer_keeps_nearest_aci_after_dwg_roundtrip() {
    let document = document();
    let dwg = DwgWriter::write_to_vec(&document).unwrap();
    let restored = DwgReader::from_stream(Cursor::new(dwg)).read().unwrap();
    let dxf = DxfWriter::new(&restored).write_to_vec().unwrap();
    assert_layer_color(&dxf);
}
