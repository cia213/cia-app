# cia render v1.3.0 validation

Validation date: 2026-10-14. This records tests of the v1.3.0 candidate, rather than
the earlier v1.2.7 baseline in [AUDIT-v1.3.md](AUDIT-v1.3.md).
Generated media, downloaded tools, runtime environments and raw logs remain in
ignored `src-tauri/target/` directories. The source recordings are not test inputs.

## Candidate environment

| Item | Local environment |
| --- | --- |
| OS | Windows 11 Home x64, build 26200 |
| GPU | NVIDIA GeForce RTX 3050 Laptop GPU, compute capability 8.6, 4 GiB VRAM |
| Driver | NVIDIA 610.62 |
| Managed profile | `rife4.26-torch2.10-cu128-py311-win64-v1` |
| RIFE/Python | Practical-RIFE 4.26, isolated Python 3.11.9 x64 |
| Tensor runtime | Torch 2.10.0+cu128, torchvision 0.25.0+cu128 |
| Render tools | Smoothie `Nightly_2025.11.30_14-36`; FFmpeg/FFprobe 8.1.1 essentials |
| Build tools | Node.js 24.15.0, npm 11.14.1, Rust 1.96.0, Windows PowerShell 5.1 |

## Checks completed before native packaging

| Check | Result | Boundary |
| --- | --- | --- |
| Frontend handler suite | 32 passed | Actual handlers and derived expressions; mocked Tauri boundary |
| Compiled UI browser suite | 6 passed | Installed Edge, compiled Svelte UI; mocked Tauri IPC |
| Production frontend build | Passed | Vite/Svelte build |
| npm dependency audit | No advisories reported | Candidate lockfile at validation time; not a general security guarantee |
| Python pipeline unit tests | 18 passed | Media, ownership, frame-count and metadata contracts |
| Integration-helper unit tests | 4 passed | Timing/signal assertions and fixture helpers |
| Runtime-health unit tests | 7 passed | Trusted profile, code/model integrity, package and status checks |
| Download fault tests | 5 passed | Interrupted/idle downloads, resume and bad artifact handling |
| Private Python bootstrap | Passed | Fresh private install, reuse/repair, bad hash rejection, spaces/accents, unchanged registered Python and user PATH |
| Generation bootstrap contract | Passed | Cross-process lock, unchanged active pointer on competing install, verified cache reuse, contained paths, stalled probe killed |
| Actual fresh managed generation | Passed | Local user profile; full CUDA/model verification |
| Corrupted model and repair | Passed | Present-but-corrupt model rejected; replacement verified before atomic promotion |
| Interrupted candidate recovery | Passed | Next repair discards its owned unfinished generation |
| Healthy generation reuse | Passed | Active pointer unchanged; full check took 23.92 seconds locally |
| Full runtime integrity and inference | Passed | 18,162 installed files verified; actual CUDA/model check took 23.78 seconds locally |

The runtime measurements are local check durations, not startup or render-speed
benchmarks. Native health status uses a bounded cache, and the worker checks its
trusted project/model and package contract before loading the model.

## Actual media integration

The candidate summary is generated at
`src-tauri/target/pipeline-integration/v1.3-candidate-summary.json`, with source
SHA-256 provenance. Tests use the new managed profile and actual RIFE/CUDA/FFmpeg;
they check decoded frames, cadence, dimensions, duration, audio and preserved
source sentinels.

**FP32 / software H.264:** 18 media cases and two rejection cases passed. Coverage
includes fractional FPS; 360 FPS input; VFR normalization; anamorphic and rotated
video; cover-art stream selection; subtitles with and without audio; two audio
tracks; language/title and disposition metadata; offset video; delayed audio;
MKV video shorter than its audio; Unicode/emoji names; and Rubber Band slow motion
including a factor of 10. Signal checks distinguish the two source tones and
check pitch/delay rather than relying only on successful exit codes.

**FP16 / NVIDIA H.264:** six media cases and two rejection cases passed at
320 × 192: 360 FPS input, multiple audio in boost and slow-motion modes, delayed
audio, MKV track titles, and Unicode/emoji names. This validates this GPU and test
geometry. It does not establish acceptable FP16 quality, sufficient VRAM at every
resolution, or encoder availability on every NVIDIA device.

## Native and publication checks

The following candidate gates are recorded separately from the browser mocks and
CUDA script checks. Their final results must be filled from the actual v1.3.0
installer and native application before publication.

| Check | Candidate status |
| --- | --- |
| Locked Rust formatting, Clippy and tests | Passed; 40 tests including the four opt-in media tests, 36 source-only tests |
| Native Smoothie/Rubber Band multi-audio and tail recovery | Passed; seven render variants, five multi-audio/timing variants and bounded recovery checks |
| Prepared and extracted NSIS payload | Passed; 1,365 exact runtime files, seven source resources plus notices, executable match |
| Native WebView2 navigation, CSP, real UI render/interpolation | Passed on extracted and installed final app; same executable SHA-256 |
| Same-host NSIS installation and upgrade | Passed; current-user 1.2.7 to 1.3.0, exit code 0, native configuration preserved |
| Signed updater manifest and installer digest | Cryptographic installer/comment verification passed; unchanged public key |
| Source commit, remote SHA and Windows CI | Recorded in the release's `RELEASE-VALIDATION.json`; checked before publication |

`scripts/tests/test_native_app.py` uses tauri-driver and matching Microsoft Edge
WebDriver to exercise the real packaged app and Tauri IPC. Its source-selection
step emits a real Tauri drag-drop event; it does not automate the Windows file
picker or a physical drag gesture. A separate NSIS check is needed to validate
installation. No native result may be inferred from `npm run test:browser`.

The native runs used WebView2 and matching Microsoft Edge WebDriver
155.0.4283.45. The existing Windows profile was used with an isolated WebView
folder and temporary generated-media settings; the original native configuration
was restored byte-for-byte. Render finished with two audio tracks at speed 1.07,
CUDA interpolation finished, a second instance exited while the original stayed
usable, all eight About credits remained present, and no CSP violation was
recorded during navigation or processing. Screenshots were inspected locally.
The installed executable SHA-256 is
`717fa67ff19faff615da7e445dcd3db9bacec0d56ca7355dc1bd731b2aa6d7f0`.

Use `--attach` for native qualification. The host's local automation endpoint
requires both `TAURI_WEBVIEW_AUTOMATION=true` and a valid numeric
`CIA_RENDER_AUTOMATION_PORT`; the harness sets them only for its child processes.
Ordinary launches do not enable it. The production frontend, CSP and real IPC
remain in use during this test.

Python discovery passed 38 tests, including eight release-payload corruption and
provenance checks. Render finishing preserves default/forced audio flags;
additional accessibility/comment dispositions are preserved by interpolation
but are outside the native Render finishing contract.

Dependency review is scoped to the shipped platform. Cargo audit still matches
two Quick XML 0.39.4 advisories through the Linux Wayland scanner build dependency;
that dependency is absent from the Windows compilation graph. Six unmaintained
dependency notices require continued upstream monitoring. Patched Rustls, Anyhow
and Event Listener versions are included. This is not a claim that all dependency
advisories are absent.

## Reproduce checks

On Windows with Node 24, Python 3.11+, Rust stable and Microsoft Edge:

```powershell
npm ci
npm test
npm run build
npm run test:browser
python -m unittest discover -s tests -v
powershell -NoProfile -ExecutionPolicy Bypass -File tests/test_bootstrap.ps1
powershell -NoProfile -ExecutionPolicy Bypass -File tests/test_bootstrap_generation.ps1
$env:TAURI_CONFIG = '{"bundle":{"resources":["resources/time_remap.py","resources/rife_worker.py","resources/runtime_health.py","resources/bootstrap/bootstrap-rife.ps1","resources/bootstrap/bootstrap-python.ps1","resources/bootstrap/rife-profile.json","resources/bootstrap/rife-requirements.lock"]}}'
cargo fmt --manifest-path src-tauri/Cargo.toml -- --check
cargo clippy --locked --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings
cargo test --locked --manifest-path src-tauri/Cargo.toml
Remove-Item Env:TAURI_CONFIG
```

Source-only Cargo checks need tracked resources rather than an unstaged render
payload. Use the same `TAURI_CONFIG` resource override as
[Windows CI](../.github/workflows/ci.yml), or prepare the payload described in
[runtime release notes](RUNTIME-RELEASE-NOTES.md). GPU/media integration is opt-in:
`scripts/test_pipeline_integration.py --help` lists runtime and fixture arguments.

## Remaining qualification limits

- A clean Windows VM, clean second physical machine and fresh Windows user
  profile have not been exercised. A new runtime generation in an existing
  profile is distinct from a clean OS installation.
- RTX 20/40/50 and other listed compute capabilities are compatibility contracts,
  not physical test results. The downloaded Torch build reports kernels for
  additional architectures, but this host has only the RTX 3050 Laptop GPU.
- Corporate proxies, restrictive permissions, anti-virus interference, and
  exceptionally slow disks still need machine-matrix coverage. Timeout and
  download fault fixtures cover their contracts, not every network product.
- Negative timestamp origins have unit/packet contract coverage; no recorded
  negative-origin hardware capture was available. Text subtitle handling does
  not guarantee preservation of ASS styling, and bitmap subtitles remain unsupported.
- The delivery profile is SDR H.264; HDR tone mapping is outside this release.
  Very large frame sizes can exceed available VRAM even on a supported device.
- Short tail recovery holds an existing image and reports the amount held. It
  cannot recover lost frames. Failed or interrupted work directories are retained
  conservatively rather than recursively deleting uncertain files.

The [release notes](RELEASE-v1.3.0.md) describe the user-visible changes within
these tested and supported boundaries.
