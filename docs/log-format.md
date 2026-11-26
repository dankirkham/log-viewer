# Log format discovery

The confirmed fixture is `2026-11-23_12.24.44.mlg` (3,860,488 bytes). It contains tune and capture metadata that may be private and is excluded from Git. Inspection below combines direct examination of the actual file with format reference material.

## Container and timestamps

The file begins with the six-byte signature `4D 4C 56 4C 47 00` and revision value 2, not an XML declaration. It contains binary field descriptors and sample data as well as an embedded XML tune. Parsing the entire `.mlg` as XML would be incorrect. Other container versions have not been examined.

The container header uses big-endian numbers. This file declares 74 descriptors, a 158-byte sample payload, info at 0x19f4, and data at 0x17d98. Each 89-byte descriptor begins at offset 24 and contains a type, name, units, display style and category; scalar descriptors additionally contain big-endian IEEE 754 scale and transform and display precision. The documented scalar conversion is `(raw + transform) * scale`. One descriptor is a bitfield with an empty name; its bit metadata has a different layout and must not be interpreted as scalar scale/transform. Types in the file comprise unsigned and signed 8- and 16-bit integers, 32-bit floats, and one unsigned 8-bit bitfield. Preserve unknown metadata and fields.

A sequential inspection using the documented block sizes finds 23,084 sample blocks and two marker blocks, ending exactly at EOF. All sample payload checksums match the documented byte sum. The first recorded field, `Time`, is an IEEE 754 binary32 value in seconds with scale 1 and transform 0. Its decoded values run from 0 to approximately 1546.283 seconds, are finite and nondecreasing, and include seven adjacent duplicates. `SecL` is an unsigned byte with unit `sec`; it is not a replacement for the `Time` field. The separate two-byte block timestamp is documented in units of 10 microseconds per bit but its rollover/relationship to the recorded `Time` channel is not established here. No missing-value sentinel has been established; do not assume zero means missing.

## Embedded tune XML

An XML document starts at byte 0x1abd and ends at 0x17d96 (exclusive), followed by a newline. Its declaration specifies ISO-8859-1. It contains `bibliography`, `versionInfo`, `page`, `settings`, and other elements. Pages contain `constant` elements with attributes such as `name`, `rows`, `cols`, `units`, and `digits`; their text holds whitespace-separated values or quoted settings. This is a snapshot of a tune embedded in a binary log, not an XML schema for the recorded samples. The container immediately before it includes a `NEW_INFO_PROVIDER,LogStart_MAIN_TUNE` marker and a stated length of 90842 bytes, matching the XML plus trailing newline. External entities and resources must not be resolved.

The following arrays are present in that snapshot:

| Candidate | Values | Candidate RPM bins | Candidate load bins |
| --- | --- | --- | --- |
| VE | `veTable`, 16 × 16, `%` | `rpmBins`, 16, `RPM` | `fuelLoadBins`, 16, `kPa` |
| second VE | `veTable2`, 16 × 16, `%` | `fuelRPM2Bins`, 16, `RPM` | `fuelLoad2Bins`, 16, `kPa` |
| AFR | `afrTable`, 16 × 16, `AFR` | `rpmBinsAFR`, 16, `RPM` | `loadBinsAFR`, 16, `kPa` |
| lambda | `lambdaTable`, 16 × 16, `Lambda` | association not verified | association not verified |

Each listed matrix has 256 numeric text values; each listed axis has 16. Names and dimensions suggest these associations, but the XML examined does not itself establish table-to-axis linkage, orientation, or which recorded load channel corresponds to each axis. Those must be verified before rendering operating points. The axes are nonuniform. Both VE RPM candidates end with duplicate 6600 breakpoints; the AFR RPM candidate ends with three 6500 breakpoints. At the user's direction, source maps skip later columns whose RPM breakpoint repeats the previous breakpoint, retaining the first matching column and its original cells. This drops one column from each VE candidate and two from AFR; it does not merge, average, or interpolate table values. Decreasing axes and invalid dimensions remain errors. All three listed load arrays increase from 16 to 100 kPa.

## Candidate recorded channels

The descriptors include `RPM` (`rpm`), `MAP` (`kpa`), `AFR` (`O2`), `AFR Target` (`O2`), `Lambda` and `Lambda Target` (no units), `VE _Current`, `VE1`, and `VE2` (`%`), `Baro Pressure` (`kpa`), and `FuelLoad` (`kPa`). The names alone do not establish that all values are usable or that pressure channels are absolute; firmware-specific reference material provides additional verification for this fixture. `AFR` and `AFR Target`, and the lambda pair, are available for same-row error calculations under the verified release mapping below. `MAP` and `Baro Pressure` are likewise used for boost only under that verified mapping. Do not infer a target from the tune, assume a stoichiometric ratio, or substitute `MAP` for `FuelLoad` without verification.

No other schema variants or missing-value conventions have yet been verified. Fixture-backed parser tests are prerequisites to claiming support. No sample or table interpolation is permitted.

## Current parser support

The parser supports version 2 scalar and bitfield descriptors, logger data blocks, and marker blocks. It validates the payload checksum, lengths and offsets, requires exactly one numeric `Time` field with unit `s`, rejects decreasing and non-finite timestamps, and preserves duplicate timestamps in source order. Non-finite channel values become unavailable; no numeric missing-value sentinel is assumed. A signed 64-bit value beyond the exact integer range of the current f64 column representation is rejected rather than silently rounded. Unknown versions, block types and field types produce an error. The parser extracts tune constants without treating candidate table associations as verified maps; malformed or unsafe embedded tune XML is reported as a tune diagnostic while recorded samples remain available. It does not resolve DTDs or external entities.

The repository's sanitized test fixture retains the original descriptor block and bit names, but replaces every sample value with synthetic values and omits capture/tune metadata. Set `LOG_VIEWER_FIXTURE` as documented in the README to run the additional full-fixture assertions locally. UTF-8/ASCII field descriptor text is supported; other descriptor encodings and non-v2 files have not been verified. Browser parsing currently executes once on the main thread, so large logs can temporarily stall the interface.

## Verified firmware channel semantics

The fixture metadata identifies one firmware release. Its matching reference definition maps the recorded `AFR` and `AFR Target` labels to measured and target channels, defines the recorded lambda pair, and defines gauge-pressure conversion from `map - baro`. Accordingly, for that exact identified firmware and matching units only, the application computes AFR and lambda errors and `MAP - Baro Pressure` from the same recorded row. It does not use the tune's stoichiometric AFR to synthesize missing channels. Other firmware releases remain unverified and reports these derivations unavailable.

The matching reference definition specifies `veTable` with `rpmBins` and `fuelLoadBins`, `veTable2` with `fuelRPM2Bins` and `fuelLoad2Bins`, and `afrTable` with `rpmBinsAFR` and `loadBinsAFR`. It identifies the first and AFR table load channel as `fuelLoad`, and the second as `fuelLoad2`; it does not justify silently substituting MAP for those channels. For repeated RPM breakpoints, the viewer follows the explicit user-requested first-column policy described above; later duplicate columns are omitted from the displayed grid and the UI reports the count.

For the verified firmware table mappings, matrix cells are read in the XML's row order, with RPM as columns and fuel load as rows, consistent with the reference definition’s array and table-editor documentation. The application validates original dimensions and units, then removes later columns at repeated RPM breakpoints before requiring strictly increasing processed axes. This is a documented deviation from preserving every source table column, requested for this project; source tune constants remain unchanged. It does not infer or synthesize values. Decreasing axes still fail. The secondary VE map additionally needs a verified recorded `fuelLoad2` channel. The reference defines it as an expression but does not establish a default datalog label; it remains unsupported instead of substituting MAP or primary fuel load.
