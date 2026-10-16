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
