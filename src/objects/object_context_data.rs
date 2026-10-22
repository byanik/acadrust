//! Annotative representations: the per-scale context objects of an annotative object.
//!
//! An annotative object keeps one context object per annotation scale. They are found through its
//! extension dictionary: `AcDbContextDataManager` → `ACDB_ANNOTATIONSCALES` → one entry per scale.
//! Every context names the `AcDbScale` (SCALE object) it belongs to; exactly one of an object's
//! contexts is its default, whose placement equals the object's own fields.
//!
//! These types and the DWG decoder that fills them (fork patch A1) were re-derived from the ODA
//! document (reference only: the common AcDbObjectContextData and AcDbAnnotScaleObjectContextData
//! fields and MLeaderAnnotContext), ACadSharp (MIT: the common fields and the block reference
//! payload), the DXF group codes of public drawings and the bit traces of every context record of the
//! public and further corpora, each default context checked against its object's own fields.
//! See PROVENANCE.md "A1 provenance". Classes for which no record was available (feature control
//! frames, ordinate and large radial dimensions) are not modelled: the DWG reader keeps such records
//! verbatim as unknown objects and the DXF reader as [`ObjectContextKind::Opaque`].

use crate::entities::{BoundaryPath, HatchPatternLine};
use crate::types::{Handle, Vector2, Vector3};

/// One annotative representation (`ACDB_<KIND>OBJECTCONTEXTDATA_CLASS` and the two hatch context
/// classes).
///
/// DWG: the common object data, then `BS` class version (DXF 70; 3 or 4 in every record seen),
/// `B` default (DXF 290) and the class payload; the handle stream holds the scale (hard pointer,
/// DXF 340) first, then the payload's own handles.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct ObjectContextData {
    /// Handle of this context object.
    pub handle: Handle,
    /// Owner: the `ACDB_ANNOTATIONSCALES` dictionary of the annotative object.
    pub owner_handle: Handle,
    /// Reactor handles.
    pub reactors: Vec<Handle>,
    /// Extension dictionary, if any.
    pub xdictionary_handle: Option<Handle>,
    /// Class version (DXF 70).
    pub class_version: i16,
    /// The object's default representation (DXF 290): its placement is the object's own.
    pub is_default: bool,
    /// The `AcDbScale` this representation is drawn at (DXF 340).
    pub scale: Handle,
    /// The class payload.
    pub kind: ObjectContextKind,
}

/// The class payload of a context object.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum ObjectContextKind {
    /// `ACDB_ANNOTSCALEOBJECTCONTEXTDATA_CLASS`: no payload after the scale.
    AnnotScale,
    /// `ACDB_BLKREFOBJECTCONTEXTDATA_CLASS`: `BD` rotation, `3BD` insertion point, three `BD`
    /// scale factors (X, Y, Z).
    BlkRef {
        rotation: f64,
        insertion: Vector3,
        scale: Vector3,
    },
    /// `ACDB_TEXTOBJECTCONTEXTDATA_CLASS`: `BS` horizontal mode (DXF 70), `BD` rotation (50),
    /// `2RD` insertion point (10), `2RD` alignment point (11). The height is not stored.
    Text {
        horizontal_mode: i16,
        rotation: f64,
        insertion: Vector2,
        alignment: Vector2,
    },
    /// `ACDB_MTEXTOBJECTCONTEXTDATA_CLASS`.
    MText(MTextContext),
    /// The five dimension classes this fork models (aligned, angular, diametric, radial).
    Dim(DimContext),
    /// `ACDB_MLEADEROBJECTCONTEXTDATA_CLASS`: a whole MLeaderAnnotContext, as a MULTILEADER
    /// entity carries its own.
    MLeader(crate::entities::multileader::MultiLeaderAnnotContext),
    /// `ACDB_MTEXTATTRIBUTEOBJECTCONTEXTDATA_CLASS` (ATTRIB and ATTDEF).
    MTextAttribute(MTextAttributeContext),
    /// `ACDB_LEADEROBJECTCONTEXTDATA_CLASS`.
    Leader(LeaderContext),
    /// `ACDB_HATCHSCALECONTEXTDATA_CLASS`.
    HatchScale(HatchScaleContext),
    /// `ACDB_HATCHVIEWCONTEXTDATA_CLASS`.
    HatchView(HatchViewContext),
    /// A context class this fork does not model (read from DXF): the common fields only.
    Opaque,
}

/// `ACDB_MTEXTOBJECTCONTEXTDATA_CLASS` payload: `BL` attachment (DXF 70), `3BD` x direction (10),
/// `3BD` insertion point (11), `BD` defined width (40), `BD` defined height (41), `BD` extents
/// width (42), `BD` extents height (43), `BS` column type (71) and, for a column type other than 0,
/// the columns. The text height is not stored.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct MTextContext {
    pub attachment: i32,
    pub x_direction: Vector3,
    pub insertion: Vector3,
    pub defined_width: f64,
    pub defined_height: f64,
    pub extents_width: f64,
    pub extents_height: f64,
    /// 0 none, 1 static, 2 dynamic.
    pub column_type: i16,
    pub columns: Option<MTextColumns>,
}

impl Default for MTextContext {
    fn default() -> Self {
        MTextContext {
            attachment: 1,
            x_direction: Vector3::new(1.0, 0.0, 0.0),
            insertion: Vector3::new(0.0, 0.0, 0.0),
            defined_width: 0.0,
            defined_height: 0.0,
            extents_width: 0.0,
            extents_height: 0.0,
            column_type: 0,
            columns: None,
        }
    }
}

/// Columns of an MTEXT context: `BL` count (72), `BD` width (44), `BD` gutter (45), `B` auto
/// height (73), `B` flow reversed (74), and one `BD` height (46) per column when the type is
/// dynamic and the height is not automatic.
#[derive(Debug, Clone, PartialEq, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct MTextColumns {
    pub count: i32,
    pub width: f64,
    pub gutter: f64,
    pub auto_height: bool,
    pub flow_reversed: bool,
    pub heights: Vec<f64>,
}

/// `ACDB_MTEXTATTRIBUTEOBJECTCONTEXTDATA_CLASS` payload: `BS` horizontal mode, `BD` rotation,
/// `2RD` insertion point, `2RD` alignment point, `B` multiline, and for a multiline attribute an
/// embedded MTEXT context.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct MTextAttributeContext {
    pub horizontal_mode: i16,
    pub rotation: f64,
    pub insertion: Vector2,
    pub alignment: Vector2,
    pub multiline: bool,
    pub mtext: Option<EmbeddedMTextContext>,
}

/// The MTEXT context embedded in a multiline attribute's context. DWG: the object prefix of a
/// non-graphical object without its type and handle (`BL` reactor count; R2004+ `B` no extension
/// dictionary; R2013+ `B` binary data flag), `BS` version, `B` default, the MTEXT payload; its
/// handles (owner, reactors, extension dictionary, scale) follow the outer context's.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct EmbeddedMTextContext {
    pub owner_handle: Handle,
    pub reactors: Vec<Handle>,
    pub xdictionary_handle: Option<Handle>,
    pub has_ds_data: bool,
    pub class_version: i16,
    pub is_default: bool,
    pub scale: Handle,
    pub mtext: MTextContext,
}

/// `ACDB_LEADEROBJECTCONTEXTDATA_CLASS` payload: `BL` vertex count (DXF 70), one `3BD` per vertex
/// (10), `3BD` horizontal direction (11), `B` (290), `3BD` block offset (12), `3BD` annotation
/// offset (13).
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct LeaderContext {
    pub vertices: Vec<Vector3>,
    pub horizontal_direction: Vector3,
    pub flag: bool,
    pub block_offset: Vector3,
    pub annotation_offset: Vector3,
}

/// Dimension context payload: `2RD` text location, `B`, `BD` text rotation, 17 single bits (their
/// meaning is not needed to draw the representation and was not established), then the class's
/// points. The handle stream holds the dimension block of this scale after the scale.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct DimContext {
    /// The anonymous dimension block drawn at this scale.
    pub block: Handle,
    pub text_location: Vector2,
    pub flag: bool,
    pub text_rotation: f64,
    /// The 17 bits after the text rotation, the first read in bit 16.
    pub bits: u32,
    pub subtype: DimSubtype,
}

/// The class-specific points of a dimension context.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum DimSubtype {
    /// `ACDB_ALDIMOBJECTCONTEXTDATA_CLASS` (aligned and rotated): `3BD`, equal to the dimension's
    /// definition point in the default context.
    Aligned { definition_point: Vector3 },
    /// `ACDB_ANGDIMOBJECTCONTEXTDATA_CLASS`: one `3BD`.
    Angular { point: Vector3 },
    /// `ACDB_DMDIMOBJECTCONTEXTDATA_CLASS`: two `3BD`, the second equal to the definition point.
    Diametric {
        point: Vector3,
        definition_point: Vector3,
    },
    /// `ACDB_RADIMOBJECTCONTEXTDATA_CLASS`: `3BD`, equal to the definition point.
    Radial { definition_point: Vector3 },
}

impl DimSubtype {
    /// The DXF class name of a context with these points.
    pub fn class_name(&self) -> &'static str {
        match self {
            DimSubtype::Aligned { .. } => "ACDB_ALDIMOBJECTCONTEXTDATA_CLASS",
            DimSubtype::Angular { .. } => "ACDB_ANGDIMOBJECTCONTEXTDATA_CLASS",
            DimSubtype::Diametric { .. } => "ACDB_DMDIMOBJECTCONTEXTDATA_CLASS",
            DimSubtype::Radial { .. } => "ACDB_RADIMOBJECTCONTEXTDATA_CLASS",
        }
    }
}

/// `ACDB_HATCHSCALECONTEXTDATA_CLASS` payload: `BL` pattern line count, per line `BD` angle,
/// `BD` `BD` base point, `BD` `BD` offset, `BS` dash count and one `BD` per dash; `BD` factor (1.0
/// in every record seen), `3BD` direction (a unit vector in every record seen), `BL` loop count and
/// the loops.
#[derive(Debug, Clone, PartialEq, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct HatchScaleContext {
    pub pattern_lines: Vec<HatchPatternLine>,
    pub factor: f64,
    pub direction: Vector3,
    pub loops: Vec<HatchLoopContext>,
}

/// One loop of a hatch context: `BL` path flags, `B` shared (when clear, the boundary path follows
/// in the hatch path layout without boundary object handles).
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct HatchLoopContext {
    pub path_flags: i32,
    pub shared: bool,
    pub boundary: Option<BoundaryPath>,
}

/// `ACDB_HATCHVIEWCONTEXTDATA_CLASS` payload: the hatch-scale payload, `3BD` normal, `BD`
/// rotation, `B`; the handle stream holds the view (soft pointer) after the scale.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct HatchViewContext {
    pub hatch: HatchScaleContext,
    pub view: Handle,
    pub normal: Vector3,
    pub rotation: f64,
    pub flag: bool,
}

impl ObjectContextKind {
    /// The DXF class name of a context with this payload (`None` for [`ObjectContextKind::Opaque`],
    /// whose class the payload does not say).
    pub fn class_name(&self) -> Option<&'static str> {
        Some(match self {
            ObjectContextKind::AnnotScale => "ACDB_ANNOTSCALEOBJECTCONTEXTDATA_CLASS",
            ObjectContextKind::BlkRef { .. } => "ACDB_BLKREFOBJECTCONTEXTDATA_CLASS",
            ObjectContextKind::Text { .. } => "ACDB_TEXTOBJECTCONTEXTDATA_CLASS",
            ObjectContextKind::MText(_) => "ACDB_MTEXTOBJECTCONTEXTDATA_CLASS",
            ObjectContextKind::Dim(d) => d.subtype.class_name(),
            ObjectContextKind::MLeader(_) => "ACDB_MLEADEROBJECTCONTEXTDATA_CLASS",
            ObjectContextKind::MTextAttribute(_) => "ACDB_MTEXTATTRIBUTEOBJECTCONTEXTDATA_CLASS",
            ObjectContextKind::Leader(_) => "ACDB_LEADEROBJECTCONTEXTDATA_CLASS",
            ObjectContextKind::HatchScale(_) => "ACDB_HATCHSCALECONTEXTDATA_CLASS",
            ObjectContextKind::HatchView(_) => "ACDB_HATCHVIEWCONTEXTDATA_CLASS",
            ObjectContextKind::Opaque => return None,
        })
    }
}

impl ObjectContextData {
    /// The DXF class name of this context; an [`ObjectContextKind::Opaque`] one is written as
    /// the plain annotation-scale class (its own payload is not known).
    pub fn class_name(&self) -> &'static str {
        self.kind
            .class_name()
            .unwrap_or("ACDB_ANNOTSCALEOBJECTCONTEXTDATA_CLASS")
    }
}
