use std::collections::BTreeMap;
use std::io::Cursor;

use acadrust::entities::{AttributeEntity, EntityType, Insert};
use acadrust::io::dwg::dwg_stream_readers::bit_reader::DwgBitReader;
use acadrust::io::dwg::dwg_stream_readers::merged_reader::DwgMergedReader;
use acadrust::io::dwg::dwg_stream_readers::object_reader::objects::read_book_color;
use acadrust::io::dwg::dwg_stream_writers::bit_writer::DwgBitWriter;
use acadrust::io::dwg::DwgVersion;
use acadrust::objects::{BookColor, ObjectType};
use acadrust::tables::BlockRecord;
use acadrust::types::{Color, DxfVersion, Handle, Vector3};
use acadrust::{CadDocument, DwgReader, DwgWriter, DxfReader, DxfWriter};

const VERSIONS: &[DxfVersion] = &[
    DxfVersion::AC1018,
    DxfVersion::AC1021,
    DxfVersion::AC1024,
    DxfVersion::AC1027,
    DxfVersion::AC1032,
];
const COLORS: &[Color] = &[
    Color::ByLayer,
    Color::ByBlock,
    Color::Index(1),
    Color::Index(255),
    Color::None,
    Color::Rgb {
        r: 13,
        g: 24,
        b: 35,
    },
];

fn dwg(document: &CadDocument) -> CadDocument {
    DwgReader::from_stream(Cursor::new(DwgWriter::write_to_vec(document).unwrap()))
        .read()
        .unwrap()
}

fn dxf(document: &CadDocument, binary: bool) -> CadDocument {
    let bytes = if binary {
        DxfWriter::new_binary(document)
    } else {
        DxfWriter::new(document)
    }
    .write_to_vec()
    .unwrap();
    DxfReader::from_reader(Cursor::new(bytes))
        .unwrap()
        .read()
        .unwrap()
}

fn add_color(document: &mut CadDocument, index: usize, color: Color) -> Handle {
    let handle = document.allocate_handle();
    let mut definition = BookColor::new();
    definition.handle = handle;
    definition.owner = document.header.named_objects_dict_handle;
    definition.color = color;
    definition.book_name = "BOOK132".into();
    definition.color_name = format!("COLOR{index}");
    document
        .objects
        .insert(handle, ObjectType::BookColor(definition));
    handle
}

fn nested_document(version: DxfVersion) -> CadDocument {
    let mut document = CadDocument::with_version(version);
    let existing = add_color(&mut document, 0, Color::ByLayer);
    let mut outer_block = BlockRecord::new("OUTER132");
    outer_block.handle = document.allocate_handle();
    let outer_owner = outer_block.handle;
    document.block_records.add(outer_block).unwrap();
    let mut leaf_block = BlockRecord::new("LEAF132");
    leaf_block.handle = document.allocate_handle();
    document.block_records.add(leaf_block).unwrap();
    for (block_name, owner, explicit_handles) in [
        ("OUTER132", Handle::NULL, false),
        ("LEAF132", outer_owner, true),
    ] {
        let mut insert = Insert::new(block_name, Vector3::ZERO);
        insert.common.owner_handle = owner;
        for (index, &color) in COLORS.iter().enumerate() {
            let mut attribute =
                AttributeEntity::simple(format!("TAG{index}"), format!("VALUE{index}"));
            if explicit_handles {
                attribute.common.handle = document.allocate_handle();
            }
            attribute.common.color = color;
            attribute.common.color_name = Some(format!("BOOK132$COLOR{index}"));
            // The first definition should be reused. Later stale references
            // must be replaced with definitions matching their name and method.
            attribute.common.color_book_handle = if index == 0 || explicit_handles {
                Some(existing)
            } else {
                None
            };
            insert.attributes.push(attribute);
        }
        document.add_entity(EntityType::Insert(insert)).unwrap();
    }
    document
}

fn assert_attributes(document: &CadDocument) {
    let inserts: Vec<_> = document
        .entities()
        .filter_map(|entity| match entity {
            EntityType::Insert(insert) => Some(insert),
            _ => None,
        })
        .collect();
    assert_eq!(inserts.len(), 2);
    let mut handles_by_color = BTreeMap::new();
    for insert in inserts {
        assert_eq!(insert.attributes.len(), COLORS.len());
        for (index, (attribute, &color)) in insert.attributes.iter().zip(COLORS).enumerate() {
            assert_eq!(
                attribute.common.color, color,
                "{} attribute {index}",
                insert.block_name
            );
            assert_eq!(
                attribute.common.color_name.as_deref(),
                Some(format!("BOOK132$COLOR{index}").as_str())
            );
            let handle = attribute
                .common
                .color_book_handle
                .expect("attribute has a color reference");
            assert!(!handle.is_null());
            let ObjectType::BookColor(definition) = document.objects.get(&handle).unwrap() else {
                panic!("DBCOLOR expected");
            };
            assert_eq!(definition.color, color);
            assert_eq!(definition.book_name, "BOOK132");
            assert_eq!(definition.color_name, format!("COLOR{index}"));
            if let Some(previous) = handles_by_color.insert(index, handle) {
                assert_eq!(previous, handle, "shared definitions must be reused");
            }
        }
    }
    assert_eq!(
        document
            .objects
            .values()
            .filter(|object| matches!(object, ObjectType::BookColor(_)))
            .count(),
        COLORS.len()
    );
}

#[test]
fn nested_insert_attributes_keep_named_color_methods_across_repeated_dwg_and_dxf_conversions() {
    for &version in VERSIONS {
        let source = nested_document(version);
        let before = source.clone();
        let mut restored = dwg(&source);
        assert_eq!(source, before, "writer mutated source {version:?}");
        for _ in 0..2 {
            assert_attributes(&restored);
            restored = dwg(&restored);
        }
        for binary in [false, true] {
            let converted = dxf(&restored, binary);
            assert_attributes(&converted);
            assert_attributes(&dwg(&converted));
        }
    }
}

#[test]
fn handle_only_nested_attribute_colors_resolve_after_dwg_read() {
    for &version in VERSIONS {
        let mut source = nested_document(version);
        let color_handles: Vec<_> = COLORS
            .iter()
            .enumerate()
            .map(|(index, &color)| {
                if index == 0 {
                    source
                        .objects
                        .iter()
                        .find_map(|(handle, object)| {
                            matches!(object, ObjectType::BookColor(_)).then_some(*handle)
                        })
                        .unwrap()
                } else {
                    add_color(&mut source, index, color)
                }
            })
            .collect();
        for entity in source.entities_mut() {
            if let EntityType::Insert(insert) = entity {
                for (index, attribute) in insert.attributes.iter_mut().enumerate() {
                    attribute.common.color_name = None;
                    attribute.common.color_book_handle = Some(color_handles[index]);
                }
            }
        }
        assert_attributes(&dwg(&source));
    }
}

#[test]
fn unsupported_nested_attribute_named_colors_fail_before_modifying_output() {
    for version in [
        DxfVersion::AC1009,
        DxfVersion::AC1012,
        DxfVersion::AC1014,
        DxfVersion::AC1015,
    ] {
        let mut source = nested_document(version);
        for handle_only in [false, true] {
            if handle_only {
                for entity in source.entities_mut() {
                    if let EntityType::Insert(insert) = entity {
                        for attribute in &mut insert.attributes {
                            attribute.common.color_name = None;
                        }
                    }
                }
            }
            let mut output = Cursor::new(vec![1, 2, 3]);
            let error = DwgWriter::write_to_writer(&mut output, &source).unwrap_err();
            assert!(
                error.to_string().contains("Named color"),
                "{version:?}: {error}"
            );
            assert_eq!(output.into_inner(), [1, 2, 3]);
        }
    }
}

#[test]
fn raw_book_color_cmc_methods_decode_without_collapsing_to_rgb() {
    for (packed, expected) in [
        (0xC000_0000u32, Color::ByLayer),
        (0xC100_0000, Color::ByBlock),
        (0xC300_0001, Color::Index(1)),
        (0xC300_00FF, Color::Index(255)),
        (0xC800_0000, Color::None),
        (
            0xC20D_1823,
            Color::Rgb {
                r: 13,
                g: 24,
                b: 35,
            },
        ),
    ] {
        let mut writer = DwgBitWriter::new(DwgVersion::AC18, DxfVersion::AC1018);
        writer.write_bit_short(0);
        writer.write_bit_long(packed as i32);
        writer.write_byte(3);
        writer.write_variable_text("COLOR");
        writer.write_variable_text("BOOK");
        let reader = DwgBitReader::new(writer.into_bytes(), DwgVersion::AC18, DxfVersion::AC1018);
        let mut reader = DwgMergedReader::from_readers(reader, None, None, DxfVersion::AC1018);
        let color = read_book_color(&mut reader);
        assert_eq!(color.color, expected, "CMC {packed:08X}");
        assert_eq!(color.color_name, "COLOR");
        assert_eq!(color.book_name, "BOOK");
    }
}

#[test]
fn standalone_book_color_definitions_preserve_every_color_method() {
    let colors: Vec<_> = COLORS
        .iter()
        .copied()
        .chain((1..=255).map(Color::Index))
        .collect();
    for &version in VERSIONS {
        let mut source = CadDocument::with_version(version);
        let handles: Vec<_> = colors
            .iter()
            .enumerate()
            .map(|(index, &color)| add_color(&mut source, index, color))
            .collect();
        let restored = dwg(&source);
        for (handle, &color) in handles.iter().zip(&colors) {
            let ObjectType::BookColor(definition) = restored.objects.get(handle).unwrap() else {
                panic!("DBCOLOR expected");
            };
            assert_eq!(definition.color, color, "{version:?}");
        }
    }
}
