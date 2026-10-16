# acadrust fork: provenance

This directory is a hardened fork of acadrust. Every change to it is one commit that
touches only `Core/third_party/acadrust/` (checked by `Core/scripts/check-fork-hygiene.sh`)
and one row in the patch table below. `Core/scripts/fork-export.sh` splits this history
into a standalone branch for publication under the MPL-2.0.

## Upstream

| Item | Value |
|---|---|
| Crate | acadrust 0.6.4 from crates.io |
| Repository | https://github.com/hakanaktt/acadrust |
| Tag / commit | v0.6.4 = 8d5c46d423941ad96575760bf77979f8c46156fb (`.cargo_vcs_info.json`) |
| Package SHA-256 | 94862189cab6e282ed0433a801a216cd1a712953813284a48fa81c67223e38f6 (`acadrust-0.6.4.crate`; the `checksum` crates.io and Cargo.lock record) |
| Licence | MPL-2.0 (`LICENSE`); attributions in `NOTICE` |

## Import

Commit `vendor(acadrust): import pristine 0.6.4 (LF-normalised, ODA PDF excluded)`.

1. Source: the `.crate` file above, checksum verified, unpacked with `tar -xzf`.
2. Line endings: the crate was packaged with CRLF; every text file was converted to LF
   (upstream's own repository stores LF). `src/` has no CRLF file in the index.
3. Copied byte for byte, CRLF kept (`-text` in the repository `.gitattributes`): `*.dwg`,
   `*.dxf`, `tests/fixtures/**`, `tests/issue51_rt_issues/**`,
   `examples/entity_atlas_assets/**`. Check: `tests/fixtures/viewports/reference-clip-activation.dxf`
   has SHA-256 `addc906c7fb62b1f8b26f93423903e8ed72452aed828d3c5da1234f837d198e4` (`Core/scripts/ci.sh`).
4. Not imported (the only deviations from the package):

| File in the package | SHA-256 | Reason |
|---|---|---|
| `src/docs/OpenDesign_Specification_for_.dwg_files.pdf` | `1ed2e02722862188120da606e4b6a816fa4014c96de68da2f84a2ecda09461e7` | ODA document, all rights reserved; it never enters any commit (decision D-6) |
| `.gitattributes` | `447f2514868d86c3f1ff0e4bbee1dd2c5bfe3a46c426b39fba51e130d69be2bd` | `* text=auto` would override the repository rules and renormalise the `.dxf` fixtures |
| `.gitignore` | `f3c4d278e7cb4446c639cd80140641fac49db1baf95573e37d49761ab0029f3e` | ignores `*.dwg`, `*.dxf` and `Cargo.lock`, so `git add` would silently drop fixtures |

5. Files after import: 360 (the package's 363 minus the three above).

## Upstream test baseline

`cargo test -p acadrust --locked` inside the host workspace, before any patch:
1780 passed, 0 failed, 35 ignored (rustc 1.98.1, aarch64-apple-darwin; the spike measured 1,780 passed).

## Patches

One row per patch commit, in commit order. "Commit" is the subject line (`git log --grep`
finds it). Origin says where the change came from; LibreDWG (GPL) is never consulted.

| ID | Commit | Files | Origin | Test |
|---|---|---|---|---|
| H6 | acadrust(H6): AllocBudget, bounded allocation helpers, DxfError limits | src/io/limits.rs; src/io/mod.rs; src/error.rs; src/io/dwg/dwg_reader.rs; Cargo.toml; tests/hardening_common/mod.rs; tests/hardening_limits.rs | This fork (original work) | tests/hardening_limits.rs; src/io/limits.rs::hardening_tests |
| H1 | acadrust(H1): bounds-checked AC21 LZ77; checked compr_len | src/io/dwg/decompressor_ac21.rs; src/io/dwg/compressor_ac21.rs; src/io/dwg/dwg_reader.rs; Cargo.toml; tests/hardening_h1_ac21.rs | This fork (original work); algorithm unchanged from upstream | tests/hardening_h1_ac21.rs; src/io/dwg/dwg_reader.rs::hardening_tests |
| H2 | acadrust(H2): validate AC18 decompressed size before allocating | src/io/dwg/decompressor_ac18.rs; src/io/dwg/dwg_reader.rs; src/io/limits.rs; Cargo.toml; tests/hardening_h2_ac18.rs | This fork (original work) | tests/hardening_h2_ac18.rs; src/io/dwg/dwg_reader.rs::hardening_tests |
| H3 | acadrust(H3): cap section and page sizes; try_reserve under budget | src/io/dwg/dwg_reader.rs; Cargo.toml; tests/hardening_h3_sections.rs; tests/hardening_h2_ac18.rs | This fork (original work) | tests/hardening_h3_sections.rs; src/io/dwg/dwg_reader.rs::hardening_tests |
| H5 | acadrust(H5): no over-read into zero padding; count page substitution | src/io/dwg/decompressor_ac21.rs; src/io/dwg/dwg_reader.rs; Cargo.toml; tests/hardening_h5_overread.rs | This fork (original work); policy D-3 (failsafe keeps page zero-fill, every event counted as page-zero-filled; strict errors) | tests/hardening_h5_overread.rs; src/io/dwg/dwg_reader.rs::hardening_tests |
| C1 | acadrust(C1): CancelToken checked at stage, page and batch boundaries | src/io/limits.rs; src/io/dwg/dwg_reader.rs; src/io/dwg/dwg_document_builder.rs; Cargo.toml; tests/hardening_c1_cancel.rs | This fork (original work) | tests/hardening_c1_cancel.rs |
| H4 | acadrust(H4): clamp DXF HATCH 93, LTYPE 73 and DGN counts | src/io/dxf/reader/section_reader.rs; Cargo.toml; tests/hardening_h4_dxf.rs; tests/hardening/h4_*.dxf | This fork (original work); clamp pattern from upstream (HATCH pattern-boundary reader) | tests/hardening_h4_dxf.rs |
| F17 | acadrust(F17): per-entity DXF failsafe; linear block membership | src/document.rs; src/tables/mod.rs; src/io/dxf/reader.rs; src/io/dxf/reader/section_reader.rs; src/io/dxf/reader/text_reader.rs; Cargo.toml; tests/hardening_f17_dxf.rs | This fork (original work) | tests/hardening_f17_dxf.rs (complexity test: --release --ignored); src/document.rs::hardening_f17_tests; src/io/dxf/reader/section_reader.rs::hardening_f17_tests |
| F11 | acadrust(F11): provenance inventory of LibreDWG-citing files | PROVENANCE.md | This fork (inventory; no code change) | `git grep` command in "LibreDWG-citing files (F11 inventory)" |
| F12 | acadrust(F12): remove from_mmap; drop PERF and ACADRUST_RAW_ALL env reads | src/io/dwg/dwg_reader.rs; src/io/dwg/dwg_document_builder.rs; src/document.rs; Cargo.toml; tests/raw_all_acds.rs; tests/raw_all_compound.rs; tests/raw_all_exclusions.rs; tests/hardening_surface.rs | This fork (original work); raw-record capture kept behind DwgReader::set_capture_raw_records (off by default) | tests/hardening_surface.rs; src/io/dwg/dwg_reader.rs::hardening_tests |
| F16 | acadrust(F16): gate DWG/DXF writers and import behind features | Cargo.toml; src/lib.rs; src/io/mod.rs; src/io/dwg/mod.rs; src/io/dwg/file_headers/mod.rs; src/io/dxf/mod.rs; src/io/dwg/embedded_entity.rs; src/io/dwg/r12/mod.rs; src/io/loft_parameters.rs; src/io/loft_surface_curves.rs; src/io/surface_history.rs; src/objects/mod.rs; 13 upstream unit-test modules (cfg only); tests/hardening_f16_readonly.rs | This fork (original work); writers stay in the fork for upstream tests and the fixture generator (decision D-7) | tests/hardening_f16_readonly.rs; src/io/dwg/embedded_entity.rs::hardening_tests (opaque without writers, typed with writers) |
| F9 | acadrust(F9): cherry-pick upstream 0e46236 (#134 auxiliary classes) | src/io/dwg/dwg_document_builder.rs; src/io/dwg/dwg_reader.rs; tests/issue134.rs; Cargo.toml | Upstream acadrust 0e4623614469db8e1eb168953884649f1a961868 (MPL-2.0, author Hakan Ak); context conflicts in dwg_document_builder.rs (upstream parent b62555f #133, fork patch C1) resolved by hand; added/removed lines identical to upstream (`git patch-id --stable` of the -U0 diff = 78ac6a65205edf9951c72c704ba53c7966cbff4d) | tests/issue134.rs (upstream); tests/hardening_f9_sweep.rs (this fork: damaged-record sweep, no panic escapes the recovery) |
| F10 | acadrust(F10): skip CRC-valid empty type-55 stubs silently | src/io/dwg/dwg_document_builder.rs; src/io/dwg/dwg_stream_readers/object_reader/mod.rs; Cargo.toml; tests/hardening_f9_f10.rs | This fork (original work); stub shape from the spike verifier (11-byte CRC-valid records of the unused fixed type 55) | tests/hardening_f9_f10.rs |
| F3 | acadrust(F3): decode AcDbImpBlockRefPathObjectId, persistent subent ids, VX flags | src/io/dwg/dwg_stream_readers/object_reader/associative.rs; src/io/dwg/dwg_stream_readers/object_reader/tables.rs; src/io/dwg/dwg_document_builder.rs; Cargo.toml; tests/hardening_f3_decode.rs | Re-derived (see "F3 provenance"); LibreDWG consulted: no | tests/hardening_f3_decode.rs |

## Limits (H2, H3)

| Limit | Value | Why |
|---|---|---|
| AC18 decompressed page | 16 MiB (`AC18_MAX_PAGE_DECOMPRESSED`) | Real pages are 0x7400 bytes; 16 MiB leaves a 500× margin. |
| AC21 page buffer | 100,000,000 bytes (`AC21_MAX_PAGE_BUFFER`) | Upstream's own `ac21_page_layout` limit, now also applied to the decompressed size. |
| Decompressed section | min(max(file length × 64, 64 MiB), pages × 100 MB) | Measured expansion on real drawings is 2.6× (median) to 5.1× (max); 64× keeps a 12× margin, the 64 MiB floor protects small files. |
| AC15 raw section | file length | R13–R2000 sections are stored uncompressed, so a larger size cannot be real. |
| Preview container | file length | The container lies inside the file; a larger claimed size leaves the preview absent instead of allocating it. |
| Any reservation above | caller's `AllocBudget` | Spec 2.5: sizes read from the file are bounded by format constants and by the byte budget. |

A refused budget (`BudgetExceeded`) is not file corruption: it is returned by `read*` in failsafe mode too, never turned into a diagnostic. A size above a format limit (`LimitExceeded`) is corruption: that section read fails and the document read records an `*-extract-failed` diagnostic and carries on, like any other unreadable section (a file-header failure is an error in strict mode).

Acceptance: no synthetic, public or corpus file is rejected by these caps (T10 corpus gate).

## LibreDWG-citing files (F11 inventory)

LibreDWG is GPL. No fork patch consults it, and no LibreDWG source or binary enters this
repository. Upstream acadrust 0.6.4 cites LibreDWG, or its `dwg.spec`, `dwg2.spec` and `acds.spec`
files, in comments in the 12 files below. This inventory changes no code. What to do with the
flagged decoders is founder decision C21 (design §11 Q1); fork patches do not edit these lines.
Regenerate the list with:

    git grep -n -i -E 'libredwg|dwg2?\.spec|acds\.spec' -- Core/third_party/acadrust/src

| File | Citing lines (at F11) | What follows the cited layout or remark | Built into the shipped reader | Flag |
|---|---|---|---|---|
| src/objects/object_context_data.rs | 34, 47, 88, 99 | `ObjectContextKind` placement payloads ("follow LibreDWG dwg2.spec"), the MText field order, the Dimension subtype tails and the `class_version` default remark | yes | **C21**: annotative representations (spec P0), MULTILEADER context data |
| src/io/dwg/dwg_stream_readers/object_reader/entities.rs | 5263, 5275, 5298, 5382, 5403, 5825, 6163, 6211 | `read_acis_entity_impl` (SAT/SAB framing, COMMON_3DSOLID) and `read_wire` (`Dwg_3DSOLID_wire`); 6163 and 6211 are unit tests | yes | **C21**: 3DSOLID/REGION/BODY layout |
| src/io/dwg/dwg_stream_writers/object_writer/entities.rs | 3177, 3179, 4370, 5614, 5657, 5789 | writer: `write_mline` cap flags, `write_multileader`, `write_acis_data_impl`, `write_acis_wireframe` | no (writer; gated by F16) | MULTILEADER / 3DSOLID layout |
| src/io/dwg/dwg_stream_writers/object_writer/objects.rs | 21, 1589 | writer: resbuf value types, `write_dictionary` item handles | no (writer; gated by F16) | — |
| src/io/dwg/dwg_writer.rs | 1975 | writer: `build_acds_prototype` (`acds.spec`) | no (writer; gated by F16) | — |
| src/io/dxf/writer/section_writer.rs | 7063 | writer: remark on dictionary group code 360 | no (writer; gated by F16) | — |
| src/io/dwg/dwg_document_builder.rs | 7498, 7535 | `decode_section_view_style` (class 795) field order | yes | non-graphical decoder |
| src/objects/class_object.rs | 4 | module doc: payloads of class-based non-graphical objects | yes | **C21** scope: non-graphical decoders |
| src/io/dxf/reader/section_reader/associative.rs | 999 | `read_dimension_association_dxf`: remark that `dwg2.spec` orders the dimension handle differently | yes | — |
| src/objects/associative.rs | 1356 | class-name alias remark ("seen in Autodesk/LibreDWG class tables") | yes | — |
| src/classes/mod.rs | 168 | remark that readers (libredwg, AutoCAD) resolve classes by index | yes | — |
| src/io/dwg/reed_solomon.rs | 697–775 (22 lines) | unit tests only: Reed-Solomon tables cross-checked against values from LibreDWG `reedsolomon.c` | no (`#[cfg(test)]`) | test vectors |

## F3 provenance

The spike carried a draft of this fix (`docs/spike/kod/yamalar/okunamayan-kayitlar.patch`
in the downstream repository). That draft was **not copied**: the layouts below were re-derived and the
code was written again for this commit. **LibreDWG consulted: no** (neither its source nor its spec
files).

| Fix | Layout | Sources |
|---|---|---|
| A: `AcDbImpBlockRefPathObjectId` dependency name | After `has_name` + `name`, a dependency whose name is a compound-object-id class stores that class's payload: BS version (always 0), BL path length, then per element RS tag (0x0001) + handle | Spike bit traces of corpus records (aggregate only): the tag was 0x0001 in 226 of 226 elements, and with this layout all 172 ACDBASSOCGEOMDEPENDENCY records in the corpus end exactly at their last bit; acadrust's own `read_dependency` frame (field order before and after the payload) |
| B: persistent sub-entity id | R2013+ (AC1027+): B flag + BL class code; R2007–R2010: TV class name (`AcDbAssocEdgePersSubentId` → code 3). Code 1: no fields; code 2: BS, BL, BL, B; code 3: BS, BL, BL; unknown codes keep upstream's greedy BL loop; the compound-object bit ends the record | Spike bit traces: 138 records; the 5 code-2 records that overran by 2 bits now end exactly. The R2007–R2010 text form is inferred from the same traces and is the least certain part |
| C: VX_TABLE_RECORD xref flags | Same version-dependent table-record layout as every other table record: R2007+ stores only BS xrefindex+1 (Xdep is bit 0x100); R13–R2000 store B 64-flag, BS xrefindex+1, B Xdep | acadrust's own `read_xref_table_flags`, already used by the other table-record readers; ACadSharp `DwgObjectReader.readXrefDependantBit` (MIT); the common table-record fields of the ODA Open Design Specification for .dwg files (consulted for reference; the document is not part of this fork); spike traces: all 12 VX records then end exactly and their CRCs are valid |

Interactions:
- **F9** (upstream #134) recovers ACDBASSOCGEOMDEPENDENCY records that still fail to decode as opaque
  objects. With F3 most of them now decode, so fewer reach F9's recovery path; both are covered by
  `tests/issue134.rs` and `tests/hardening_f3_decode.rs`.
- **F1** (T37) boxes the `ParsedEntry` table variants and edits the same `OBJ_VPENT_HDR` arm of
  `dwg_document_builder.rs`. The merged form (spike lessons §3.1) that T37 applies is:

      OBJ_VPENT_HDR => {
          let data =
              tables::read_vx_table_record(&mut reader, self.obj_reader.version());
          Some(ParsedEntry::Vx(obj_handle, Box::new(data)))
      }

- Upstream issues to file (not done here; no remote is used): the compound-object-id payload (#135)
  and the R2007+ VX flags.
