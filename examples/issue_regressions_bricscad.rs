//! Inputs for native BricsCAD validation of issues #127 through #132.

use std::fs;
use std::io::Cursor;
use std::path::{Path, PathBuf};

use acadrust::entities::acis::primitives::build_box;
use acadrust::entities::solid3d::{Solid3D, Wire};
use acadrust::entities::{CellRange, EntityType, Line, Table, Text};
use acadrust::objects::ObjectType;
use acadrust::tables::{AppId, Layer, LineType};
use acadrust::xdata::{ExtendedData, ExtendedDataRecord, XDataValue};
use acadrust::{
    CadDocument, Color, DwgReader, DwgWriter, DxfVersion, DxfWriter, SabWriter, Vector3,
};

fn modern_document(version: DxfVersion) -> CadDocument {
    let mut document = CadDocument::with_version(version);
    let header = &mut document.header;
    header.angle_base = std::f64::consts::FRAC_PI_2;
    header.angle_direction = 1;
    header.fillet_radius = 2.5;
    header.chamfer_distance_a = 1.0;
    header.chamfer_distance_b = 2.0;
    header.elevation = -3.0;
    header.thickness = 4.0;
    document.line_types.add(LineType::dashed()).unwrap();
    let mut line = Line::from_points(Vector3::ZERO, Vector3::new(10.0, 0.0, 0.0));
    line.common.linetype = "Dashed".to_string();
    document.add_entity(EntityType::Line(line)).unwrap();
    let mut color_line = Line::from_points(Vector3::UNIT_Y, Vector3::new(10.0, 1.0, 0.0));
    color_line.common.color = Color::from_rgb(13, 24, 35);
    color_line.common.color_name = Some("MYBOOK$MYCOLOR".to_string());
    document.add_entity(EntityType::Line(color_line)).unwrap();

    let mut solid = Solid3D::from_sab(SabWriter::write(&build_box(
        [20.0, 0.0, 0.0],
        2.0,
        3.0,
        4.0,
    )));
    solid.wires.push(Wire {
        color: Color::ByLayer,
        acis_index: 0x80FA,
        points: vec![Vector3::new(20.0, 0.0, 0.0), Vector3::new(22.0, 0.0, 0.0)],
        ..Wire::default()
    });
    document.add_entity(EntityType::Solid3D(solid)).unwrap();

    let mut table = Table::new(Vector3::new(0.0, 20.0, 0.0), 3, 3);
    table.set_cell_text(0, 0, "Merged title");
    table.merge_cells(CellRange::new(0, 0, 0, 2));
    table.set_cell_text(1, 0, "Header");
    table.set_cell_text(2, 0, "Data");
    if let Some(style) = document
        .objects
        .values_mut()
        .find_map(|object| match object {
            ObjectType::TableStyle(style) if style.name == "Standard" => Some(style),
            _ => None,
        })
    {
        style.set_title_visible(false);
        style.set_header_visible(true);
        table.table_style_handle = Some(style.handle);
    }
    document
        .add_entity(EntityType::Table(Box::new(table)))
        .unwrap();

    document.app_ids.add(AppId::new("REGRESSION132")).unwrap();
    document.layers.add(Layer::new("XDATA132")).unwrap();
    document.assign_table_entry_handles();
    let handle = document.layers.get("XDATA132").unwrap().handle;
    let mut record = ExtendedDataRecord::new("REGRESSION132");
    record.values = vec![
        XDataValue::String("table record payload".to_string()),
        XDataValue::Integer16(132),
    ];
    let mut data = ExtendedData::new();
    data.add_record(record);
    document.object_xdata.insert(handle, data);
    document
}

fn legacy_document(page: &str, text: &str) -> CadDocument {
    let mut document = CadDocument::with_version(DxfVersion::AC1015);
    document.header.code_page = page.to_string();
    document
        .add_entity(EntityType::Text(Text::with_value(text, Vector3::ZERO)))
        .unwrap();
    document
}

fn write_inputs(
    output: &Path,
    stem: &str,
    document: &CadDocument,
    manifest: &mut String,
) -> anyhow::Result<()> {
    let dwg_path = output.join(format!("{stem}.dwg"));
    DwgWriter::write_to_file(&dwg_path, document)?;
    manifest.push_str(&format!("{stem}.dwg\n"));
    for (suffix, writer) in [
        ("ascii", DxfWriter::new(document)),
        ("binary", DxfWriter::new_binary(document)),
    ] {
        writer.write_to_file(output.join(format!("{stem}-{suffix}.dxf")))?;
        manifest.push_str(&format!("{stem}-{suffix}.dxf\n"));
    }
    let decoded = DwgReader::from_stream(Cursor::new(DwgWriter::write_to_vec(document)?)).read()?;
    DxfWriter::new(&decoded).write_to_file(output.join(format!("{stem}-from-dwg.dxf")))?;
    manifest.push_str(&format!("{stem}-from-dwg.dxf\n"));
    Ok(())
}

fn main() -> anyhow::Result<()> {
    if std::env::args().nth(1).as_deref() == Some("--inspect") {
        let path = std::env::args_os()
            .nth(2)
            .expect("--inspect requires a DWG path");
        let document = DwgReader::from_file(&path)?.read()?;
        for object in document.objects.values() {
            if let ObjectType::TableStyle(style) = object {
                println!(
                    "TABLESTYLE {}: flags={:?} raw-long={} title={} header={}",
                    style.name,
                    style.flags,
                    style.modern_unknown_long1,
                    style.title_suppressed,
                    style.header_suppressed
                );
            }
        }
        for entity in document.entities() {
            if let EntityType::Table(table) = entity {
                println!(
                    "TABLE ranges={:?} first-row={:?}",
                    table.merged_ranges,
                    table.rows.first().map(|row| row
                        .cells
                        .iter()
                        .map(|cell| (cell.merged, cell.merge_width, cell.merge_height))
                        .collect::<Vec<_>>())
                );
            }
        }
        DxfWriter::new(&document)
            .write_to_file(PathBuf::from(path).with_extension("inspected.dxf"))?;
        return Ok(());
    }
    let output = std::env::args_os()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("target/bricscad-issues127-132"));
    fs::create_dir_all(&output)?;
    let mut manifest = String::new();
    for (stem, version) in [
        ("modern2010", DxfVersion::AC1024),
        ("modern2018", DxfVersion::AC1032),
    ] {
        write_inputs(&output, stem, &modern_document(version), &mut manifest)?;
    }
    for (stem, page, text) in [
        ("cp437", "DOS437", "\u{e9}"),
        ("macroman", "MAC-ROMAN", "\u{e9}"),
        ("johab", "JOHAB", "\u{ac00}"),
    ] {
        write_inputs(&output, stem, &legacy_document(page, text), &mut manifest)?;
    }
    fs::write(output.join("inputs.txt"), manifest)?;
    println!("Native validation inputs: {}", output.display());
    Ok(())
}
