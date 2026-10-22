# acadrust (hardened fork): provenance

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
Corrections to the documents themselves are `vendor(acadrust):` commits and carry no row.
So are renames that change no behaviour (test file names, fixture directories, comments).
`Core/scripts/fork-export.sh --verify` compares these rows with the `acadrust(<ID>):`
commits one by one, in order.

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
| F9 | acadrust(F9): sweep damaged auxiliary records for panics | Cargo.toml; PROVENANCE.md; tests/hardening_f9_sweep.rs | This fork (original work); test only, no change under src/ | tests/hardening_f9_sweep.rs |
| F3 | acadrust(F3): decode AcDbImpBlockRefPathObjectId, persistent subent ids, VX flags | src/io/dwg/dwg_stream_readers/object_reader/associative.rs; src/io/dwg/dwg_stream_readers/object_reader/tables.rs; src/io/dwg/dwg_document_builder.rs; Cargo.toml; tests/hardening_f3_decode.rs | Re-derived (see "F3 provenance"); LibreDWG consulted: no | tests/hardening_f3_decode.rs |
| H4 | acadrust(H4): clamp and stop the HATCH 95, 96, 97 and 98 count loops | src/io/dxf/reader/section_reader.rs; tests/hardening_h4_dxf.rs; tests/hardening/h4_hatch9*_eof.dxf; tests/hardening/h4_hatch_lists_huge.dxf | This fork (original work); second H4 commit, the first commit's clamp-and-stop pattern applied to the seed-point, spline knot, control-point and fit-point and source-object loops | tests/hardening_h4_dxf.rs |
| H3 | acadrust(H3): checked offset sum in the AC1021 page map | src/io/dwg/dwg_reader.rs | This fork (original work); found by the T24 seed smoke test (debug-build `attempt to add with overflow` on mutated AC1021 files) | src/io/dwg/dwg_reader.rs::hardening_tests |
| F15 | acadrust(F15): public read_visiting_with_stats | src/io/dwg/dwg_reader.rs; Cargo.toml; tests/hardening_f15_f18_f4.rs | This fork, new code (thin public wrapper over the private read_with_optional_visitor) | tests/hardening_f15_f18_f4.rs::f15_visit_with_stats_matches_full_read_stats |
| F18 | acadrust(F18): public canonical block membership accessor | src/document.rs; tests/hardening_f15_f18_f4.rs | This fork, new code (read-only view of the pub(crate) block_entity_handles; rationale: visitor-consumed entities never reach BlockRecord::entity_handles, design V1) | tests/hardening_f15_f18_f4.rs::f18_* |
| F4 | acadrust(F4): keep raw viewport status bits; expose is_off | src/entities/viewport.rs; src/entities/mod.rs; src/io/dwg/dwg_document_builder.rs; src/io/dxf/reader/section_reader.rs; src/io/dwg/legacy_viewport.rs; tests/hardening_f15_f18_f4.rs | This fork, new code (spike §2.6: from_bits drops 0x20000) | src/entities/viewport.rs::hardening_tests, tests/hardening_f15_f18_f4.rs::f4_* |
| F1 | acadrust(F1): box ParsedEntry table variants | src/io/dwg/dwg_document_builder.rs; Cargo.toml; tests/hardening_viewer_mem.rs | The spike patch bellek-yarilama.diff hunks 1–5, re-applied by hand; OBJ_VPENT_HDR in the merged F3 form (spike §3.1) | tests/hardening_viewer_mem.rs::f1_appid_heavy_read_has_no_huge_single_allocation + compile-time size assert |
| F2' | acadrust(F2): viewer option to skip bulky XRecords and ProxyObjects | src/io/limits.rs; src/io/dwg/dwg_reader.rs; src/io/dwg/dwg_document_builder.rs; tests/hardening_viewer_mem.rs | This fork; spike hunks 6–7 reworked: DwgReadOptions field instead of ACADRUST_SKIP_BULKY_OBJECTS, small XRecords kept (gaps §0.5); an XRecord whose entries could not be decoded completely is measured by its raw data | tests/hardening_viewer_mem.rs::f2_viewer_mode_drops_bulky_xrecords_but_keeps_scales_and_overrides; src/io/limits.rs::hardening_tests |
| F13 | acadrust(F13): viewer option to drop EntityCommon::raw_record | src/io/limits.rs; src/io/dwg/dwg_document_builder.rs; tests/hardening_viewer_mem.rs | This fork, new code (gaps C2) | tests/hardening_viewer_mem.rs::f13_drop_raw_records_keeps_graphic_data |
| P1 | acadrust(P1): read the inline names of a colour-book entity colour | src/io/dwg/dwg_stream_readers/bit_reader.rs; Cargo.toml; tests/hardening_p1_line_decode.rs; PROVENANCE.md | This fork, new code; layout re-derived from ODA spec 2.11 and bit traces of five public drawings (see "P1 provenance"); LibreDWG consulted: no | tests/hardening_p1_line_decode.rs |
| P3 | acadrust(P3): read ACAD_TABLE linked-data flags and the header-suppressed override as written | src/io/dwg/dwg_stream_readers/object_reader/entities.rs; src/io/dwg/dwg_stream_writers/object_writer/entities.rs; PROVENANCE.md | This fork, new code; the override rule is ODA spec 20.4.96.1, the linked-data flags word comes from bit traces of public drawings (see "P3 provenance"); LibreDWG consulted: no | src/io/dwg/dwg_stream_readers/object_reader/entities.rs::tests (table_cell_with_linked_data_flags_of_three_reads_its_data_link, table_override_with_the_header_flag_reads_no_bit_for_it) |
| P3 | acadrust(P3): count records of known non-graphical classes apart from skipped ones | src/io/dwg/dwg_document_builder.rs; src/io/dwg/dwg_reader.rs; src/io/read.rs; Cargo.toml; tests/hardening_p3_nongraphical.rs; PROVENANCE.md | This fork, new code; extends the F9 auxiliary-object mechanism (see "P3 provenance"); LibreDWG consulted: no | tests/hardening_p3_nongraphical.rs |
| M1 | acadrust(M1): CIF escapes take exactly four hex digits in MTEXT and the readers | src/entities/mtext_format/parser.rs; src/io/dxf/code_page.rs | This fork, new code; the rule decode_cif_escapes already documented (exactly four hex digits, a surrogate pair is one scalar, a malformed escape stays literal, a lone surrogate is dropped) now holds in the MTEXT parser too, which read every hex digit that followed (`25\U+00B0C` became U+0B0C) and dropped a malformed `\U+` (`\U+ZZZZ` became `ZZZZ`); decode_cif_escapes no longer takes a sign (`u16::from_str_radix` accepts `+041`); LibreDWG consulted: no | src/entities/mtext_format/parser.rs::tests (test_parse_unicode_escape_takes_exactly_four_hex_digits, test_parse_malformed_unicode_escape_stays_literal, test_parse_unicode_escape_surrogates_match_the_reader); src/io/dxf/code_page.rs::tests::test_decode_cif_escapes |
| M2 | acadrust(M2): undeclared DXF text falls back to Windows-1252 in the text and binary readers | src/io/dxf/code_page.rs; src/io/dxf/reader/text_reader.rs; src/io/dxf/reader/binary_reader.rs | This fork, new code; a DXF without `$DWGCODEPAGE` (common before R2007, R12 binary files included) kept UTF-8 first, but text that was not UTF-8 became Latin-1 in the text reader (0x80-0x9F as C1 controls) and lossy UTF-8 (U+FFFD) in the binary reader; both now decode it as Windows-1252, the system code page AutoCAD reads such files in on Western systems, so every byte is a character and the two readers agree; files that declare a code page are unchanged; LibreDWG consulted: no | src/io/dxf/reader/text_reader.rs::tests::test_undeclared_non_utf8_value_decodes_as_windows_1252; src/io/dxf/reader/binary_reader.rs::tests::test_undeclared_non_utf8_string_decodes_as_windows_1252 |
| M3 | acadrust(M3): MTEXT numeric code values parse in linear time | src/entities/mtext_format/parser.rs | This fork, new code; a numeric code value (`\C`) that was not a number was found only after a scan to the next `;` and then rewound, so a value of repeated `\C` codes without `;` took quadratic time (32,768 of them: 12 s in a debug build); the scan now stops at the first character that cannot belong to a signed integer, with the same result for every input; LibreDWG consulted: no | src/entities/mtext_format/parser.rs::tests (test_parse_numeric_code_value_semantics, test_parse_numeric_code_values_take_linear_time) |
| M4 | acadrust(M4): \X breaks the line in MTEXT | src/entities/mtext_format/parser.rs; src/entities/mtext_format/mod.rs | This fork, new code; the parser dropped `\X` (wrap at the dimension line), so the text after it ran on in the same line (`25\X+0.1` read as `25+0.1`); it now starts a new paragraph that keeps the current paragraph's properties (span properties carry over as with `\P`), so layout puts it on the next line and plain text separates it with a line feed; LibreDWG consulted: no | src/entities/mtext_format/parser.rs::tests::test_parse_dimension_line_wrap_breaks_the_line |
| S1 | acadrust(S1): decode the STYLE TrueType typeface before the entities are visited | src/io/dwg/dwg_document_builder.rs; Cargo.toml; tests/hardening_s1_style_typeface.rs; PROVENANCE.md | This fork, new code; upstream decoded the typeface and its flags from the STYLE's `ACAD` EED in a post-pass after Pass 2, so an entity visitor (F15) saw every TrueType style with an empty family (a downstream converter laid such text out through the STYLE's SHX file in a visit and through the family in a full read); the same decoding now runs right after the table records are built, before Pass 2, with the same result for a full read; LibreDWG consulted: no | tests/hardening_s1_style_typeface.rs |
| L1 | acadrust(L1): read the MLINESTYLE flags in DXF | src/io/dxf/reader/section_reader.rs; Cargo.toml; tests/hardening_l1_mlinestyle.rs; PROVENANCE.md | This fork, new code; the DXF reader skipped group 70 of MLINESTYLE, so every style read from DXF lost its fill, joint and cap flags (a downstream viewer drew no fill and no caps); it now reads 70 with the DXF bit values of the DXF reference (1 fill, 2 joints, 16/32/64 start square, inner arcs, round, 256/512/1024 the same at the end), the inverse of the fork's own DXF writer (`MLineStyleFlags::from_bits`; the DWG reader's mapping of its swapped bits is unchanged); ezdxf (MIT) reads the same values; LibreDWG consulted: no | tests/hardening_l1_mlinestyle.rs |
| Z1 | acadrust(Z1): single-record decode entry for fuzzing behind a feature | src/fuzzing.rs; src/lib.rs; src/io/dwg/dwg_document_builder.rs; Cargo.toml; tests/hardening_z1_record.rs; tests/fixtures/z1/records.{ac1014,ac1015,ac1018,ac1021,ac1032}.dwg; PROVENANCE.md | This fork, new code; feature `fuzzing` (off by default; a downstream shipped build never enables it): `fuzzing::decode_object(type_code, version, bytes)` runs one framed object record (MS size, R2010+ MC handle bits, merged data, CRC) through the pass-2 dispatch of a full read with an empty context and a fixed class table, without `catch_unwind`, for LEADER, TOLERANCE, MLINE, HATCH, the proxy entity, MULTILEADER, SPATIAL_FILTER and the DGN line-style definition and component classes (`fuzzing::SCOPE`); `true` when the record decodes without a reader error and the builder keeps an entity (an entity kind) or an object (an object kind) of it. `fuzzing::scope_records` takes those records out of a DWG file for tests and fuzz seeds. The version takes a `DxfVersion` (not a `DwgVersion`): the R2013+ entity preamble differs from R2010's under one `DwgVersion`. The fixtures are written by this fork's own writer (an ignored test regenerates them); no decoder changes; LibreDWG consulted: no | tests/hardening_z1_record.rs; the `compile_fail` doctest in src/lib.rs (no `fuzzing` module without the feature) |
| L1 | acadrust(L1): decode DGN line-style definitions and components | src/io/dwg/dwg_stream_readers/object_reader/dgn_linestyle.rs; src/objects/dgn_linestyle.rs; src/io/dwg/dwg_stream_writers/object_writer/objects.rs; src/io/dwg/dwg_document_builder.rs; Cargo.toml; tests/hardening_l1_dgn.rs; tests/fixtures/z1/records.{ac1014,ac1015,ac1018,ac1021,ac1032}.dwg; PROVENANCE.md | This fork, new code; every `LSDEFINITION` and the stroke patterns with a phase mode other than 0 failed to decode (counted known non-graphical by P3); the layouts were re-derived from the bit traces of the public drawings' records (aggregates only, see "L1 provenance"): the definition stores its units and flags before a uid that is its root component's, the stroke pattern a BL phase mode before the iteration limit and the phase before the auto phase, a stroke its cap mode before its width mode; the writer (feature `writers`) follows, the Z1 fixtures are rewritten by their generator; the ODA document was not needed; LibreDWG consulted: no | tests/hardening_l1_dgn.rs |
| X1 | acadrust(X1): SPATIAL_FILTER matrices are stored row by row | src/io/dwg/dwg_document_builder.rs; src/io/dwg/dwg_stream_readers/object_reader/objects.rs; src/io/dwg/dwg_stream_writers/object_writer/objects.rs; src/io/dxf/reader/section_reader.rs; src/io/dxf/writer/section_writer.rs; src/objects/stub_objects.rs; Cargo.toml; tests/hardening_x1_spatial_filter.rs; tests/fixtures/z1/records.{ac1014,ac1015,ac1018,ac1021,ac1032}.dwg; PROVENANCE.md | This fork, new code; the inverse block transform and the clip bound transform of a SPATIAL_FILTER (DWG: 12 BD each; DXF: 12 group-40 doubles each) are three rows of four with the translation last in each row; upstream read and wrote them column by column, so every translation and rotation landed in the wrong cells (see "X1 provenance"); reader and writer of both formats follow, the Z1 fixtures are rewritten by their generator; the field docs say what the matrices and code 71 hold; sources: ezdxf (MIT) `SpatialFilter` loading, the public drawings' filters; LibreDWG consulted: no | tests/hardening_x1_spatial_filter.rs; the builder's and the DXF reader's unit tests |
| A1 | acadrust(A1): re-derived annotative context decoder | src/objects/object_context_data.rs; src/io/dwg/dwg_stream_readers/object_reader/object_context.rs; src/io/dwg/dwg_stream_readers/object_reader/mod.rs; src/io/dwg/dwg_stream_readers/merged_reader.rs; src/io/dwg/dwg_document_builder.rs; src/io/dwg/dwg_stream_writers/object_writer/objects.rs; src/io/dxf/reader/section_reader.rs; src/io/dxf/writer/section_writer.rs; src/document.rs; tests/annotative_context_encoder.rs; tests/dxf_autocad_conformance.rs; tests/issue134.rs; PROVENANCE.md | This fork, new code (founder decision C21, option b): the context types and the DWG decoder of the annotative representations (`AcDb*ObjectContextData` and the two hatch context classes) are written again from the layouts re-derived in "A1 provenance"; the upstream decoder and its hatch-loop helper and regression test are removed without having been read; a record is modelled only when its payload ends exactly with the record, otherwise it keeps its scale and stays verbatim as an unknown object; classes with no record to derive from (feature control frame, ordinate and large radial dimension) are not modelled; the DWG context encoder mirrors the decoder; the DXF reader and writer map their group codes to the new types (the TEXT context angle is degrees in DXF, as every DXF angle); sources: the ODA document (reference only), ACadSharp (MIT) `DwgObjectReader` (common fields, block reference payload), DXF group codes of public drawings, bit traces of the public and further corpora; LibreDWG consulted: no | object_reader::object_context::tests; tests/annotative_context_encoder.rs |

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
C21 was decided as option (b): fork patch A1 replaces the first row's decoders (see "A1 provenance").
Regenerate the list with:

    git grep -n -i -E 'libredwg|dwg2?\.spec|acds\.spec' -- Core/third_party/acadrust/src

| File | Citing lines (at F11) | What follows the cited layout or remark | Built into the shipped reader | Flag |
|---|---|---|---|---|
| src/objects/object_context_data.rs | (none since A1) | Replaced by fork patch A1: types and DWG decoder re-derived without the flagged layouts | yes | **C21** resolved: option (b), A1 |
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

## P1 provenance

Five public encoding-test drawings (R2004, R2007 twice, R2013 twice, all from the LibreCAD and
libdxfrw test data) lost two of their three LINE records to `record-decode-failed` (type 19,
"Truncated DWG stream"). **LibreDWG consulted: no** (no source or spec file read). Its `dwgread`
command-line tool was run on the same files as a black-box oracle (entity counts and the LINE
coordinates of the output only), as `cf parity` does. The layout was found from the bit trace of
the failing records (aggregate facts only; the drawings are not part of this repository).

| Item | Finding |
|---|---|
| Where | The entity colour (ENC, ODA spec 2.11) of the two lost lines: BS `0xC300`, i.e. flags `0x8000` and `0x4000` (book colour, AcDbColor handle in the handle stream, no RGB long) plus `0x0300`. The third line of each drawing has a true colour with transparency (`0xA100`) and decoded all along |
| What follows the BS | Two texts in the main stream, each BS length (the NUL counts) plus bytes in the document code page: the colour name (`Entry`, `Linked`) and the book name (`Book`). They are inline 8-bit text in R2004, R2007 and R2013 alike: the R2007+ text stream is empty (its flag bit is 0), and no AcDbColor object exists in the file |
| Evidence | Read as the line style scale, the next fields slid and the LINE's first coordinate was requested 14 to 22 bits before the end of the data. With the two texts consumed, every field of the preamble lands (line weight 29, layer and book handles at the end of the handle stream) and the coordinates are the oracle's, (5, 6)-(7, 8) and (9, 10)-(11, 12), with the data ending on the padding that precedes the handle stream |
| Rule | Colour-book colour (`0x4000`) with bit `0x0200` set: the colour name (only when bit `0x0100` is set too) and then the book name follow, each as BS length plus bytes (the `&1` / `&2` convention of the CMC colour byte), before a transparency long if any. The discriminator is `0x0200` because the low bits hold the ACI fallback index, 0 to 257: ByLayer (256) and None (257) set `0x0100`, and the fork's own writer emits `0xC100` and `0xC101` for such book colours (upstream's issue 132 tests failed on a first version of the rule that tested `0x0100`). No ACI value reaches `0x0200`. Every other drawing seen (59 local and 1,403 public ones, except the few above 120 MB; 8 book-colour flag patterns, `0xC007` to `0xC0FE`) has `0xC0` plus an index below 256 |
| Not observed | A book colour with only bit `0x0200`, or together with transparency: the book-name-only reading and the order (names before the transparency long) are the CMC order, with no sample behind them. The five drawings have both bits |
| Not fixed | In the same drawings the table control objects (BLOCK_CONTROL and, in one file, the other control objects) store the whole record length as the main size, leaving no handle stream, so their common data fails and the file reports `record-decode-failed` for types 48 to 68. No entity is lost by it (the block records and all three lines decode), and the header variables of two of the drawings fail to decode too. Both are the same writer's deviations and stay open |

## P3 provenance

Public drawings whose ACAD_TABLE entities failed to decode (`record-decode-failed`, class names
lost to the internal type codes -16 for ACAD_TABLE and 105 for TABLECONTENT: 54 and 6 records).
**LibreDWG consulted: no.** Both causes are field-level readings of records the reader already
framed correctly, found with a bit trace of the failing records (aggregate facts only).

| Item | Finding |
|---|---|
| Linked-data flags (R2010+, TABLECONTENT cells) | "Has linked data flags" (BL 92) is a flags word. Four R2013 drawings store 3 on the cell that carries the table's data link (the other cells 0), then the link handle and three BL numbers (row and column count of the linked range, one unknown). The reader took `== 1` as the only value, left those fields in the stream, read the next count from them, and the whole record failed (every table of the four drawings). Now any non-zero word reads the link fields. All four drawings then decode all tables, cell by cell, to the last bit |
| Header-suppressed override (pre-R2010 tables) | In the table flag override (BL 93), bit 0x0002 "header suppressed" is "always true (do not read any data for this)" in the ODA spec. The reader and the writer both handled it as a bit. The 24 tables of two R2004/R2007 sheet templates that set 0x0002 (flags 0x40001b) ended their override one bit too far and failed; the 22 that did not decoded. Now the reader reads nothing and reports `Some(true)`, and the writer writes nothing. Both templates then decode 46 of 46 tables |
| Not changed | The cell-content type 8 seen in the traces of the broken reads was a consequence of the first cause, not a type to decode. Nothing else of the table readers moved |

The second P3 commit does not change what any decoder reads. In the same public drawings, objects
of classes that never draw anything failed to decode, were counted as skipped records and made a
strict read (failsafe off) fail. They are now kept raw when their framing and common data are
sound (the F9 mechanism: `ObjectType::Unknown` with the raw record) and counted apart otherwise;
neither case is a skipped record or an error.

| Item | Finding |
|---|---|
| The list | Fixed type 81 (VBA_PROJECT), and the classes `LSDEFINITION`, `LSSTROKEPATTERNCOMPONENT`, `BREAKDATA`, `ACDBASSOC2DCONSTRAINTGROUP`, `ACDBASSOCPLANESURFACEACTIONBODY` and every `ACSH_*` (solid history: extrusion, sweep, revolve, loft, boolean and so on). Names are compared in upper case without stray characters; a class the table marks as an entity is never in the list. In the 1,403 public drawings (at least, the reader caps its diagnostics at 1,000 a file): VBA_PROJECT 167 records in 167 drawings, LSDEFINITION 2,301 in 8, LSSTROKEPATTERNCOMPONENT 97 in 9, ACSH_* 102 in 2, ACDBASSOC2DCONSTRAINTGROUP 13 in 10, BREAKDATA 2, ACDBASSOCPLANESURFACEACTIONBODY 1 |
| Reporting | `DwgBuildOutcome::known_nongraphical_records` and `ReadStats::known_nongraphical_records`; diagnostics `known-nongraphical-preserved` and `known-nongraphical-skipped`; the notification is a warning, so the record is not among the recovered errors either. `skipped_source_records` no longer counts them, and the strict-read failure on a skipped record no longer fires for them |
| Not decoded | None of the classes gains a decoder. The line-style definitions are Plan 4's work (complex line types; fork patch L1 decodes them); the others have no use in a viewer |

## L1 provenance

`LSDEFINITION` and part of `LSSTROKEPATTERNCOMPONENT` failed to decode in the public drawings (P3
counted 2,301 and 97 of them as known non-graphical). **LibreDWG consulted: no.** The layouts were
re-derived from the bit traces of every line-style record of the public drawings (63 files, 94,680
records, all R2013 and R2018) and of a further set of nine drawings (1,252 records); only aggregate
facts are recorded here, and the drawings are not part of this repository. The ODA document was not
needed.

| Item | Finding |
|---|---|
| Every record | TV description (the R2007+ text stream), BL version (1 in every record), BL kind: 0 in every definition; 1 symbol, 2 compound, 3 stroke, 4 point, 6 internal in the components |
| Component header | 16 bytes uid, BD scale, RC flags. The uid is an RFC 4122 version 1 GUID in every component (version nibble 1 in byte 7, variant 10 in byte 8). Upstream's header and its symbol, compound, point and internal layouts already end every such record exactly |
| Definition | BD unit definition, BL units type, BD unit scale, five B, 16 bytes; handle stream: the root component. All 2,641 definitions end exactly after the 16 bytes, which equal the uid of the component the handle names in every definition whose root is present (2,597; the other 44 hold a null root). The unit definition is 1.0 with units type 0, and 3.28e-6 or 0.001 with units type 1 (drawing units per UOR: feet per micrometre, metres per millimetre); the unit scale is 1.0 in every record. Upstream read the uid right after the kind and then the flags and units, which ran out of bits in every record |
| Definition flags | The five B are named in upstream's order (continuous first, then element, physical, scale independent, snappable); the first is 0 in every record, the patterns of the others are 01000, 00100, 01010 and 00011. Their meaning is not confirmed by the traces |
| Stroke pattern | B, B, BL phase mode (0, 1 or 2), BL (0 or 1; 1 wherever the first B is set), BD (0, 3.5, 4, 10: with mode 0 only), BD (0, or 1 with mode 1), BL stroke count. Read this way all 48,803 stroke patterns end exactly; upstream's order (BL, BD, BD, B, B) reads the same bits only when the mode and both doubles are 0, and the 142 with mode 1 failed. Named iteration-limit flag, single segment, phase mode, iteration limit, phase, auto phase |
| Stroke | Five B, BD length, BD start width, BD end width, BL, BL. The first BL is 0 in every stroke, the second is 0 to 3 and is non-zero in every stroke with a width: cap mode, then width mode (upstream had them the other way round) |
| Check against AutoCAD | Where AutoCAD also wrote dash elements into the line type that names the definition (a root that is a plain stroke component), they equal the strokes times the unit definition in every case seen |
| Not observed | Definitions and components in drawings before R2007 (the description's place in an inline-text stream is upstream's, unconfirmed); a units type other than 0 and 1 |

## X1 provenance

The fork read the two SPATIAL_FILTER matrices column by column. **LibreDWG consulted: no.** ezdxf
(MIT) reads the twelve values as three rows of four; the public and a further set of drawings
decide between the two (aggregates only; the drawings are not part of this repository).

| Item | Finding |
|---|---|
| Inverse block transform | Read row by row, its product with the current transform of the INSERT that owns the filter is the identity in 390 of 1,178 public filters (32 drawings) and 172 of 877 in the further set (29 drawings); read column by column, in none. The others belong to INSERTs copied or moved after the clip was defined, which keep the original transform |
| Clip bound transform | Read row by row, it takes the filter's origin (code 11) to 0 in every public filter; identity in 126 public and 510 further filters, otherwise a rotation and translation (the UCS of the boundary) |
| Code 71 | 0 in 1,037 public filters; their boundaries cover none of their block's content under any reading of the matrices. The field is the clip's on/off state (ezdxf: clipping enabled) |
| Not observed | Front or back clipping planes; a normal other than +Z |

## A1 provenance

C21 option (b): the annotative context decoders that upstream wrote after LibreDWG's specification were replaced.
**LibreDWG consulted: no.** The flagged file's bodies and the upstream DWG decoder of these classes were not read;
the new decoder was written from the ODA document (reference only: the common AcDbObjectContextData and
AcDbAnnotScaleObjectContextData fields, MLeaderAnnotContext, SCALE), ACadSharp (MIT: the common fields and the block
reference payload), the DXF group codes of the TEXT and LEADER contexts in public DXF drawings, and the bit traces of
every context record of the public drawings (889 DWG files read) and of two further sets (49 and 123 files). Each trace
was decoded under a layout hypothesis and checked twice: the record must end exactly at the end of its main stream,
and the default context (one per object) must equal the owning object's own fields. Only aggregate facts are recorded
here; the drawings are not part of this repository.

| Item | Finding |
|---|---|
| Common fields | `BS` class version (3 or 4), `B` default; handle stream: the scale first. The bit that the ODA document puts between the two ("has file to extension dictionary") is not in any record: read that way 2,328 of the first 4,101 TEXT contexts traced fail, without it all end exactly. Exactly one context per object is default |
| TEXT (19,749 records) | `BS` horizontal mode, `BD` rotation, `2RD` insertion, `2RD` alignment; the DXF codes (70, 50, 10, 11) agree; every record ends exactly and every default equals its TEXT |
| MTEXT (40,880) | `BL` attachment, `3BD` x direction, `3BD` insertion, four `BD` (defined width and height, extents width and height), `BS` column type; type 2 (675 records) adds `BL` count, `BD` width, `BD` gutter, `B` auto height, `B` flow reversed and, without auto height, one `BD` height per column. No record of type 1 was seen; it is read with no heights. Defaults equal their MTEXT except the stored extents in 143 records (AutoCAD refreshes the entity's) |
| BLKREF (618) | `BD` rotation, `3BD` insertion, `BD` X, Y and Z scale (ACadSharp's order); defaults equal their INSERT |
| MTEXTATTRIBUTE (1,614) | `BS` horizontal mode, `BD` rotation, `2RD` insertion, `2RD` alignment, `B` multiline; the 4 multiline records embed an MTEXT context with the prefix of a non-graphical object (`BL` reactors, `B` no extension dictionary, R2013+ `B` data flag), `BS` version, `B` default and the MTEXT payload; their handles (owner, then scale) follow the outer scale. Only R2013+ multiline records were seen |
| LEADER (448) | `BL` count, one `3BD` per vertex, `3BD` horizontal direction, `B`, `3BD` block offset, `3BD` annotation offset; the DXF codes (70, 10, 11, 290, 12, 13) agree, also in R14 and R2000 records |
| MLEADER (1,387) | MLeaderAnnotContext as the MULTILEADER entity stores it (the fork's entity reader, read without its stand-alone variant): all end exactly and every default equals its MULTILEADER's own context |
| Dimensions (704) | `2RD` text location, `B`, `BD` text rotation, 17 single bits, then aligned and radial one `3BD`, diametric two, angular one; handle stream: the scale, then the dimension block of this scale (the entity's own block in every default). The 17 bits vary between records but never change the record's length; their meaning was not needed and is not established |
| HATCHSCALE (31), HATCHVIEW (65) | pattern lines (`BL` count; `BD` angle, `BD` `BD` base, `BD` `BD` offset, `BS` dash count, `BD` per dash), `BD` (1.0 in every record), `3BD` (a unit vector in every record), `BL` loops (`BL` path flags, `B`; when the bit is clear the boundary follows in the hatch path layout without boundary handles); the view class adds `3BD`, `BD`, `B` and the view handle. Defaults equal their HATCH's pattern lines |
| Result | Read with A1, every context record of the three sets is modelled (none left unknown) and every default context equals its object |
| Not observed | Feature control frame, ordinate and large radial dimension contexts (not modelled), R2004-R2010 multiline attribute contexts, MTEXT static columns |
