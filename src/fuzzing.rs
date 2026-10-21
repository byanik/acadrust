//! Single-record decode entry for fuzzing (this fork, patch Z1; feature `fuzzing`, off by default
//! and never part of a shipped build).
//!
//! [`decode_object`] runs one object record through the same pass-2 dispatch a full DWG read
//! uses, without the file around it, so a fuzzer can drive the record decoders of the kinds in
//! [`SCOPE`] directly. A record is given as it lies in the objects section:
//!
//! ```text
//! [MS size][R2010+: MC handle-stream bits][merged data (size bytes)][CRC-16]
//! ```
//!
//! The CRC is not checked (a full read does not check it either). Unlike a full read, no
//! `catch_unwind` surrounds the decoder: a panic escapes to the caller.

use crate::error::DxfError;
use crate::io::dwg::crc::{crc16, CRC16_SEED};
use crate::types::DxfVersion;

/// One record kind the entry decodes: its DXF name and the type code [`decode_object`] takes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ScopeKind {
    pub name: &'static str,
    pub type_code: i16,
    /// An entity (otherwise an object): the record must decode to one.
    pub entity: bool,
}

const fn kind(name: &'static str, type_code: i16, entity: bool) -> ScopeKind {
    ScopeKind {
        name,
        type_code,
        entity,
    }
}

/// The kinds [`decode_object`] accepts. Fixed DWG types keep their type code; class-based kinds
/// get the class numbers of a fixed class table from 500 on (a file numbers its classes itself).
pub const SCOPE: &[ScopeKind] = &[
    kind("LEADER", 45, true),
    kind("TOLERANCE", 46, true),
    kind("MLINE", 47, true),
    kind("HATCH", 78, true),
    kind("ACAD_PROXY_ENTITY", 498, true),
    kind("MULTILEADER", 500, true),
    kind("SPATIAL_FILTER", 501, false),
    kind("LSDEFINITION", 502, false),
    kind("LSSYMBOLCOMPONENT", 503, false),
    kind("LSCOMPOUNDCOMPONENT", 504, false),
    kind("LSSTROKEPATTERNCOMPONENT", 505, false),
    kind("LSPOINTCOMPONENT", 506, false),
    kind("LSINTERNALCOMPONENT", 507, false),
];

/// The versions with an object stream (R13 to 2018).
pub const VERSIONS: &[DxfVersion] = &[
    DxfVersion::AC1012,
    DxfVersion::AC1014,
    DxfVersion::AC1015,
    DxfVersion::AC1018,
    DxfVersion::AC1021,
    DxfVersion::AC1024,
    DxfVersion::AC1027,
    DxfVersion::AC1032,
];

/// Decodes one framed record as the kind `type_code` (a [`SCOPE`] code) of `version`. `true` when
/// the record decoded without a reader error and a full read would have kept it as what the kind
/// is (an entity, or an object: a DGN line-style part is an object too); `false` for a code outside the scope, a version without an
/// object stream, a frame that does not hold the record, or a record that fails to decode.
pub fn decode_object(type_code: i16, version: DxfVersion, bytes: &[u8]) -> bool {
    if !VERSIONS.contains(&version) {
        return false;
    }
    let Some(kind) = SCOPE.iter().find(|k| k.type_code == type_code) else {
        return false;
    };
    let classes: Vec<(i16, &str, bool)> = SCOPE
        .iter()
        .filter(|k| k.type_code >= 500)
        .map(|k| (k.type_code, k.name, k.entity))
        .collect();
    crate::io::dwg::dwg_document_builder::decode_one_record(
        version,
        bytes,
        type_code,
        kind.entity,
        &classes,
    )
}

/// One record of a [`SCOPE`] kind taken from a DWG file, framed for [`decode_object`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScopeRecord {
    pub type_code: i16,
    pub version: DxfVersion,
    /// The framed record ([`frame_record`] of `data` and `handle_bits`).
    pub bytes: Vec<u8>,
    /// The merged data between the size prefix and the CRC.
    pub data: Vec<u8>,
    /// The R2010+ handle-stream bit count of the frame.
    pub handle_bits: i64,
}

/// The records of the [`SCOPE`] kinds in a DWG file that a full read decoded, in handle order.
pub fn scope_records(dwg: &[u8]) -> Result<Vec<ScopeRecord>, DxfError> {
    let mut reader = crate::io::dwg::DwgReader::from_stream(std::io::Cursor::new(dwg.to_vec()));
    reader.set_capture_raw_records(true);
    let outcome = reader.read_with_stats()?;
    let document = outcome.document;
    let version = document.version;
    let mut handles: Vec<u64> = document.raw_records.keys().copied().collect();
    handles.sort_unstable();
    let mut records = Vec::new();
    for handle in handles {
        let (raw_type_code, raw) = &document.raw_records[&handle];
        let name = if *raw_type_code >= 500 {
            document
                .classes
                .iter()
                .find(|class| class.class_number == *raw_type_code)
                .map(|class| class.dxf_name.trim().to_ascii_uppercase())
        } else {
            SCOPE
                .iter()
                .find(|k| k.type_code == *raw_type_code && k.type_code < 500)
                .map(|k| k.name.to_string())
        };
        let Some(kind) = name.and_then(|name| SCOPE.iter().find(|k| k.name == name)) else {
            continue;
        };
        records.push(ScopeRecord {
            type_code: kind.type_code,
            version,
            bytes: frame_record(version, &raw.data, raw.handle_bits),
            data: raw.data.clone(),
            handle_bits: raw.handle_bits,
        });
    }
    Ok(records)
}

/// Frames a record's merged data as the objects section stores it: MS size, the R2010+ MC
/// handle-stream bits, the data and its CRC-16.
pub fn frame_record(version: DxfVersion, data: &[u8], handle_bits: i64) -> Vec<u8> {
    let mut out = Vec::with_capacity(data.len() + 8);
    push_modular(&mut out, data.len() as u64, 15);
    if version >= DxfVersion::AC1024 {
        push_modular(&mut out, handle_bits.max(0) as u64, 7);
    }
    out.extend_from_slice(data);
    let crc = crc16(CRC16_SEED, &out);
    out.extend_from_slice(&crc.to_le_bytes());
    out
}

/// Little-endian groups of `bits` (15: 16-bit words, 7: bytes), the next bit above each group
/// set while more follow.
fn push_modular(out: &mut Vec<u8>, mut value: u64, bits: u32) {
    loop {
        let group = value & ((1 << bits) - 1);
        value >>= bits;
        let word = group | if value == 0 { 0 } else { 1 << bits };
        if bits == 15 {
            out.extend_from_slice(&(word as u16).to_le_bytes());
        } else {
            out.push(word as u8);
        }
        if value == 0 {
            return;
        }
    }
}
