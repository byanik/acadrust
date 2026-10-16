//! Regression coverage for issue #132: conversion must preserve or report data.

use std::io::Cursor;

use acadrust::entities::{EntityType, Line, UnknownEntity};
use acadrust::objects::{BookColor, ObjectType};
use acadrust::tables::{
    AppId, BlockRecord, DimStyle, Layer, LineType, TextStyle, Ucs, VPort, View,
};
use acadrust::xdata::{ExtendedData, ExtendedDataRecord, XDataValue};
use acadrust::{
    CadDocument, Color, DwgReader, DwgWriter, DxfReader, DxfVersion, DxfWriter, Handle, Vector3,
};

fn dwg_roundtrip(document: &CadDocument) -> CadDocument {
    DwgReader::from_stream(Cursor::new(DwgWriter::write_to_vec(document).unwrap()))
        .read()
        .unwrap()
}

fn dxf_roundtrip(document: &CadDocument, binary: bool) -> CadDocument {
    let writer = if binary {
        DxfWriter::new_binary(document)
    } else {
        DxfWriter::new(document)
    };
    DxfReader::from_reader(Cursor::new(writer.write_to_vec().unwrap()))
        .unwrap()
        .read()
        .unwrap()
}

#[test]
fn dxf_only_custom_entities_and_objects_return_actionable_errors() {
    for version in [DxfVersion::AC1009, DxfVersion::AC1018, DxfVersion::AC1032] {
        let mut document = CadDocument::with_version(version);
        let mut custom = UnknownEntity::new("MYENTITY");
        custom.raw_dxf_codes = Some(vec![(100, "MyEntityData".into()), (1, "payload".into())]);
        let handle = document.add_entity(EntityType::Unknown(custom)).unwrap();
        let mut output = Cursor::new(vec![0xA5; 8]);
        let error = DwgWriter::write_to_writer(&mut output, &document)
            .unwrap_err()
            .to_string();
        assert!(
            error.contains("MYENTITY") && error.contains(&handle.to_string()),
            "{error}"
        );
        assert_eq!(output.into_inner(), vec![0xA5; 8]);
        document.remove_entity(handle);
        let handle = document.allocate_handle();
        document.objects.insert(
            handle,
            ObjectType::Unknown {
                type_name: "MYOBJECT".into(),
                handle,
                owner: Handle::NULL,
                raw_dxf_codes: Some(vec![(1, "payload".into())]),
                raw_dwg_data: None,
                raw_dwg_handle_bits: 0,
                raw_dwg_version: None,
            },
        );
        let error = DwgWriter::write_to_vec(&document).unwrap_err().to_string();
        assert!(
            error.contains("MYOBJECT") && error.contains(&handle.to_string()),
            "{error}"
        );
        let mut output = Cursor::new(Vec::new());
        assert!(DwgWriter::write_to_writer_no_lz77(&mut output, &document).is_err());
        assert!(output.into_inner().is_empty());
    }
}

#[test]
fn custom_raw_dwg_data_cannot_silently_disappear_on_version_change() {
    let mut document = CadDocument::with_version(DxfVersion::AC1032);
    let mut custom = UnknownEntity::new("MYENTITY");
    custom.raw_dwg_data = Some(vec![1]);
    custom.dwg_source_version = Some(DxfVersion::AC1018);
    document.add_entity(EntityType::Unknown(custom)).unwrap();
    assert!(DwgWriter::write_to_vec(&document)
        .unwrap_err()
        .to_string()
        .contains("MYENTITY"));
}

#[test]
fn unsupported_named_colors_return_errors_before_writing() {
    for version in [
        DxfVersion::AC1009,
        DxfVersion::AC1012,
        DxfVersion::AC1014,
        DxfVersion::AC1015,
    ] {
        let mut document = CadDocument::with_version(version);
        let mut line = Line::from_points(Vector3::ZERO, Vector3::UNIT_X);
        line.common.color_name = Some("MYBOOK$MYCOLOR".into());
        document.add_entity(EntityType::Line(line)).unwrap();
        let mut output = Cursor::new(vec![1, 2, 3]);
        assert!(DwgWriter::write_to_writer(&mut output, &document)
            .unwrap_err()
            .to_string()
            .contains("Named color"));
        assert_eq!(output.into_inner(), vec![1, 2, 3]);
    }
}

#[test]
fn entity_named_colors_survive_both_conversion_directions_and_reuse_objects() {
    for version in [
        DxfVersion::AC1018,
        DxfVersion::AC1021,
        DxfVersion::AC1024,
        DxfVersion::AC1027,
        DxfVersion::AC1032,
    ] {
        let mut document = CadDocument::with_version(version);
        for _ in 0..2 {
            let mut line = Line::from_points(Vector3::ZERO, Vector3::new(10.0, 0.0, 0.0));
            line.common.color = Color::from_rgb(13, 24, 35);
            line.common.color_name = Some("MYBOOK$MYCOLOR".into());
            document.add_entity(EntityType::Line(line)).unwrap();
        }
        let input = dxf_roundtrip(&document, false);
        let restored = dwg_roundtrip(&input);
        let colors: Vec<_> = restored
            .objects
            .values()
            .filter_map(|object| match object {
                ObjectType::BookColor(color) => Some(color),
                _ => None,
            })
            .collect();
        assert_eq!(colors.len(), 1, "{version:?}");
        assert_eq!(colors[0].book_name, "MYBOOK");
        assert_eq!(colors[0].color_name, "MYCOLOR");
        assert!(document
            .objects
            .values()
            .all(|object| !matches!(object, ObjectType::BookColor(_))));
        for converted in [
            restored.clone(),
            dxf_roundtrip(&restored, false),
            dxf_roundtrip(&restored, true),
        ] {
            for line in converted.entities().filter_map(|entity| match entity {
                EntityType::Line(line) => Some(line),
                _ => None,
            }) {
                assert_eq!(
                    line.common.color_name.as_deref(),
                    Some("MYBOOK$MYCOLOR"),
                    "{version:?}"
                );
                assert_eq!(line.common.color, Color::from_rgb(13, 24, 35));
            }
        }
        // An existing definition is reused as well.
        let handle = document.allocate_handle();
        let mut color = BookColor::new();
        color.handle = handle;
        color.owner = document.header.named_objects_dict_handle;
        color.color = Color::from_rgb(13, 24, 35);
        color.book_name = "MYBOOK".into();
        color.color_name = "MYCOLOR".into();
        document
            .objects
            .insert(handle, ObjectType::BookColor(color));
        assert_eq!(
            dwg_roundtrip(&document)
                .objects
                .values()
                .filter(|o| matches!(o, ObjectType::BookColor(_)))
                .count(),
            1
        );
    }
}

#[test]
fn all_symbol_table_record_xdata_survives_ascii_binary_and_dwg() {
    for version in [
        DxfVersion::AC1015,
        DxfVersion::AC1018,
        DxfVersion::AC1021,
        DxfVersion::AC1024,
        DxfVersion::AC1032,
    ] {
        let mut document = CadDocument::with_version(version);
        document.app_ids.add(AppId::new("MYAPP")).unwrap();
        document.layers.add(Layer::new("L1")).unwrap();
        document.line_types.add(LineType::new("LT1")).unwrap();
        document.text_styles.add(TextStyle::new("S1")).unwrap();
        document.dim_styles.add(DimStyle::new("D1")).unwrap();
        document.views.add(View::new("V1")).unwrap();
        document.vports.add(VPort::new("VP1")).unwrap();
        document.ucss.add(Ucs::new("U1")).unwrap();
        document.block_records.add(BlockRecord::new("B1")).unwrap();
        document.assign_table_entry_handles();
        let handles = [
            document.layers.get("L1").unwrap().handle,
            document.line_types.get("LT1").unwrap().handle,
            document.text_styles.get("S1").unwrap().handle,
            document.dim_styles.get("D1").unwrap().handle,
            document.views.get("V1").unwrap().handle,
            document.vports.get("VP1").unwrap().handle,
            document.ucss.get("U1").unwrap().handle,
            document.app_ids.get("MYAPP").unwrap().handle,
            document.block_records.get("B1").unwrap().handle,
        ];
        for (index, handle) in handles.into_iter().enumerate() {
            let mut record = ExtendedDataRecord::new("MYAPP");
            record.values = vec![
                XDataValue::String(format!("record {index}")),
                XDataValue::ControlString("{".into()),
                XDataValue::Integer16(-7),
                XDataValue::Integer32(123456),
                XDataValue::Real(2.5),
                XDataValue::BinaryData(vec![0, 1, 255]),
                XDataValue::Point3D(Vector3::new(1.0, 2.0, 3.0)),
                XDataValue::LayerName("L1".into()),
                XDataValue::Handle(handles[0]),
                XDataValue::ControlString("}".into()),
            ];
            let mut data = ExtendedData::new();
            data.add_record(record);
            document.object_xdata.insert(handle, data);
        }
        for binary in [false, true] {
            let input = dxf_roundtrip(&document, binary);
            let dwg = dwg_roundtrip(&input);
            for converted in [input, dwg.clone(), dxf_roundtrip(&dwg, binary)] {
                for handle in handles {
                    assert_eq!(
                        converted
                            .object_xdata
                            .get(&handle)
                            .unwrap()
                            .get_record("MYAPP"),
                        document.object_xdata[&handle].get_record("MYAPP"),
                        "{version:?} {handle}"
                    );
                }
            }
        }
    }
}

#[test]
fn clearing_modeled_table_metadata_preserves_other_values_for_the_same_app() {
    use acadrust::Transparency;

    fn assert_metadata(document: &CadDocument, expected_cleared: bool, handles: [Handle; 4]) {
        let layer = document.layers.get("META_LAYER").unwrap();
        assert_eq!(
            layer.description,
            if expected_cleared {
                ""
            } else {
                "source description"
            }
        );
        assert_eq!(
            layer.transparency,
            if expected_cleared {
                Transparency::OPAQUE
            } else {
                Transparency::T_50
            }
        );
        assert_eq!(
            document.text_styles.get("META_STYLE").unwrap().annotative,
            !expected_cleared
        );
        assert_eq!(
            document.dim_styles.get("META_DIM").unwrap().annotative,
            !expected_cleared
        );
        assert_eq!(
            document.block_records.get("META_BLOCK").unwrap().units,
            if expected_cleared { 0 } else { 4 }
        );
        for (handle, application) in [
            (handles[0], "AcCmTransparency"),
            (handles[0], "AcAecLayerStandard"),
            (handles[1], "AcadAnnotative"),
            (handles[2], "AcadAnnotative"),
            (handles[3], "ACAD"),
        ] {
            if application == "ACAD" && document.version >= DxfVersion::AC1021 {
                continue;
            }
            let data = document.object_xdata.get(&handle).unwrap_or_else(|| {
                panic!("missing {:?} {handle} {application}", document.version)
            });
            let record = data.get_record(application).unwrap();
            assert_eq!(
                data.records()
                    .iter()
                    .filter(|record| record.application_name == application)
                    .count(),
                1
            );
            for value in [
                XDataValue::String(format!("extra {application}")),
                XDataValue::Real(91.25),
                XDataValue::Handle(handles[0]),
                XDataValue::Integer32(123456789),
            ] {
                assert!(
                    record.values.contains(&value),
                    "{application}: {:?}",
                    record.values
                );
            }
        }
    }

    for version in [
        DxfVersion::AC1015,
        DxfVersion::AC1018,
        DxfVersion::AC1021,
        DxfVersion::AC1032,
    ] {
        let mut document = CadDocument::with_version(version);
        for application in ["AcCmTransparency", "AcAecLayerStandard", "AcadAnnotative"] {
            if document.app_ids.get(application).is_none() {
                document.app_ids.add(AppId::new(application)).unwrap();
            }
        }
        let mut layer = Layer::new("META_LAYER");
        layer.transparency = Transparency::T_50;
        layer.description = "source description".to_string();
        document.layers.add(layer).unwrap();
        let mut style = TextStyle::new("META_STYLE");
        style.annotative = true;
        document.text_styles.add(style).unwrap();
        let mut dimension = DimStyle::new("META_DIM");
        dimension.annotative = true;
        document.dim_styles.add(dimension).unwrap();
        let mut block = BlockRecord::new("META_BLOCK");
        block.units = 4;
        document.block_records.add(block).unwrap();
        document.assign_table_entry_handles();
        let handles = [
            document.layers.get("META_LAYER").unwrap().handle,
            document.text_styles.get("META_STYLE").unwrap().handle,
            document.dim_styles.get("META_DIM").unwrap().handle,
            document.block_records.get("META_BLOCK").unwrap().handle,
        ];
        for (handle, application, mut values) in [
            (
                handles[0],
                "AcCmTransparency",
                vec![XDataValue::Integer32(Transparency::T_50.to_dxf_value())],
            ),
            (
                handles[0],
                "AcAecLayerStandard",
                vec![
                    XDataValue::String(String::new()),
                    XDataValue::String("source description".into()),
                ],
            ),
            (
                handles[1],
                "AcadAnnotative",
                vec![
                    XDataValue::String("AnnotativeData".into()),
                    XDataValue::ControlString("{".into()),
                    XDataValue::Integer16(1),
                    XDataValue::Integer16(1),
                    XDataValue::ControlString("}".into()),
                ],
            ),
            (
                handles[2],
                "AcadAnnotative",
                vec![
                    XDataValue::String("AnnotativeData".into()),
                    XDataValue::ControlString("{".into()),
                    XDataValue::Integer16(1),
                    XDataValue::Integer16(1),
                    XDataValue::ControlString("}".into()),
                ],
            ),
            (
                handles[3],
                "ACAD",
                vec![
                    XDataValue::String("DesignCenter Data".into()),
                    XDataValue::ControlString("{".into()),
                    XDataValue::Integer16(1),
                    XDataValue::Integer16(4),
                    XDataValue::ControlString("}".into()),
                ],
            ),
        ] {
            values.extend([
                XDataValue::String(format!("extra {application}")),
                XDataValue::Real(91.25),
                XDataValue::Handle(handles[0]),
                XDataValue::Integer32(123456789),
            ]);
            document
                .object_xdata
                .entry(handle)
                .or_default()
                .add_record(ExtendedDataRecord {
                    application_name: application.to_string(),
                    values,
                });
        }
        let mut inputs = vec![
            (dxf_roundtrip(&document, false), false),
            (dwg_roundtrip(&document), false),
        ];
        if version >= DxfVersion::AC1021 {
            inputs.push((dwg_roundtrip(&document), true));
        }
        for (mut input, raw_only) in inputs {
            assert_metadata(&input, false, handles);
            if raw_only {
                input.object_xdata.clear();
            }
            let layer = input.layers.get_mut("META_LAYER").unwrap();
            layer.description.clear();
            layer.transparency = Transparency::OPAQUE;
            input.text_styles.get_mut("META_STYLE").unwrap().annotative = false;
            input.dim_styles.get_mut("META_DIM").unwrap().annotative = false;
            input.block_records.get_mut("META_BLOCK").unwrap().units = 0;
            for binary in [false, true, false] {
                input = dwg_roundtrip(&input);
                assert_metadata(&input, true, handles);
                input = dxf_roundtrip(&input, binary);
                assert_metadata(&input, true, handles);
            }
        }
    }
}
