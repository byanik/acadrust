//! Annotative context records (fork patch A1). The layouts were re-derived from the ODA document
//! (reference only), ACadSharp (MIT), the DXF group codes of public drawings and the bit traces of
//! every context record of the public and further corpora, each default context checked against
//! its object's own fields (aggregates in PROVENANCE.md "A1 provenance"; LibreDWG consulted: no).
//!
//! After the common object data every record holds `BS` class version, `B` default and the class
//! payload ([`crate::objects::ObjectContextKind`] documents each); the handle stream holds the
//! scale first. A record is modelled only when its payload ends exactly at the end of the main
//! stream: anything else keeps its scale and is preserved verbatim by the caller.

use super::safe_count;
use crate::io::dwg::DwgVersion;
use crate::io::dwg::dwg_stream_readers::merged_reader::DwgMergedReader;
use crate::objects::{
    DimContext, DimSubtype, EmbeddedMTextContext, HatchLoopContext, HatchScaleContext,
    HatchViewContext, LeaderContext, MTextAttributeContext, MTextColumns, MTextContext,
    ObjectContextKind,
};
use crate::types::{DxfVersion, Handle, Vector2, Vector3};

/// What a context record gave: its payload (class version, default flag, scale, payload) or, for a
/// class this fork does not model or a payload that does not end where the record does, the scale
/// alone.
#[derive(Debug)]
pub(crate) enum ContextRecord {
    Modeled {
        kind: ObjectContextKind,
        class_version: i16,
        is_default: bool,
        scale: u64,
    },
    ScaleOnly(u64),
}

/// Reads a context record from right after its common object data. `class` is its DXF class name.
pub(crate) fn read_object_context(
    reader: &mut DwgMergedReader,
    version: DwgVersion,
    dxf_version: DxfVersion,
    class: &str,
) -> ContextRecord {
    let class_version = reader.read_bit_short();
    let is_default = reader.read_bit();
    let scale = reader.read_handle();
    let kind = read_payload(reader, version, dxf_version, class);
    accept(
        kind,
        reader.check_error().is_ok(),
        reader.main_remaining_bits(),
        class_version,
        is_default,
        scale,
    )
}

/// A payload is kept only when the record's streams read without error and the main stream ends
/// exactly where the payload does; otherwise the record keeps its scale alone.
fn accept(
    kind: Option<ObjectContextKind>,
    read_ok: bool,
    main_left: i64,
    class_version: i16,
    is_default: bool,
    scale: u64,
) -> ContextRecord {
    match kind {
        Some(kind) if read_ok && main_left == 0 => ContextRecord::Modeled {
            kind,
            class_version,
            is_default,
            scale,
        },
        _ => ContextRecord::ScaleOnly(scale),
    }
}

/// The class payload of a context record (`None` for a class this fork does not model); the
/// caller decides with [`accept`] whether it stands.
pub(crate) fn read_payload(
    reader: &mut DwgMergedReader,
    version: DwgVersion,
    dxf_version: DxfVersion,
    class: &str,
) -> Option<ObjectContextKind> {
    match class {
        "ACDB_ANNOTSCALEOBJECTCONTEXTDATA_CLASS" => Some(ObjectContextKind::AnnotScale),
        "ACDB_BLKREFOBJECTCONTEXTDATA_CLASS" => Some(ObjectContextKind::BlkRef {
            rotation: reader.read_bit_double(),
            insertion: reader.read_3bit_double(),
            scale: {
                let x = reader.read_bit_double();
                let y = reader.read_bit_double();
                let z = reader.read_bit_double();
                Vector3::new(x, y, z)
            },
        }),
        "ACDB_TEXTOBJECTCONTEXTDATA_CLASS" => Some(ObjectContextKind::Text {
            horizontal_mode: reader.read_bit_short(),
            rotation: reader.read_bit_double(),
            insertion: reader.read_2raw_double(),
            alignment: reader.read_2raw_double(),
        }),
        "ACDB_MTEXTOBJECTCONTEXTDATA_CLASS" => Some(ObjectContextKind::MText(read_mtext(reader))),
        "ACDB_MTEXTATTRIBUTEOBJECTCONTEXTDATA_CLASS" => Some(ObjectContextKind::MTextAttribute(
            read_mtext_attribute(reader, version, dxf_version),
        )),
        "ACDB_LEADEROBJECTCONTEXTDATA_CLASS" => {
            Some(ObjectContextKind::Leader(read_leader(reader)))
        }
        "ACDB_MLEADEROBJECTCONTEXTDATA_CLASS" => Some(ObjectContextKind::MLeader(
            super::entities::read_multileader_annotation_context(
                reader,
                version,
                dxf_version,
                false,
            ),
        )),
        "ACDB_ALDIMOBJECTCONTEXTDATA_CLASS"
        | "ACDB_ANGDIMOBJECTCONTEXTDATA_CLASS"
        | "ACDB_DMDIMOBJECTCONTEXTDATA_CLASS"
        | "ACDB_RADIMOBJECTCONTEXTDATA_CLASS" => {
            Some(ObjectContextKind::Dim(read_dim(reader, class)))
        }
        "ACDB_HATCHSCALECONTEXTDATA_CLASS" => {
            Some(ObjectContextKind::HatchScale(read_hatch(reader, version)))
        }
        "ACDB_HATCHVIEWCONTEXTDATA_CLASS" => {
            let hatch = read_hatch(reader, version);
            let normal = reader.read_3bit_double();
            let rotation = reader.read_bit_double();
            let flag = reader.read_bit();
            let view = Handle::from(reader.read_handle());
            Some(ObjectContextKind::HatchView(HatchViewContext {
                hatch,
                view,
                normal,
                rotation,
                flag,
            }))
        }
        _ => None,
    }
}

fn read_mtext(reader: &mut DwgMergedReader) -> MTextContext {
    let attachment = reader.read_bit_long();
    let x_direction = reader.read_3bit_double();
    let insertion = reader.read_3bit_double();
    let defined_width = reader.read_bit_double();
    let defined_height = reader.read_bit_double();
    let extents_width = reader.read_bit_double();
    let extents_height = reader.read_bit_double();
    let column_type = reader.read_bit_short();
    let columns = (column_type != 0).then(|| {
        let count = safe_count(reader.read_bit_long(), reader);
        let width = reader.read_bit_double();
        let gutter = reader.read_bit_double();
        let auto_height = reader.read_bit();
        let flow_reversed = reader.read_bit();
        let mut heights = Vec::new();
        if column_type == 2 && !auto_height {
            for _ in 0..count.max(0) {
                if reader.check_error().is_err() {
                    break;
                }
                heights.push(reader.read_bit_double());
            }
        }
        MTextColumns {
            count,
            width,
            gutter,
            auto_height,
            flow_reversed,
            heights,
        }
    });
    MTextContext {
        attachment,
        x_direction,
        insertion,
        defined_width,
        defined_height,
        extents_width,
        extents_height,
        column_type,
        columns,
    }
}

fn read_mtext_attribute(
    reader: &mut DwgMergedReader,
    version: DwgVersion,
    dxf_version: DxfVersion,
) -> MTextAttributeContext {
    let horizontal_mode = reader.read_bit_short();
    let rotation = reader.read_bit_double();
    let insertion = reader.read_2raw_double();
    let alignment = reader.read_2raw_double();
    let multiline = reader.read_bit();
    let mtext = multiline.then(|| {
        // the embedded object's prefix, as a non-graphical object's after its handle
        let reactor_count = safe_count(reader.read_bit_long(), reader);
        let has_xdict = if version.r2004_plus() {
            !reader.read_bit()
        } else {
            true
        };
        let has_ds_data = version.r2013_plus(dxf_version) && reader.read_bit();
        let class_version = reader.read_bit_short();
        let is_default = reader.read_bit();
        let mtext = read_mtext(reader);
        let owner_handle = Handle::from(reader.read_handle());
        let mut reactors = Vec::new();
        for _ in 0..reactor_count.max(0) {
            if reader.check_error().is_err() {
                break;
            }
            reactors.push(Handle::from(reader.read_handle()));
        }
        let xdictionary_handle = if has_xdict {
            let h = reader.read_handle();
            (h != 0).then(|| Handle::from(h))
        } else {
            None
        };
        let scale = Handle::from(reader.read_handle());
        EmbeddedMTextContext {
            owner_handle,
            reactors,
            xdictionary_handle,
            has_ds_data,
            class_version,
            is_default,
            scale,
            mtext,
        }
    });
    MTextAttributeContext {
        horizontal_mode,
        rotation,
        insertion,
        alignment,
        multiline,
        mtext,
    }
}

fn read_leader(reader: &mut DwgMergedReader) -> LeaderContext {
    let n = safe_count(reader.read_bit_long(), reader);
    let mut vertices = Vec::new();
    for _ in 0..n.max(0) {
        if reader.check_error().is_err() {
            break;
        }
        vertices.push(reader.read_3bit_double());
    }
    LeaderContext {
        vertices,
        horizontal_direction: reader.read_3bit_double(),
        flag: reader.read_bit(),
        block_offset: reader.read_3bit_double(),
        annotation_offset: reader.read_3bit_double(),
    }
}

fn read_dim(reader: &mut DwgMergedReader, class: &str) -> DimContext {
    let block = Handle::from(reader.read_handle());
    let text_location: Vector2 = reader.read_2raw_double();
    let flag = reader.read_bit();
    let text_rotation = reader.read_bit_double();
    let mut bits = 0u32;
    for _ in 0..17 {
        bits = (bits << 1) | u32::from(reader.read_bit());
    }
    let subtype = match class {
        "ACDB_ANGDIMOBJECTCONTEXTDATA_CLASS" => DimSubtype::Angular {
            point: reader.read_3bit_double(),
        },
        "ACDB_DMDIMOBJECTCONTEXTDATA_CLASS" => DimSubtype::Diametric {
            point: reader.read_3bit_double(),
            definition_point: reader.read_3bit_double(),
        },
        "ACDB_RADIMOBJECTCONTEXTDATA_CLASS" => DimSubtype::Radial {
            definition_point: reader.read_3bit_double(),
        },
        _ => DimSubtype::Aligned {
            definition_point: reader.read_3bit_double(),
        },
    };
    DimContext {
        block,
        text_location,
        flag,
        text_rotation,
        bits,
        subtype,
    }
}

fn read_hatch(reader: &mut DwgMergedReader, version: DwgVersion) -> HatchScaleContext {
    let n = safe_count(reader.read_bit_long(), reader);
    let mut pattern_lines = Vec::new();
    for _ in 0..n.max(0) {
        if reader.check_error().is_err() {
            break;
        }
        let angle = reader.read_bit_double();
        let base_point = Vector2::new(reader.read_bit_double(), reader.read_bit_double());
        let offset = Vector2::new(reader.read_bit_double(), reader.read_bit_double());
        let dashes = i32::from(reader.read_bit_short()).max(0);
        let dashes = safe_count(dashes, reader);
        let mut dash_lengths = Vec::new();
        for _ in 0..dashes.max(0) {
            if reader.check_error().is_err() {
                break;
            }
            dash_lengths.push(reader.read_bit_double());
        }
        pattern_lines.push(crate::entities::HatchPatternLine {
            angle,
            base_point,
            offset,
            dash_lengths,
        });
    }
    let factor = reader.read_bit_double();
    let direction = reader.read_3bit_double();
    let loop_count = safe_count(reader.read_bit_long(), reader);
    let mut loops = Vec::new();
    for _ in 0..loop_count.max(0) {
        if reader.check_error().is_err() {
            break;
        }
        let path_flags = reader.read_bit_long();
        let shared = reader.read_bit();
        let boundary = (!shared).then(|| {
            crate::io::dwg::dwg_document_builder::boundary_path_from_dwg(
                super::entities::read_hatch_boundary_path_contents(
                    reader, version, path_flags, false,
                ),
            )
        });
        loops.push(HatchLoopContext {
            path_flags,
            shared,
            boundary,
        });
    }
    HatchScaleContext {
        pattern_lines,
        factor,
        direction,
        loops,
    }
}

#[cfg(all(test, feature = "writer-dwg"))]
mod tests {
    //! The payloads are written bit by bit here from the layouts in the module documentation (not
    //! through the object writer), then decoded. The two-stream test streams pad the main stream to
    //! a byte, so these tests check the fields and leave fewer than 8 bits; the exact end is
    //! [`accept`]'s and the document round trips' (tests/annotative_context_encoder.rs).
    use super::*;
    use crate::io::dwg::dwg_reference_type::DwgReferenceType;
    use crate::io::dwg::dwg_stream_writers::merged_writer::DwgMergedWriter;

    const V: DwgVersion = DwgVersion::AC15;
    const D: DxfVersion = DxfVersion::AC1015;

    fn decode(
        class: &str,
        f: impl FnOnce(&mut DwgMergedWriter),
    ) -> (Option<ObjectContextKind>, i64) {
        let mut w = DwgMergedWriter::new(V, D);
        f(&mut w);
        let data = w.merge();
        let mut r = DwgMergedReader::new(data, D, w.handle_start_bits());
        let kind = read_payload(&mut r, V, D, class);
        assert!(r.check_error().is_ok(), "{class}: {:?}", r.check_error());
        (kind, r.main_remaining_bits())
    }

    fn v3(x: f64, y: f64, z: f64) -> Vector3 {
        Vector3::new(x, y, z)
    }

    #[test]
    fn text_payload() {
        let (k, left) = decode("ACDB_TEXTOBJECTCONTEXTDATA_CLASS", |w| {
            w.write_bit_short(2);
            w.write_bit_double(0.5);
            w.write_2raw_double(Vector2::new(1.0, 2.0));
            w.write_2raw_double(Vector2::new(3.0, 4.0));
        });
        assert!(left < 8);
        assert_eq!(
            k,
            Some(ObjectContextKind::Text {
                horizontal_mode: 2,
                rotation: 0.5,
                insertion: Vector2::new(1.0, 2.0),
                alignment: Vector2::new(3.0, 4.0),
            })
        );
    }

    #[test]
    fn block_reference_payload() {
        let (k, left) = decode("ACDB_BLKREFOBJECTCONTEXTDATA_CLASS", |w| {
            w.write_bit_double(0.25);
            w.write_3bit_double(v3(5.0, 6.0, 0.0));
            w.write_bit_double(2.0);
            w.write_bit_double(3.0);
            w.write_bit_double(1.0);
        });
        assert!(left < 8);
        assert_eq!(
            k,
            Some(ObjectContextKind::BlkRef {
                rotation: 0.25,
                insertion: v3(5.0, 6.0, 0.0),
                scale: v3(2.0, 3.0, 1.0),
            })
        );
    }

    fn write_mtext(w: &mut DwgMergedWriter, column_type: i16, auto_height: bool) {
        w.write_bit_long(7);
        w.write_3bit_double(v3(0.0, 1.0, 0.0));
        w.write_3bit_double(v3(10.0, 20.0, 0.0));
        w.write_bit_double(30.0);
        w.write_bit_double(0.0);
        w.write_bit_double(28.5);
        w.write_bit_double(4.5);
        w.write_bit_short(column_type);
        if column_type != 0 {
            w.write_bit_long(2);
            w.write_bit_double(12.0);
            w.write_bit_double(1.5);
            w.write_bit(auto_height);
            w.write_bit(true);
            if column_type == 2 && !auto_height {
                w.write_bit_double(8.0);
                w.write_bit_double(6.0);
            }
        }
    }

    fn mtext(columns: Option<MTextColumns>, column_type: i16) -> MTextContext {
        MTextContext {
            attachment: 7,
            x_direction: v3(0.0, 1.0, 0.0),
            insertion: v3(10.0, 20.0, 0.0),
            defined_width: 30.0,
            defined_height: 0.0,
            extents_width: 28.5,
            extents_height: 4.5,
            column_type,
            columns,
        }
    }

    #[test]
    fn mtext_payload_with_and_without_columns() {
        let class = "ACDB_MTEXTOBJECTCONTEXTDATA_CLASS";
        let (k, left) = decode(class, |w| write_mtext(w, 0, false));
        assert!(left < 8);
        assert_eq!(k, Some(ObjectContextKind::MText(mtext(None, 0))));
        let cols = |heights: Vec<f64>, auto_height| MTextColumns {
            count: 2,
            width: 12.0,
            gutter: 1.5,
            auto_height,
            flow_reversed: true,
            heights,
        };
        // dynamic columns: one height per column unless the height is automatic
        let (k, _) = decode(class, |w| write_mtext(w, 2, false));
        assert_eq!(
            k,
            Some(ObjectContextKind::MText(mtext(
                Some(cols(vec![8.0, 6.0], false)),
                2
            )))
        );
        let (k, _) = decode(class, |w| write_mtext(w, 2, true));
        assert_eq!(
            k,
            Some(ObjectContextKind::MText(mtext(Some(cols(vec![], true)), 2)))
        );
        // static columns store no heights
        let (k, _) = decode(class, |w| write_mtext(w, 1, false));
        assert_eq!(
            k,
            Some(ObjectContextKind::MText(mtext(
                Some(cols(vec![], false)),
                1
            )))
        );
    }

    #[test]
    fn multiline_attribute_payload_embeds_an_mtext_context() {
        let (k, left) = decode("ACDB_MTEXTATTRIBUTEOBJECTCONTEXTDATA_CLASS", |w| {
            w.write_bit_short(0);
            w.write_bit_double(0.0);
            w.write_2raw_double(Vector2::new(1.0, 1.0));
            w.write_2raw_double(Vector2::new(0.0, 0.0));
            w.write_bit(true);
            // embedded prefix (R2000: no xdict bit, no data flag), version, default, payload
            w.write_bit_long(0);
            w.write_bit_short(4);
            w.write_bit(true);
            write_mtext(w, 0, false);
            w.write_handle(DwgReferenceType::SoftPointer, 0);
            w.write_handle(DwgReferenceType::HardOwnership, 0);
            w.write_handle(DwgReferenceType::HardPointer, 0x2A);
        });
        assert!(left < 8);
        let Some(ObjectContextKind::MTextAttribute(a)) = k else {
            panic!("{k:?}")
        };
        assert!(a.multiline);
        let e = a.mtext.expect("embedded context");
        assert_eq!((e.class_version, e.is_default), (4, true));
        assert_eq!(e.scale, Handle::from(0x2Au64));
        assert_eq!(e.xdictionary_handle, None);
        assert_eq!(e.mtext, mtext(None, 0));
    }

    #[test]
    fn single_line_attribute_payload_has_no_embedded_context() {
        let (k, left) = decode("ACDB_MTEXTATTRIBUTEOBJECTCONTEXTDATA_CLASS", |w| {
            w.write_bit_short(1);
            w.write_bit_double(1.0);
            w.write_2raw_double(Vector2::new(1.0, 2.0));
            w.write_2raw_double(Vector2::new(3.0, 2.0));
            w.write_bit(false);
        });
        assert!(left < 8);
        assert_eq!(
            k,
            Some(ObjectContextKind::MTextAttribute(MTextAttributeContext {
                horizontal_mode: 1,
                rotation: 1.0,
                insertion: Vector2::new(1.0, 2.0),
                alignment: Vector2::new(3.0, 2.0),
                multiline: false,
                mtext: None,
            }))
        );
    }

    #[test]
    fn leader_payload() {
        let (k, left) = decode("ACDB_LEADEROBJECTCONTEXTDATA_CLASS", |w| {
            w.write_bit_long(2);
            w.write_3bit_double(v3(0.0, 0.0, 0.0));
            w.write_3bit_double(v3(4.0, 3.0, 0.0));
            w.write_3bit_double(v3(1.0, 0.0, 0.0));
            w.write_bit(true);
            w.write_3bit_double(v3(0.5, 0.0, 0.0));
            w.write_3bit_double(v3(0.0, -0.09, 0.0));
        });
        assert!(left < 8);
        assert_eq!(
            k,
            Some(ObjectContextKind::Leader(LeaderContext {
                vertices: vec![v3(0.0, 0.0, 0.0), v3(4.0, 3.0, 0.0)],
                horizontal_direction: v3(1.0, 0.0, 0.0),
                flag: true,
                block_offset: v3(0.5, 0.0, 0.0),
                annotation_offset: v3(0.0, -0.09, 0.0),
            }))
        );
    }

    fn write_dim_head(w: &mut DwgMergedWriter, bits: u32) {
        w.write_handle(DwgReferenceType::HardPointer, 0x77);
        w.write_2raw_double(Vector2::new(9.0, 8.0));
        w.write_bit(true);
        w.write_bit_double(0.125);
        for i in (0..17).rev() {
            w.write_bit((bits >> i) & 1 == 1);
        }
    }

    #[test]
    fn dimension_payloads_keep_the_block_of_their_scale() {
        let bits = 0b1_0000_0100_0100_0011;
        let (k, left) = decode("ACDB_ALDIMOBJECTCONTEXTDATA_CLASS", |w| {
            write_dim_head(w, bits);
            w.write_3bit_double(v3(1.0, 2.0, 0.0));
        });
        assert!(left < 8);
        assert_eq!(
            k,
            Some(ObjectContextKind::Dim(DimContext {
                block: Handle::from(0x77u64),
                text_location: Vector2::new(9.0, 8.0),
                flag: true,
                text_rotation: 0.125,
                bits,
                subtype: DimSubtype::Aligned {
                    definition_point: v3(1.0, 2.0, 0.0)
                },
            }))
        );
        let (k, left) = decode("ACDB_DMDIMOBJECTCONTEXTDATA_CLASS", |w| {
            write_dim_head(w, 0);
            w.write_3bit_double(v3(1.0, 0.0, 0.0));
            w.write_3bit_double(v3(-1.0, 0.0, 0.0));
        });
        assert!(left < 8);
        let Some(ObjectContextKind::Dim(d)) = k else {
            panic!("{k:?}")
        };
        assert_eq!(
            d.subtype,
            DimSubtype::Diametric {
                point: v3(1.0, 0.0, 0.0),
                definition_point: v3(-1.0, 0.0, 0.0)
            }
        );
        for (class, want) in [
            (
                "ACDB_RADIMOBJECTCONTEXTDATA_CLASS",
                DimSubtype::Radial {
                    definition_point: v3(3.0, 0.0, 0.0),
                },
            ),
            (
                "ACDB_ANGDIMOBJECTCONTEXTDATA_CLASS",
                DimSubtype::Angular {
                    point: v3(3.0, 0.0, 0.0),
                },
            ),
        ] {
            let (k, left) = decode(class, |w| {
                write_dim_head(w, 0);
                w.write_3bit_double(v3(3.0, 0.0, 0.0));
            });
            assert!(left < 8);
            let Some(ObjectContextKind::Dim(d)) = k else {
                panic!("{k:?}")
            };
            assert_eq!(d.subtype, want);
        }
    }

    #[test]
    fn hatch_payload_reads_a_boundary_only_for_an_unshared_loop() {
        let (k, left) = decode("ACDB_HATCHSCALECONTEXTDATA_CLASS", |w| {
            w.write_bit_long(1);
            w.write_bit_double(0.75);
            w.write_bit_double(1.0);
            w.write_bit_double(2.0);
            w.write_bit_double(-0.5);
            w.write_bit_double(0.5);
            w.write_bit_short(2);
            w.write_bit_double(0.25);
            w.write_bit_double(-0.125);
            w.write_bit_double(1.0);
            w.write_3bit_double(v3(0.0, 1.0, 0.0));
            w.write_bit_long(2);
            w.write_bit_long(1);
            w.write_bit(true);
            // a polyline loop (flags 2): no bulges, closed, three vertices
            w.write_bit_long(2);
            w.write_bit(false);
            w.write_bit(false);
            w.write_bit(true);
            w.write_bit_long(3);
            for (x, y) in [(0.0, 0.0), (1.0, 0.0), (1.0, 1.0)] {
                w.write_2raw_double(Vector2::new(x, y));
            }
        });
        assert!(left < 8);
        let Some(ObjectContextKind::HatchScale(h)) = k else {
            panic!("{k:?}")
        };
        assert_eq!(h.pattern_lines.len(), 1);
        let l = &h.pattern_lines[0];
        assert_eq!(
            (l.angle, l.base_point, l.offset),
            (0.75, Vector2::new(1.0, 2.0), Vector2::new(-0.5, 0.5))
        );
        assert_eq!(l.dash_lengths, vec![0.25, -0.125]);
        assert_eq!((h.factor, h.direction), (1.0, v3(0.0, 1.0, 0.0)));
        assert_eq!(h.loops.len(), 2);
        assert_eq!((h.loops[0].path_flags, h.loops[0].shared), (1, true));
        assert!(h.loops[0].boundary.is_none());
        assert_eq!((h.loops[1].path_flags, h.loops[1].shared), (2, false));
        assert!(h.loops[1].boundary.is_some());
    }

    #[test]
    fn unmodelled_classes_have_no_payload() {
        for class in [
            "ACDB_FCFOBJECTCONTEXTDATA_CLASS",
            "ACDB_ORDDIMOBJECTCONTEXTDATA_CLASS",
            "ACDB_RADIMLGOBJECTCONTEXTDATA_CLASS",
            "ACDB_SOMETHINGOBJECTCONTEXTDATA_CLASS",
        ] {
            assert_eq!(decode(class, |w| w.write_bit(true)).0, None, "{class}");
        }
    }

    #[test]
    fn a_payload_stands_only_when_the_record_ends_with_it() {
        let modeled = |left, ok| {
            matches!(
                accept(Some(ObjectContextKind::AnnotScale), ok, left, 4, true, 9),
                ContextRecord::Modeled {
                    class_version: 4,
                    is_default: true,
                    scale: 9,
                    ..
                }
            )
        };
        assert!(modeled(0, true));
        assert!(!modeled(1, true), "trailing bits");
        assert!(!modeled(0, false), "a read error");
        assert!(matches!(
            accept(None, true, 0, 4, true, 9),
            ContextRecord::ScaleOnly(9)
        ));
        assert!(matches!(
            accept(Some(ObjectContextKind::AnnotScale), true, 3, 4, false, 9),
            ContextRecord::ScaleOnly(9)
        ));
    }
}

#[cfg(test)]
mod bit_vector_tests {
    //! Payloads assembled bit by bit from the DWG bit codes themselves (BS: `01` + one byte, `10`
    //! zero; BD: `00` + 8 raw bytes, `01` one, `10` zero; RD: 8 raw bytes; bytes most significant bit
    //! first, doubles little-endian), not through the fork's bit writer, so a codec slip shared by
    //! the writer and the reader cannot hide (P4-M4 review Minor 13).
    use super::*;

    const D: DxfVersion = DxfVersion::AC1015;

    #[derive(Default)]
    struct Bits(Vec<bool>);

    impl Bits {
        fn code(&mut self, s: &str) -> &mut Self {
            self.0.extend(s.chars().map(|c| c == '1'));
            self
        }
        fn byte(&mut self, b: u8) -> &mut Self {
            self.0.extend((0..8).rev().map(|i| b >> i & 1 == 1));
            self
        }
        fn rd(&mut self, v: f64) -> &mut Self {
            for b in v.to_le_bytes() {
                self.byte(b);
            }
            self
        }
        /// BD with the full-double code.
        fn bd(&mut self, v: f64) -> &mut Self {
            self.code("00").rd(v)
        }
        fn bytes(&self) -> (Vec<u8>, i64) {
            let mut out = vec![0u8; self.0.len().div_ceil(8)];
            for (i, &b) in self.0.iter().enumerate() {
                if b {
                    out[i / 8] |= 0x80 >> (i % 8);
                }
            }
            (out, self.0.len() as i64)
        }
    }

    fn decode(class: &str, bits: &Bits) -> (Option<ObjectContextKind>, i64) {
        let (data, n) = bits.bytes();
        let mut r = DwgMergedReader::new(data, D, n);
        let k = read_payload(&mut r, DwgVersion::AC15, D, class);
        (k, r.main_remaining_bits())
    }

    #[test]
    fn text_payload_from_raw_bits() {
        let mut b = Bits::default();
        // BS 2, BD 0.5, 2RD (1, 2), 2RD (0, -3)
        b.code("01").byte(2).bd(0.5).rd(1.0).rd(2.0).rd(0.0).rd(-3.0);
        let (k, left) = decode("ACDB_TEXTOBJECTCONTEXTDATA_CLASS", &b);
        assert_eq!(left, 0);
        assert_eq!(
            k,
            Some(ObjectContextKind::Text {
                horizontal_mode: 2,
                rotation: 0.5,
                insertion: Vector2::new(1.0, 2.0),
                alignment: Vector2::new(0.0, -3.0),
            })
        );
    }

    #[test]
    fn block_reference_payload_from_raw_bits() {
        let mut b = Bits::default();
        // BD 0.25 rotation; 3BD (5, 6, 0); BD 2, BD 1 (code 01), BD 0 (code 10)
        b.bd(0.25).bd(5.0).bd(6.0).code("10").bd(2.0).code("01").code("10");
        let (k, left) = decode("ACDB_BLKREFOBJECTCONTEXTDATA_CLASS", &b);
        assert_eq!(left, 0);
        assert_eq!(
            k,
            Some(ObjectContextKind::BlkRef {
                rotation: 0.25,
                insertion: Vector3::new(5.0, 6.0, 0.0),
                scale: Vector3::new(2.0, 1.0, 0.0),
            })
        );
    }

    #[test]
    fn leader_payload_from_raw_bits() {
        let mut b = Bits::default();
        // BL count 2 (`01` + byte), two 3BD vertices, 3BD direction (1, 0, 0), B 1, two 3BD offsets
        b.code("01").byte(2);
        b.bd(1.0).bd(2.0).code("10");
        b.bd(3.0).bd(4.0).code("10");
        b.code("01").code("10").code("10");
        b.code("1");
        b.code("10").code("10").code("10");
        b.bd(0.5).code("10").code("10");
        let (k, left) = decode("ACDB_LEADEROBJECTCONTEXTDATA_CLASS", &b);
        assert_eq!(left, 0);
        let Some(ObjectContextKind::Leader(l)) = k else {
            panic!("{k:?}");
        };
        assert_eq!(
            l.vertices,
            vec![Vector3::new(1.0, 2.0, 0.0), Vector3::new(3.0, 4.0, 0.0)]
        );
        assert_eq!(l.horizontal_direction, Vector3::new(1.0, 0.0, 0.0));
        assert!(l.flag);
        assert_eq!(l.block_offset, Vector3::new(0.0, 0.0, 0.0));
        assert_eq!(l.annotation_offset, Vector3::new(0.5, 0.0, 0.0));
    }
}
