//! Annotative context objects through a whole DWG save and read (fork patch A1): every modelled
//! class is written by the object writer and read back by the re-derived decoder, which keeps a
//! payload only when the record ends exactly with it. A record of a class the fork does not model
//! survives verbatim as an unknown object with its scale link.

use std::io::Cursor;

use acadrust::entities::{
    BoundaryEdge, BoundaryPath, BoundaryPathFlags, HatchPatternLine, LineEdge,
};
use acadrust::objects::{
    DimContext, DimSubtype, EmbeddedMTextContext, HatchLoopContext, HatchScaleContext,
    HatchViewContext, LeaderContext, MTextAttributeContext, MTextColumns, MTextContext,
    ObjectContextData, ObjectContextKind, ObjectType, Scale,
};
use acadrust::types::{DxfVersion, Vector2, Vector3};
use acadrust::{CadDocument, DwgReader, DwgWriter, Handle};

fn roundtrip(doc: &CadDocument) -> CadDocument {
    let bytes = DwgWriter::write_to_vec(doc).expect("dwg write");
    let mut r = DwgReader::from_stream(Cursor::new(bytes));
    r.read().expect("dwg re-read")
}

/// One context leaf of `kind` on a fresh document of `version`, written and read back: the
/// reloaded leaf (scale link checked) as (kind, class version, default).
fn roundtrip_leaf(version: DxfVersion, kind: ObjectContextKind) -> (ObjectContextKind, i16, bool) {
    let mut doc = CadDocument::with_version(version);
    let class = kind.class_name().expect("a modelled class");
    doc.register_object_context_class(class);

    let scale_h = doc.allocate_handle();
    let mut scale = Scale::new("1:50", 1.0, 50.0);
    scale.handle = scale_h;
    doc.objects.insert(scale_h, ObjectType::Scale(scale));

    let owner_h = doc.allocate_handle();
    let leaf_h = doc.allocate_handle();
    let ctx = ObjectContextData {
        handle: leaf_h,
        owner_handle: owner_h,
        reactors: vec![owner_h],
        xdictionary_handle: None,
        class_version: 4,
        is_default: true,
        scale: scale_h,
        kind,
    };
    doc.objects
        .insert(leaf_h, ObjectType::ObjectContextData(ctx));

    let rt = roundtrip(&doc);
    let c = match rt.objects.get(&leaf_h) {
        Some(ObjectType::ObjectContextData(c)) => c.clone(),
        other => panic!("{class}: the leaf came back as {other:?}"),
    };
    assert_eq!(c.scale, scale_h, "{class}: scale link");
    assert!(
        matches!(rt.objects.get(&c.scale), Some(ObjectType::Scale(_))),
        "{class}: the scale handle must resolve to a SCALE"
    );
    assert_eq!(rt.context_scales.get(&leaf_h), Some(&scale_h), "{class}");
    (c.kind, c.class_version, c.is_default)
}

const VERSIONS: [DxfVersion; 4] = [
    DxfVersion::AC1015,
    DxfVersion::AC1018,
    DxfVersion::AC1024,
    DxfVersion::AC1032,
];

fn assert_roundtrips(kind: ObjectContextKind) {
    for v in VERSIONS {
        let (got, cv, def) = roundtrip_leaf(v, kind.clone());
        assert_eq!((cv, def), (4, true), "{v:?}");
        assert_eq!(got, kind, "{v:?}");
    }
}

fn mtext(column_type: i16, columns: Option<MTextColumns>) -> MTextContext {
    MTextContext {
        attachment: 2,
        x_direction: Vector3::new(0.6, 0.8, 0.0),
        insertion: Vector3::new(7306.97, 12427.8, 0.0),
        defined_width: 39.26,
        defined_height: 17.6,
        extents_width: 1.146,
        extents_height: 0.0975,
        column_type,
        columns,
    }
}

#[test]
fn annotation_scale_context_roundtrips() {
    assert_roundtrips(ObjectContextKind::AnnotScale);
}

#[test]
fn block_reference_context_roundtrips() {
    assert_roundtrips(ObjectContextKind::BlkRef {
        rotation: 0.75,
        insertion: Vector3::new(123.5, -42.25, 7.0),
        scale: Vector3::new(2.0, 3.0, 4.0),
    });
}

#[test]
fn text_context_roundtrips() {
    assert_roundtrips(ObjectContextKind::Text {
        horizontal_mode: 1,
        rotation: 0.5,
        insertion: Vector2::new(10.0, 20.0),
        alignment: Vector2::new(-3.0, 4.0),
    });
}

#[test]
fn mtext_context_roundtrips_with_and_without_columns() {
    assert_roundtrips(ObjectContextKind::MText(mtext(0, None)));
    assert_roundtrips(ObjectContextKind::MText(mtext(
        2,
        Some(MTextColumns {
            count: 3,
            width: 30.0,
            gutter: 5.0,
            auto_height: false,
            flow_reversed: true,
            heights: vec![10.0, 20.0, 15.0],
        }),
    )));
}

#[test]
fn attribute_context_roundtrips_single_and_multiline() {
    assert_roundtrips(ObjectContextKind::MTextAttribute(MTextAttributeContext {
        horizontal_mode: 2,
        rotation: 0.25,
        insertion: Vector2::new(1.0, 2.0),
        alignment: Vector2::new(5.0, 2.0),
        multiline: false,
        mtext: None,
    }));
    for v in VERSIONS {
        let embedded = EmbeddedMTextContext {
            owner_handle: Handle::NULL,
            reactors: Vec::new(),
            xdictionary_handle: None,
            has_ds_data: false,
            class_version: 4,
            is_default: true,
            scale: Handle::NULL,
            mtext: mtext(0, None),
        };
        let kind = ObjectContextKind::MTextAttribute(MTextAttributeContext {
            horizontal_mode: 0,
            rotation: 0.0,
            insertion: Vector2::new(1.0, 2.0),
            alignment: Vector2::new(0.0, 0.0),
            multiline: true,
            mtext: Some(embedded),
        });
        let (got, _, _) = roundtrip_leaf(v, kind.clone());
        let (ObjectContextKind::MTextAttribute(g), ObjectContextKind::MTextAttribute(k)) =
            (&got, &kind)
        else {
            panic!("{v:?}: {got:?}")
        };
        assert!(g.multiline, "{v:?}");
        // handles of the embedded object may be remapped by the save; its payload may not
        assert_eq!(
            g.mtext
                .as_ref()
                .map(|e| (&e.mtext, e.class_version, e.is_default)),
            k.mtext
                .as_ref()
                .map(|e| (&e.mtext, e.class_version, e.is_default)),
            "{v:?}"
        );
    }
}

#[test]
fn leader_context_roundtrips() {
    assert_roundtrips(ObjectContextKind::Leader(LeaderContext {
        vertices: vec![
            Vector3::new(9.57, 10.07, 0.0),
            Vector3::new(12.44, 12.75, 0.0),
            Vector3::new(15.39, 12.75, 0.0),
        ],
        horizontal_direction: Vector3::new(1.0, 0.0, 0.0),
        flag: false,
        block_offset: Vector3::new(0.0, 0.0, 0.0),
        annotation_offset: Vector3::new(0.0, -0.09, 0.0),
    }));
}

fn dim(subtype: DimSubtype) -> ObjectContextKind {
    ObjectContextKind::Dim(DimContext {
        block: Handle::from(0x2Au64),
        text_location: Vector2::new(11.5, 22.5),
        flag: true,
        text_rotation: 0.25,
        bits: 0b0_0000_1000_1000_0011,
        subtype,
    })
}

#[test]
fn dimension_contexts_roundtrip() {
    assert_roundtrips(dim(DimSubtype::Aligned {
        definition_point: Vector3::new(1.0, 2.0, 3.0),
    }));
    assert_roundtrips(dim(DimSubtype::Angular {
        point: Vector3::new(4.0, 5.0, 0.0),
    }));
    assert_roundtrips(dim(DimSubtype::Diametric {
        point: Vector3::new(4.0, 5.0, 0.0),
        definition_point: Vector3::new(-4.0, -5.0, 0.0),
    }));
    assert_roundtrips(dim(DimSubtype::Radial {
        definition_point: Vector3::new(7.0, 8.0, 0.0),
    }));
}

fn hatch() -> HatchScaleContext {
    let mut boundary = BoundaryPath::with_flags(BoundaryPathFlags::from_bits(1));
    boundary.add_edge(BoundaryEdge::Line(LineEdge {
        start: Vector2::new(1.0, 2.0),
        end: Vector2::new(3.0, 4.0),
    }));
    HatchScaleContext {
        pattern_lines: vec![HatchPatternLine {
            angle: 0.5,
            base_point: Vector2::new(5.0, 6.0),
            offset: Vector2::new(0.25, 0.5),
            dash_lengths: vec![1.0, -0.5],
        }],
        factor: 1.0,
        direction: Vector3::new(0.0, 1.0, 0.0),
        loops: vec![
            HatchLoopContext {
                path_flags: 7,
                shared: true,
                boundary: None,
            },
            HatchLoopContext {
                path_flags: 1,
                shared: false,
                boundary: Some(boundary),
            },
        ],
    }
}

#[test]
fn hatch_contexts_roundtrip() {
    for v in VERSIONS {
        let (got, _, _) = roundtrip_leaf(v, ObjectContextKind::HatchScale(hatch()));
        let ObjectContextKind::HatchScale(h) = got else {
            panic!("{v:?}: {got:?}")
        };
        assert_eq!(h.pattern_lines, hatch().pattern_lines, "{v:?}");
        assert_eq!((h.factor, h.direction), (1.0, hatch().direction), "{v:?}");
        let shape: Vec<_> = h
            .loops
            .iter()
            .map(|l| {
                (
                    l.path_flags,
                    l.shared,
                    l.boundary.as_ref().map(|b| b.edges.len()),
                )
            })
            .collect();
        assert_eq!(shape, vec![(7, true, None), (1, false, Some(1))], "{v:?}");
    }
    let (got, _, _) = roundtrip_leaf(
        DxfVersion::AC1032,
        ObjectContextKind::HatchView(HatchViewContext {
            hatch: hatch(),
            view: Handle::NULL,
            normal: Vector3::new(0.0, 0.0, 1.0),
            rotation: 1.5,
            flag: false,
        }),
    );
    let ObjectContextKind::HatchView(hv) = got else {
        panic!("{got:?}")
    };
    assert_eq!(
        (hv.normal, hv.rotation, hv.flag),
        (Vector3::new(0.0, 0.0, 1.0), 1.5, false)
    );
}
