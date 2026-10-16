# Native Validation

Run `pwsh -File tests/bricscad/run_issue_regressions.ps1` from the repository root
on a Windows machine with BricsCAD. The example generates synthetic DWG, ASCII
DXF, binary DXF, and DWG-to-DXF conversion inputs under `target/`.

The harness starts a separate hidden BricsCAD process for each input, opens the
file through a command script, runs AUDIT without repairs, checks the relevant
fields, and writes a report plus `native-results.json`. It stops only the process
ID it created on timeout. Existing BricsCAD sessions are never addressed.

Use `-Filter modern2010.dwg -SkipGenerate` to run one existing input or
`-WriteNativeFixtures` to save BricsCAD's interpretation alongside the source.
Native fixture export does not repair failed checks. Native BricsCAD validation
can reveal interoperability problems beyond the library's internal round trips;
a FAIL report remains a failure even when AUDIT finds no errors.

ELEVATION is checked through the active viewport as well as the Rust header
regression for #127. Wire-cache color for #129 is not exposed by AutoLISP;
the harness checks solid count and volume while the Rust tests verify wire
colors and binary record alignment.

BricsCAD V20 retains TABLESTYLE title/header suppression when importing DXF but
drops both values even when saving its own suppression-enabled style to a 2010
DWG and reopening it. The native DWG suppression checks remain strict and fail
on this behavior. The Rust tests separately check the values in the public
model and DWG/DXF conversion. `create_style_fixture.lsp` is a minimal native
reproduction, accepting an output basename through `ar:create-style-fixture`.
