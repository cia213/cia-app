# Quality and performance patch report

**Final report: October 1, 2026.** The patch fixes the priority risks identified
on revision `241f350` and replaces the RIFE path's intermediate video with a
direct RGB stream to the final encoder. The initial audit remains available in
[AUDIT-QUALITE-PERFORMANCES.md](AUDIT-QUALITE-PERFORMANCES.md); the
[project map](CARTOGRAPHIE.md) describes the patched code.

## Fixes and evidence

| Audit | Delivered change | Validation |
| --- | --- | --- |
| Q1 — history and concurrent processing | Immutable job snapshot, independent navigation, global backend reservation before preparation | Production-handler and Rust registry tests |
| Q2 — overwritten intermediate file | Intermediate MP4 and glob removed; owned temporary files and no-clobber publication | No-clobber and late-publication tests; fixtures with brackets in names |
| Q3 — orphaned processes | Windows Job Object with kill-on-close, suspended spawn followed by assignment, cleanup guards, cancellation during preparation | Real suspended-descendant termination test and reservation/cancellation tests |
| Q4 — Smoothie aspect ratio | Dimensions preserved, SAR passed through; encoding overrides without independent width/height rounding | Real Smoothie fixture with SAR 4:3 |
| Q5 — ineffective settings | Selected recipe/model passed through, configured LUT, non-neutral colour processing activated; unapplied scene/blend controls removed | Rust contracts, model actually used, Smoothie colour/LUT fixture |
| Q6 — cadence and validation | Rational FPS, average VFR cadence, parameter limits, rotation, and SAR; explicit SDR contract | Fractional FPS, factor 3, VFR, rotation 90, and SAR 4:3 fixtures |
| Q7 — false success/truncation | Decode errors propagated, bounded pipes, worker and encoder awaited, frame counts and metadata validated | Full decode of integration results; simulated encoder failure; audio-only Smoothie output rejected |
| Q8 — stale responses/previews | Request identifiers, native cancellation, hover loading, bounded cache, eight extractions instead of sixteen | Stale-analysis-response tests and real extraction of eight PNGs |
| Q9 — configuration | Serialized atomic writes with unique temporary files; errors propagated without false success or replacement of unreadable data | UI-save tests; source verification of atomic write paths |
| Q10 — paths/subtitles/colour | Explicit paths without glob, MP4 text subtitles and retiming, HDR/bitmap rejection, SDR BT709 profile | Copied AAC audio verified by packet hashes, subtitles and languages retained, HDR/no-clobber rejection |
| Q11 — final preview | Extraction from the actual output file | Result-handler and actual-output-path test |
| Maintenance | One Python development wrapper, bounded logs, sliders without permanent RAF, CI verifiable without release payload | Tests, lint, and builds |

Patch review also fixed two cases absent from the original audit: selecting an
embedded video cover instead of the actual track, and permissive loading of
RIFE weights. The worker uses the analyzed stream index and loads a complete
checkpoint with `strict=True`, accepting ordinary or `module.`-prefixed keys.
An incomplete/incompatible model fails before decoding; results computed with
partially loaded weights are never presented as valid.

## Performance

The previous per-component estimates for FP16, preview process count, and the
encoder **are not additive gains**. The patch's first exploratory series
coincided with a Rust build; it is retained as evidence but excluded from the
gain report.

Final measurements are recorded in [PATCH-PERFORMANCE.json](PATCH-PERFORMANCE.json),
with the initial revision frozen in [BASELINE-PATCH.json](BASELINE-PATCH.json).
The protocol compares complete process durations, including Python startup,
model loading, decoding, interpolation, audio, and encoding. Before/after order
is alternated; every result is checked with FFprobe and a full decode. Code,
source, and model hashes are retained.

Local measurements on **RTX 3050 Laptop, 4 GiB VRAM**, 1680 × 1050 / 120 FPS source
with audio, boost ×2. Encoding comparison: x264 CRF18/**medium**; optional NVENC
CQ18/p5.

| Profile | 48 frames: median of 3 trials | Reduction vs old | 120 frames: 1 trial | Reduction vs old |
| --- | ---: | ---: | ---: | ---: |
| Before: FP32/x264 | 27.07 s | 0.0% | 50.52 s | 0.0% |
| Patch: FP32/x264 | 21.14 s | 21.9% | 47.65 s | 5.7% |
| Patch: FP16/x264 | 17.74 s | 34.5% | 34.59 s | 31.5% |
| Patch: FP16/NVENC | 17.40 s | 35.7% | 32.41 s | 35.8% |

All 16 timed outputs pass cadence, duration, audio, and full-decoding checks.
The 120-frame series is a single additional check and provides no statistical
spread. FP32 gains vary here from **5.7 to 21.9%**; FP16/NVENC stays near **36%**.
These values describe only these local tests.

| Output comparison | SSIM, 48 frames | PSNR, 48 frames | SSIM, 120 frames | PSNR, 120 frames |
| --- | ---: | ---: | ---: | ---: |
| Patch FP32 vs old FP32 | 0.969939 | 38.13 dB | 0.970412 | 38.26 dB |
| Patch FP16/x264 vs patch FP32/x264 | 0.973620 | 38.82 dB | 0.973574 | 38.88 dB |
| Patch FP16/NVENC vs patch FP32/x264 | 0.975968 | 39.28 dB | 0.974627 | 39.17 dB |

Image comparisons use SSIM/PSNR on shared frame indices and are not a subjective
quality assessment. The old pipeline added intermediate MPEG-4 compression;
the patch directly encodes interpolated frames. Holding the last frame
completes N×factor frames, unlike the old shorter tail. FP16 can change computed
values; **NVENC CQ18 is not equivalent to x264 CRF18**.

Short clips include a substantial proportion of model-loading time. They do not
establish sustained throughput over several minutes, a universal gain, or a 4K
gain. Eight previews halve the number of processes without claiming a 51% time
gain: local luminance was absent from that initial microbenchmark.

## Validation and reproduction

**Final results:** 19 frontend tests, 11 Python tests, 14 Rust tests, and 4 extra
Python checks passed; 10 real RIFE or pre-launch rejection cases passed. Real
Smoothie testing passed in four variants, with pixel changes verified.
`cargo fmt --check` and warning-free Clippy pass. Vite and native release Tauri
builds without a bundle pass; JS is 133.34 kB / gzip 41.48 kB. Rust checks without
the payload use the same resource override as CI.

Source-only checks:

```powershell
npm test
npm run build
python -m unittest discover -s tests -v
$env:TAURI_CONFIG = '{"bundle":{"resources":["resources/time_remap.py","resources/rife_worker.py","resources/bootstrap/bootstrap-rife.ps1"]}}'
cargo fmt --manifest-path src-tauri/Cargo.toml -- --check
cargo clippy --locked --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings
cargo test --locked --manifest-path src-tauri/Cargo.toml
Remove-Item Env:TAURI_CONFIG
```

Additional real tests require the local runtime and CUDA virtual environment:

```powershell
..\time-remap-app\venv\Scripts\python.exe scripts\test_pipeline_integration.py --run
cargo test --manifest-path src-tauri/Cargo.toml bundled_smoothie_pipeline -- --ignored --nocapture
..\time-remap-app\venv\Scripts\python.exe scripts\benchmark-patch.py --run --frames 48 --repeats 3 --profiles --preset medium --label final-48-clean
..\time-remap-app\venv\Scripts\python.exe scripts\benchmark-patch.py --run --frames 120 --repeats 1 --profiles --preset medium --label final-120-clean
```

Measurement scripts retain artifacts in a unique `time-remap-app/audit-benchmark`
directory. The old engine receives only benchmark-owned copies of media and
runtime files so its overwrite risk is not reintroduced.

## Limits and future work

- The current delivery profile is SDR 8-bit H.264. HDR requires a real transfer/tone-mapping conversion; the current backend rejects odd dimensions.
- The local Smoothie runtime requires consistent BT709 metadata for LUT conversion; an untagged or incompatible source must be rejected before that filter. This behaviour was discovered in a real test.
- Text subtitles are converted to `mov_text`: advanced ASS styles are not retained. RIFE rejects bitmap subtitles. The Smoothie path does not promise to retain subtitle tracks.
- Production validates metadata and process completion; it does not systematically perform a full decode of each result. Fixtures and benchmarks do perform that decode to verify generated videos.
- UI handlers are tested with a mocked Tauri boundary; browser visual inspection covers ABOUT and access to advanced settings. No complete automated native-interface test on a fresh Windows installation is claimed.
- This patch itself publishes no signed installer or release. Native compilation does not replace validation of the distribution payload and its installer.
- Extracting the Svelte/Rust monoliths, a persistent RIFE worker, TensorRT, 4K optimization, and a long RAM/VRAM measurement remain separate tasks. They are not claimed as delivered or included in the gains.

Training helpers `teacher.*` and `caltime.*`, absent from the inference network,
are excluded; all network parameters are loaded strictly, with PyTorch checking
their shapes.
