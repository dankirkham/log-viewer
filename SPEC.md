# ECU Log Viewer — Specification

## 1. Purpose

Build a local-first Rust application for inspecting and replaying ECU datalogs. Provide automotive-style instrumentation, synchronized plots, and operating-point visualization over recorded ECU tables. The application is read-only: it does not connect to, configure, or flash an ECU.

The initial targets are x86_64 Linux and modern desktop browsers via WebAssembly. Both targets must share the application state, parsing, analysis, and UI implementation. Browser processing must remain entirely client-side; logs must never be uploaded.

## 2. Technology and repository constraints

- Rust, `egui`, `eframe`, and compatible `egui_plot` versions.
- `serde::Deserialize` and `quick-xml` for XML parsing, subject to verification against the supplied fixture.
- `rfd` or an equivalent native/WASM-compatible file picker.
- `wasm32-unknown-unknown` and Trunk for the static web application.
- Nix flakes, `rust-overlay`, and Crane for pinned toolchains and reproducible builds. Commit `flake.lock` and `Cargo.lock`.
- All development dependencies must be provided by Nix. Do not install global Cargo tools or system packages, or require sudo. Nix itself and a functioning graphical/browser environment are prerequisites.
- Prefer one Rust package with clear module boundaries. Use target-specific dependencies only where necessary. Keep core logic independent of egui and platform APIs.
- Prefer straightforward, tested implementations over speculative abstractions. Document any deviations from this specification.

## 3. Input format and discovery

Before implementing the parser, inspect the `.mlg` fixture already present in the repository. Verify the actual file encoding, XML structure, timestamp representation, field metadata, missing-value conventions, and whether AFR/VE tables and target AFR are present. Do not assume that all files with an `.mlg` extension are XML. If the fixture contradicts the expected format, document the discrepancy and resolve it before implementing unsupported assumptions.

Create typed deserialization structures based on the actual XML, separate from the normalized application model. Preserve unknown field names and available units and metadata. Do not hardcode a fixed list of channels. Document supported schema variants and limitations in `docs/log-format.md`, including the mapping of RPM, MAP, AFR, target AFR, VE, and table axes to source fields.

Support one log at a time, typically 1–30 MB. Support native and browser file selection; drag-and-drop is desirable. Display filename, duration, sample count, and load errors. A failed load must not destroy the previously loaded dataset. A successful replacement resets playback and invalidates dataset-dependent caches while retaining compatible layout preferences. No multi-log comparison or additional log formats are required initially.

Malformed or unsupported input must produce actionable errors rather than panics. Reject ambiguous timestamp units rather than guessing. Preserve missing values as missing, not zero. Treat non-finite numeric values as unavailable. Disable external entity resolution and external resource fetching.

## 4. Internal data model

Use a column-oriented dataset with stable field identifiers, field descriptors (name, unit, type, source/derived provenance), timestamps, typed columns, metadata, and optional source tables. Use f64 for time and numeric calculations where appropriate. Represent elapsed time relative to the first sample while preserving original timestamps for inspection. Validate nondecreasing timestamps; reject decreasing timestamps with a clear diagnostic rather than silently sorting or repairing resets. Duplicate timestamps are permitted; the last recorded row at a timestamp wins during seeking. Preserve source row order.

Expose APIs for field discovery, sample lookup by elapsed time, observed statistics, time-range selection, and derived fields. Keep missing values explicit. Cache statistics and derived columns at load or on demand, not every frame. Use stable identifiers rather than display labels for UI references.

## 5. Playback

Maintain one authoritative playback clock, with paused/playing state, elapsed position, speed, and current sample index. All views consume the same sample snapshot per frame.

- Persistent timeline slider spanning the entire log, with current elapsed timestamp and total duration displayed to millisecond precision.
- Play/pause, seek backward 10 seconds, seek forward 10 seconds, and speed selector (0.25x, 0.5x, 1x, 2x, 4x, 8x, 16x).
- Advance using monotonic elapsed time multiplied by playback speed, independent of frame rate. Request repaint while playing; do not busy-loop while paused.
- Select the last recorded sample whose timestamp is at or before the playback position using binary search. **Never interpolate samples**, including numeric channels, derived values, or operating-point positions.
- Clamp seeking to the log bounds. Preserve play/pause state when seeking. Pause automatically at the end; playing from the end restarts at the beginning. Load new logs paused at the first sample.
- Timeline dragging and plot seeking update every visualization immediately. Display the actual selected sample timestamp separately when it differs from the playback clock.
- Keyboard shortcuts: Space toggles playback; Left/Right seek 10 seconds; Home/End seek to bounds. Do not intercept shortcuts while editing text or interacting with controls that consume them.
- Handle empty logs gracefully and single-sample logs without division by zero.

## 6. Dashboard and radial gauges

Use a dark, automotive-instrument-inspired default theme with readable labels and units. Provide a searchable field browser, configurable gauge dashboard, plot area, and persistent transport controls. Place searchable field values in the left panel, gauges and time-series plots in separate tabs in the central panel, and stacked VE and AFR/scatter views in the right panel. Every numeric field, including derived fields, must be eligible for a radial gauge; nonnumeric fields may use text/status indicators.

When no log is loaded, hide the data panels and show a compact, centered Open log action. With a log loaded, show its filename and available summary metadata together in a single-row status area. Keep central dashboard tabs outside the scrollable content area.

Each radial gauge must display field name, current value, units, needle or radial arc, scale, and configurable low/typical/high operating regions. Show observed log minimum and maximum and a typical statistic (median) separately from nominal operating limits. Distinguish these concepts visually and in tooltips.

Gauge configuration includes scale minimum/maximum, nominal lower/upper bounds, optional warning and critical limits, precision, size, and position. Use a consistent color convention for nominal, warning, and critical states, supplemented with text or symbols so color is not the only indicator. Values outside the scale must retain their true numeric readout and display an overflow indication; do not silently clamp the displayed value. Missing data displays N/A, never a stale needle or zero.

Provide editable example presets for common recognized automotive channels only when units and semantics match. For example, a conventional 12 V battery preset can use a nominal band of 12.0–14.8 V and high critical threshold of 16.0 V; 18 V must visibly indicate a high critical state. These are illustrative defaults, not universal ECU safety limits. Do not infer nominal limits from observed log extrema. Unknown fields have neutral styling until limits are configured. Allow users to disable or override presets.

Users can add, remove, resize, reorder, and configure gauges. Provide sensible initial gauges for recognized fields when available, and a reset-layout action. Persist layout and gauge settings across sessions.

## 7. Plotting

Implement overlaid and stacked time-series plots, plus a dedicated RPM/MAP scatter plot colored by AFR.

### 7.1 Time series

Allow arbitrary numeric source and derived fields to be selected and grouped. Overlay mode supports multiple series with stable colors and a legend. Each series can use the left or right Y axis; display recorded-value ticks for each selected axis and retain raw values in tooltips. Stacked mode uses independent Y scales. Link horizontal pan and zoom across every time-series plot. Provide fit-to-log and reset-view controls.

Show a synchronized vertical playback cursor and current sample values. Clicking a plot seeks the shared playback clock; distinguish clicks from drag/pan gestures. Missing values break lines. Connecting recorded points for visualization is allowed, but tooltips and playback must use actual samples, not interpolated values. Preserve extrema when downsampling; never downsample the data used by playback or analysis. Cache plot geometry by dataset, selected fields, visible range, and resolution.

### 7.2 AFR-colored scatter plot

Plot RPM on X, MAP on Y, and measured AFR as point color. Display a labeled color scale with units and configurable limits. Highlight the current operating point. Use actual simultaneous recorded samples only; omit points missing any required channel and report omitted count. Support zoom/pan and reset. Use deterministic sampling or cached rendering for dense logs. If required fields are unavailable, show a clear explanation. Arbitrary XY plots and histograms are deferred.

## 8. AFR and VE table panels

Show the VE table at the top of the right panel and the AFR view below it. The AFR view has tabs for the AFR table and the RPM/MAP scatter plot. The central dashboard has separate Gauges and Plots tabs. Put the searchable tabulated field values in a resizable left side panel. Persist map display preferences and the selected dashboard and AFR/scatter tabs.

### 8.1 Table provenance and coordinates

Use only actual AFR and VE table definitions found in the log. Preserve source breakpoints, values, units, orientation, and metadata. Validate dimensions and axis ordering; support nonuniform breakpoint spacing. Render numeric cells, axis labels, and an optional heatmap legend. If tables are absent, invalid, or ambiguous, show an unavailable state with a diagnostic. Never reconstruct a table by binning, averaging, extrapolating, or inventing values. External tune-file loading and table editing are out of scope.

Use RPM as the expected horizontal axis and MAP as the expected load axis for both maps, subject to the source table metadata. VE is the cell value for a conventional VE table, not automatically a load axis. If the source specifies a different load axis, use the corresponding recorded channel only when its identity and units are verified; otherwise report the map as unsupported. Do not silently substitute MAP, TPS, or VE for one another. If the fixture contains multiple tables, allow selection among verified tables.

### 8.2 Operating-point particle and trail

Overlay a clearly visible particle at the current recorded operating point, using the same sample as the gauges and plots. Map physical axis values to grid coordinates respecting actual breakpoints and cell alignment. This coordinate transformation is not sample interpolation. Do not snap the particle to a cell center. Show an out-of-range indication for points beyond the table domain rather than falsely placing them inside it.

Draw a fading trail from recorded samples in the preceding five seconds of log time, configurable with a default of five seconds. Compute opacity from sample age (linear decay is sufficient). Reconstruct the trail from timestamps on every seek, including backward seeks; it must not depend on frame rate or playback history. Pause freezes the trail. Missing coordinates break the trail; do not fabricate positions. Use bounded/cached rendering if necessary. No interpolation of operating points or table values is required.

## 9. Derived fields

Derived numeric channels are first-class fields: searchable, gaugeable, plottable, and labeled with their formula and dependencies. Compute them from the same recorded row, without interpolation or filling missing values. Use explicit unit and semantic checks. Initially implement:

| Field | Formula | Requirements |
|---|---|---|
| AFR error | measured AFR - target AFR | Compatible measured and recorded target AFR |
| AFR error (%) | 100 * (measured AFR - target AFR) / target AFR | Positive, nonzero target AFR |
| Lambda error | measured lambda - target lambda | Compatible recorded lambda channels, if present |
| Boost pressure | MAP - barometric pressure | Both absolute pressures in compatible units |

Positive AFR/lambda error means lean relative to target; negative means rich. Missing or invalid dependencies yield missing values. Do not assume a stoichiometric AFR, atmospheric pressure, or target AFR. Do not silently derive a target from a table or interpolate table cells. If target channels are absent, explain why the error channel is unavailable. A user-defined expression editor is deferred. Keep the derived-field registry extensible and test formulas and invalid inputs.

## 10. Persistence and platform behavior

Use versioned serde settings and eframe persistence (native application storage and browser local storage). Persist gauge layout, thresholds, plot groups, map display preferences, selected dashboard and AFR/scatter tabs, trail duration, theme, and playback speed. Do not persist log bytes or automatically upload any data. Handle unavailable storage, incompatible settings, and removed fields without panics. Retain unmatched field configurations as inactive or clearly mark them missing. Provide reset-to-default settings.

Native and web builds must expose the same core features. Isolate startup, file picking, and storage differences. Avoid blocking the browser event loop with repeated parsing or expensive per-frame computations; use asynchronous file acquisition and provide a visible loading state. Use browser-compatible timing and avoid unconditional native filesystem/thread APIs in shared code.

## 11. Performance and reliability

Design for 1–30 MB input files on both targets. Avoid cloning datasets per frame, repeated XML parsing, full-series allocations per repaint, or linear scans for every seek. Bound plot and trail rendering work by visible resolution or configurable point budgets. Keep the full original dataset for analysis. Prefer measurable optimizations over premature complexity.

Proposed performance targets (not hard CI timing gates): on a documented reference machine, load a representative 30 MB log within five seconds, sustain responsive scrubbing and approximately 30 FPS or better during normal playback with several gauges and plots. Measure native and browser performance and document results and limitations. Do not claim targets are met without measurement.

## 12. Suggested repository layout

```text
SPEC.md
README.md
Cargo.toml
Cargo.lock
flake.nix
flake.lock
index.html
src/
  main.rs
  lib.rs
  app.rs
  log/{mod.rs,schema.rs,parser.rs,dataset.rs}
  playback/{mod.rs,clock.rs}
  analysis/{mod.rs,derived.rs,statistics.rs}
  ui/{mod.rs,timeline.rs,gauges.rs,plots.rs,maps.rs,fields.rs}
  platform/{mod.rs,native.rs,web.rs}
  settings.rs
tests/
  fixtures/
  parser.rs
  playback.rs
  analysis.rs
docs/log-format.md
nix/
ci/{validation.yml,publication.yml}
```

This layout is guidance, not a requirement to create empty modules. Keep the supplied original fixture intact. If it contains private information or is too large for public source control, use a sanitized representative fixture for committed tests and document how to run local full-fixture validation. Do not deploy logs as web assets.

## 13. Nix build interface

The flake is the source of truth for tools, dependencies, builds, and validation. Pin nixpkgs, rust-overlay, and Crane; use the same pinned Rust version for native and WASM, including rustfmt, Clippy, and the WASM target. Include native graphics/windowing libraries and browser tooling in the appropriate derivations and dev shell. Ensure runtime library discovery works for the packaged Linux application.

| Command | Contract |
|---|---|
| `nix develop` | Complete development shell |
| `nix run` | Launch packaged desktop app |
| `nix build` | Build native executable |
| `nix build .#web` | Produce deployable static web directory |
| `nix run .#web-dev` | Start local web development server |
| `nix flake check` | Execute all mandatory automated validation |

Use Crane dependency artifacts where appropriate; separate native and WASM build artifacts. Pin compatible Trunk/wasm-bindgen tooling. Build without network access inside derivations using declared dependencies. Configure web assets to work under a hosted static-site subpath (relative URLs or an explicitly configured base path). No backend is required.

## 14. Automated validation

Expose mandatory checks under `checks.x86_64-linux`. All must be runnable through `nix flake check` without separate system installations or external services.

- Rust formatting (`cargo fmt --check`) and Nix formatting.
- Clippy with warnings denied for relevant native and WASM targets.
- Unit and integration tests for parsing, playback, field lookup, derived fields, gauge status, settings, map coordinates and trails.
- Native release build and WASM/static web build.
- Headless browser smoke test using Nix-provided browser/test tooling: load app, import a test fixture, seek, verify current sample, verify dashboard map panels and their unavailable states, and check for fatal browser errors. Use a local test server and no external network.
- Dependency audit against a pinned advisory database, with documented update process and any explicit, justified exceptions. No live advisory fetch during sandboxed checks.

Tests must cover malformed XML, missing fields, missing/nonfinite values, timestamp boundaries and duplicates, decreasing timestamps, empty/single-sample input, playback speeds and end behavior, deterministic backward seeks, derived error signs and zero denominators, 18 V critical gauge state under the example preset, nonuniform table breakpoints, absent tables, trail expiry and reconstruction, and persistence round trips. Test logic independently of rendering wherever possible. Use fixture-backed assertions for actual schema and metadata rather than invented assumptions.

## 15. Continuous integration and deployment

Automated validation runs on pushes and pull requests using an x86_64 Linux runner with Nix installed and flakes enabled. The validation step invokes `nix flake check`; do not duplicate individual checks in workflow YAML. Cache Nix artifacts where practical.

Publish the static site from the default branch only after successful checks. Build using `nix build .#web`, upload only the resulting static application artifact, and use the hosting provider’s artifact/deployment workflow with minimal required permissions. Configure concurrency to prevent stale deployments. Pull requests must not deploy. Document one-time repository hosting configuration. Never publish source log fixtures, private data, or credentials as deployment assets.

## 16. Implementation order

1. Inspect the fixture and document schema/field/table findings; establish Nix toolchain, native and web skeletons, and basic CI.
2. Implement normalized dataset and fixture-backed parser tests; establish file loading on both platforms.
3. Implement deterministic playback and timeline; test sample selection and transport behavior.
4. Implement radial gauges, thresholds, field browser, and layout editing.
5. Implement derived fields, overlaid and stacked plots, and AFR-colored RPM/MAP scatter plot.
6. Implement source-backed AFR/VE dashboard panels and deterministic trails; explicitly handle unavailable source tables.
7. Implement versioned persistence, performance improvements, browser smoke tests, and Pages deployment.

Run `nix flake check` at each completed milestone. Keep the application runnable. Do not mark unimplemented features as complete. If the fixture lacks a required input, implement the honest unavailable state and report the limitation rather than fabricating data.

## 17. Definition of done

- A user can open the supplied supported log on native Linux and in a browser and inspect its actual recorded fields.
- All visualizations share deterministic playback with no sample interpolation.
- Radial gauges are customizable and clearly distinguish nominal limits from observed statistics; the configured battery preset visibly flags 18 V.
- Overlay and stacked plots and AFR-colored RPM/MAP scatter work with available channels.
- AFR/VE dashboard panels display verified source tables and a five-second fading operating-point trail, or clearly explain why source data is unavailable.
- Derived errors are correct and missing dependencies are handled explicitly.
- Layout and visualization preferences survive reloads where storage is available.
- Native and web packages build through Nix, all flake checks pass, and workflows are ready for CI and Pages deployment.
- README documents commands, supported formats, limitations, controls, deployment setup, and performance findings.

## 18. Non-goals and decisions to verify

No ECU communications, tune editing, synthetic maps, sample interpolation, multi-log comparison, backend, telemetry, user accounts, additional native OS targets, arbitrary expression editor, or external tune-file import in the initial release.

Verify against the fixture: exact XML schema, timestamp units, field aliases, actual AFR/VE table availability and axis semantics, and target AFR availability. The default RPM/MAP axis interpretation, editable voltage preset, derived-field formulas, performance targets, keyboard shortcuts, and strict handling of decreasing timestamps are proposed implementation defaults; document any fixture-driven changes. Hosted static-site deployment requires repository access and configuration, which the specification alone cannot establish.
