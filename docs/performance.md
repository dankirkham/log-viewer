# Performance measurements

## Supplied capture on Linux and a headless browser

Measured locally on an Intel Core i5-8250U CPU at 1.60 GHz, Linux 6.18.35, using the pinned Rust 1.88.0 release build and Nix-provided browser 158.0.8127.36. The supplied capture was 3,860,488 bytes; parsing produced 23,084 rows and 78 normalized fields (74 recorded source fields plus the available derived fields).

| Measurement | Result | Method |
| --- | ---: | --- |
| Native file read | 6.817 ms | One read into memory; the file may have been in the OS page cache |
| Native parse median | 70.958 ms | Nine release parser runs over those in-memory bytes; range 65.992–75.401 ms |
| Browser loopback read + parse | 106.300 ms | One full-capture run; app `Performance.now()` covered local HTTP fetch and parsing in the release WebAssembly smoke build |

The native parser measurement includes dataset construction and derived-field preparation, but not drawing the dashboard. The browser measurement includes loopback file acquisition and parsing, but not an estimate of sustained rendering frame rate. The separate browser smoke wall time is dominated by browser startup and automation and is not reported as app load time.

This fixture is about 3.86 MB, so these results do not establish the proposed five-second target for a 30 MB log. No 30 MB fixture was supplied. Frame rate during playback, highly fragmented plot rendering, cold-storage reads, and slower devices have not been measured. The implementation keeps all recorded rows for playback and analysis and bounds plot geometry, but that is not a measured FPS guarantee.

## Reproducing measurements

Native measurement (nine parse iterations, one file read):

```sh
nix develop --command cargo run --release --example measure -- /path/to/private-log.mlg
```

For a repeatable browser measurement, build the test-only WebAssembly instrumentation, copy the chosen log into the local-only test directory, and serve it on loopback. The test-only hook fetches that file locally and sends its bytes through the app's parser/load path. Do not commit the temporary copy or deploy the smoke build. The standard browser smoke derivation uses the same path with only the sanitized fixture. The actual browser picker was also exercised locally with the supplied capture; it is not invoked by the sandboxed smoke test because the browser file chooser crashes in the Nix build sandbox.

```sh
nix develop --command bash
unset NO_COLOR
export CARGO_ENCODED_RUSTFLAGS='--cfg=browser_smoke'
trunk build index-smoke.html --skip-version-check --release --public-url ./ --dist dist-smoke
cp /path/to/private-log.mlg dist-smoke/private-log.mlg
python3 -m http.server 8765 --bind 127.0.0.1 --directory dist-smoke &
python3 tests/browser_smoke.py \
  --url http://127.0.0.1:8765/ \
  --browser "$BROWSER_BIN" \
  --driver "$DRIVER_BIN" \
  --fixture-url /private-log.mlg \
  --fixture-name private-log.mlg \
  --expect-samples 23084 \
  --expect-first-rpm 0 \
  --expect-last-rpm 0
```

Use the actual row count and first/last RPM values printed by the native measurement program for a different input. Browser smoke instrumentation, its hidden status element, and the import hook are built only with `browser_smoke`; the deployable `.#web` application does not contain them. A headless local-fetch measurement and the regular picker can have different acquisition costs; compare the app-reported local-fetch-plus-parse metric with the file-picker run on the same machine before drawing conclusions.
