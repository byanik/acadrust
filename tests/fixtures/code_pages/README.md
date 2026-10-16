# Code Page Reference Data

These Unicode Consortium mapping files provide independent byte expectations
for issue 131. Trailing whitespace has been normalized; mapping data and the
original copyright and use notices remain in each file.

- `CP437.TXT` through `CP869.TXT`: `https://www.unicode.org/Public/MAPPINGS/VENDORS/MICSFT/PC/CP<number>.TXT`
- `JOHAB.TXT`: `https://www.unicode.org/Public/MAPPINGS/OBSOLETE/EASTASIA/KSC/JOHAB.TXT`
- `ROMAN.TXT`: `https://www.unicode.org/Public/MAPPINGS/VENDORS/APPLE/ROMAN.TXT`

Windows CP1361 preserves the US-ASCII backslash at byte 0x5C. The Johab
reference notes that this alternative is allowed; the regression test uses
that mapping so CAD paths and CIF/MIF escapes remain readable. The modern
KS X 1001 euro addition at Johab 0xD9E6 has an additional explicit test.
