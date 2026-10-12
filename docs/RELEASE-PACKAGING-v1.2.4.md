# Windows package v1.2.4

The published runtime is a clean copy of Smoothie `Nightly_2025.11.30_14-36`,
commit `3da73de9df0c991801c4f8474b320445fb25a208`. The portable archive's SHA-256
is `a9e49e8638c0c1db90a86d4ddb4e5eae0681e773c5bd9177caab8eb5586c4a07`.
Binaries and VapourSynth scripts shared with the tested local installation are
identical byte for byte.

`prepare-release.py` preserves `#{type: ...}` metadata and the `:` separator in
Smoothie recipes. Rebuilding with a standard INI serializer removed that
metadata and caused the two crashes observed during preparation. Those test
packages were not published.

The shipped defaults enable no LUT, colour adjustment, or speed change. Default
output preserves aspect ratio and uses x264/CRF18/medium, yuv420p, and AAC.
Caches, media, models, personal paths, and `last_args.txt` are excluded; notices
and Python's `distutils-precedence.pth` are preserved.

The package includes FFmpeg/FFprobe `8.1.1-essentials_build-www.gyan.dev`,
VapourSynth R70, and the official Python 3.11.9 installer for optional bootstrap.
`RELEASE-PAYLOAD.json` records each runtime file's size and SHA-256, Smoothie
provenance, FFmpeg configuration, and Python installer hash. Upstream Smoothie,
VapourSynth, and FFmpeg source archives are attached to the release.

## Reproducing preparation

Place the official portable archive at
`src-tauri/target/release-kit-v1.2.4/smoothie-rs-nightly.zip`, and the FFmpeg tools
and Python installer at the locations described in `RUNTIME-RELEASE-NOTES.md`
for that historical version. The script rejects an unexpected archive or Python
installer. Use the v1.2.4 source tag to reproduce its original packaging; the
current main branch uses the newer private Python bootstrap.

```powershell
python scripts/prepare-release.py
$env:CIA_TEST_RUNTIME_ROOT=(Resolve-Path src-tauri/target/release-kit-v1.2.4/runtime).Path
cargo test --locked --manifest-path src-tauri/Cargo.toml bundled_smoothie_pipeline -- --ignored --nocapture
# Remove engine-generated caches and sidecars before building.
python scripts/prepare-release.py
$env:TAURI_SIGNING_PRIVATE_KEY = '<existing key path>'
$env:TAURI_SIGNING_PRIVATE_KEY_PASSWORD = '<password>'
node node_modules/@tauri-apps/cli/tauri.js build --config src-tauri/target/release-kit-v1.2.4/build.json
```

Release configuration maps the clean runtime to `resources/runtime`, alongside
RIFE scripts, Python bootstrap, notices, and provenance manifest. The generic
development `resources/runtime` directory is not used as the public package's
source.

## Validation

Local checks comprise 19 frontend tests, 11 Python tests, and 14 Rust tests, with
fmt/Clippy passing. The real Smoothie test covers four variants: neutral, colour
adjustment, adjustment + LUT, and an untagged source. It verifies 320×200, SAR
4:3, fractional input cadence, 30 frames at 30 FPS, audio, BT709 tags, changed
pixels for different settings, eight previews, no-clobber output, and cleanup.

Pipeline validation uses a clean isolated runtime; it does not simulate NSIS
installation and complete GUI navigation on a new Windows account. RIFE tests
and patch performance measurements are detailed in `PATCH-QUALITE-PERFORMANCES.md`.
