# cia render v1.3 reliability audit and delivery specification

Audit date: 2026-10-14. Baseline: v1.2.7, commit
`74b4a8786bc9f8bef88007ff9ffa58f24b450d60`.

## Decision

The current application has useful safeguards and passes its existing tests,
but it is not yet qualified as a finished product across other Windows machines.
The main problems are reproducible environment, media and lifecycle defects,
combined with a release process that does not exercise a clean installation.
Another narrow installer patch would leave several independent failures intact.

Proposed v1.3 is a reliability release: a verified, repairable managed runtime;
an explicit hardware compatibility contract; preservation of media tracks and
timing; consistent operation states; persistent diagnostics; and release gates
against the actual installer. These are acceptance requirements, not claims that
v1.3 has already been implemented or tested.

Only tracked documentation was changed; isolated reproduction helpers and
generated fixtures were created under ignored `src-tauri/target/` directories.
No application source, version,
production configuration, working runtime or user recording was modified.
No release was published.

## Scope and evidence

Reviewed the Svelte interface, Rust commands/configuration/process ownership,
Python orchestration and worker, both PowerShell installers, output validation,
packaging/updater scripts, dependency locks, documentation and CI. The audit
also exercised isolated fault fixtures and actual FFmpeg/RIFE operations.

Evidence levels used below:

- **Reproduced:** an isolated fixture or test reached the defective behavior.
  Its boundary is stated; mocked IPC is not a native Windows installation test.
- **Source-confirmed:** current code or a release artifact establishes the gap.
- **Compatibility inference:** local inventory and official requirements identify
  a risk; the affected physical machine was not available for testing.
- **Unqualified:** a required product scenario has not been tested.

The local GPU exercised was an NVIDIA GeForce RTX 3050 Laptop GPU, compute
capability 8.6, with the previously packaged private Python 3.11.9 and
Torch 2.5.1+cu121 environment. This is evidence for that environment only.
No clean Windows VM, physical RTX 20/40/50 matrix, corporate profile or native
high-DPI matrix was exercised during this audit.

### Checks performed

| Check | Result | Boundary |
| --- | --- | --- |
| `npm test` | 21 passed | Actual component handlers extracted into a mocked VM; no native app |
| `npm run build` | Passed | Production Svelte/Vite build |
| Python unit discovery | 11 passed | Input/model/ownership contracts |
| Rust default tests | 17 passed; 2 opt-in tests initially ignored | Current source, tracked-resource configuration |
| Rust opt-in media tests | Both passed | Local bundled Smoothie/FFmpeg, geometry, SAR, color/LUT, audio speed and tail recovery |
| `cargo fmt -- --check` / locked Clippy | Passed | All targets, warnings denied |
| Private Python bootstrap test | Passed | Verified cached NuGet package; isolation/reuse/repair, spaces/accents, unchanged registry/user PATH |
| v1.2.7 payload verification | Passed | 1,365 runtime files, pipeline/bootstrap/notices, compiled executable and NSIS marker |
| `npm audit` | 8 affected package entries: 5 high, 3 moderate | Development dependency graph; no shipped exploit demonstrated |
| `npm audit --omit=dev` | 0 reported | npm production dependency graph only |
| `cargo-audit` 0.22.2 | 3 vulnerability advisories, plus maintenance/unsoundness warnings | Lockfile scan; reachability requires separate analysis |
| Isolated Python package inventory/advisory scan | 29 installed packages reviewed; setuptools matches and Torch loader advisories found | Exact CUDA wheel labels skipped by PyPI lookup; separate base-version lookup explicitly distinguished |
| Targeted failures | Reproduced | Results below; summarized in `AUDIT-v1.3-EVIDENCE.json` |

There are **51 passing existing automated tests** across frontend, Python and
Rust, plus the separate PowerShell bootstrap check. Passing these tests did
not prevent the new failures: the missing scenarios matter more than a larger
test count.

## Existing behavior to preserve

- Bounded RGB streaming instead of a directory of interpolated image files.
- Strict model tensor/state validation. Keep restricted checkpoint loading,
  with a patched Torch profile: the current `weights_only=True` flag alone is
  not a sufficient safety boundary (see V13-24).
- Windows process-tree ownership, descendant termination, cancellation guards
  and backend reservation of one operation per application process.
- Immutable job settings, actual post-interpolation FPS and verified output paths.
- Exclusive output names/publication: existing destinations and original inputs
  are preserved. Auto-chain retains the good interpolation output if rendering fails.
- Lazy previews, bounded log/cache buffers and a slider animation that settles.
- Explicit rejection of unsupported HDR/bitmap subtitle delivery; fractional
  FPS, rotation and sample aspect ratio handling already covered by fixtures.
- Private, SHA-verified Python acquisition with base-runtime replacement rollback.
- Default rendering speed **1.07**, adjustable **0.5–2.0**, using
  **Rubber Band with `pitch=1.0`**. The real Smoothie integration tests passed
  at speeds 1.00, 1.07 and 0.80, including audio checks.
- The requested NVIDIA end-of-video last-frame hold. It needs a measured recovery
  budget and a visible recovery result; it should not be removed.
- English application/repository text. The removed **ADVANCED RUNTIME PATHS**
  button must remain absent from About.

## Prioritized findings

**P1** means a v1.3 release blocker: core operation, output integrity or essential
recovery/qualification is affected. **P2** means required product quality or
hardening work with lower immediate impact. Priority is the product decision,
not a claim that every item is a demonstrated security exploit.

### V13-01 — P1: runtime presence is mistaken for runtime health

**Reproduced, extracted original Rust resolver.** Eight empty placeholder files
satisfy `rife_runtime`; the result is READY, but launching the supposed Python
fails with Windows error 193. There is no executable/import/model/CUDA probe.
Both configured and discovered environments can bypass installation/repair on
this same presence check.

Source: `src-tauri/src/lib.rs:1195,1346–1372,2486–2506,2573`.

Implement separate states `missing`, `installing`, `verifying`, `ready`, `broken`
and `unsupported`. Verify interpreter identity, locked packages/DLL imports,
model/code integrity, a CUDA kernel and a real model forward pass before Ready.
Cache successful checks by runtime generation and driver/device fingerprint;
run asynchronously with a deadline. Never adopt an external environment solely
because marker files exist.

Acceptance: a removed Torch DLL, corrupt present model, missing model module or
changed/incompatible driver yields an actionable state. Contextual Repair
restores a managed environment and a real interpolation succeeds.

### V13-02 — P1: interrupted or corrupt components cannot reliably repair

**Reproduced, original PowerShell AST statements.** A corrupt existing
`flownet.pkl` fails the hash check on two retries; download calls remain **zero**.
The installer downloads only when the file is absent. Project reuse similarly
relies on `inference_video.py` being present. Packages/model files are written
into the active environment before a complete runtime verification.

Source: `bootstrap-rife.ps1:58–80,84–101,105–140`.

Install a new generation in staging, validate the complete package/project/model
manifest, then activate it atomically. Quarantine invalid components and reuse
only verified artifacts. Preserve the previous working generation through
failure. Apply the base-Python rollback contract to the entire environment.
Do not delete an external user-configured runtime when repairing the managed one.

Acceptance: cancellation, disk-full and locked files at every phase leave either
the old healthy generation or a clearly incomplete state. Retry recovers without
manual file deletion or a falsely Ready environment.

### V13-03 — P1: one CUDA profile is assumed to cover every NVIDIA GPU

**Inventory reproduced; incompatible coverage is an inference.** Every device
receives Torch 2.5.1/cu121. The local wheel lists `sm_50,60,61,70,75,80,86,90`.
The bootstrap checks a GPU name, then `cuda.is_available()` and device name;
it does not execute a kernel or the model. NVIDIA lists RTX 50 devices at compute
capability 12.0, and PyTorch introduced Blackwell/CUDA 12.8 support in 2.7.
The current profile is therefore not qualified for the broad advertised NVIDIA
contract. No physical RTX 50 failure was reproduced. See
[NVIDIA capabilities](https://developer.nvidia.com/cuda/gpus) and
[PyTorch Blackwell support](https://pytorch.org/blog/pytorch-2-7/).

Source: `bootstrap-rife.ps1:20–28,79–80,142–144`;
`rife_worker.py:68–87,164–171`.

Choose tested profiles by compute capability and driver compatibility; do not
blindly replace every older GPU profile with the newest Torch build. Query
device identity/driver/VRAM, support explicit device selection, then execute a
kernel and model inference. NVENC is a separate encoder capability check.
Detect an unsupported combination before a multi-GB download when possible.
The non-RIFE software Render path must remain available.

Acceptance: each promised GPU family/profile has recorded physical or GPU-runner
evidence; old drivers, absent NVIDIA hardware, multiple GPUs, FP16, NVENC and
low-memory cases have clear, tested outcomes.

### V13-04 — P1: actual interpolation inherits foreign Python settings

**Reproduced with healthy packaged Python/Torch.** Inherited `PYTHONHOME` makes
the private interpreter fail before startup. A controlled `PYTHONPATH` module
replaces the intended Torch import. Isolated controls succeed. The installer
clears these variables, but the render command and its child worker do not.

Source: `lib.rs:2698–2713`; `time_remap.py:197–200`.

Use one explicit child-process environment policy for orchestration, worker
and preflight: remove Python redirection/user-site interference and launch each
interpreter in isolated mode. Applying `-I` only to the parent is insufficient
because the worker is a new interpreter. See
[Python command-line isolation](https://docs.python.org/3.11/using/cmdline.html).

Acceptance: foreign Python/Conda variables, user site packages and working-directory
shadow modules cannot redirect the managed runtime; intentional app modules
continue loading through explicit paths.

### V13-05 — P1: Windows text encoding can invalidate successful interpolation

**Reproduced with forced CP1252 output streams.** A valid non-Latin output name
encodes 24 frames at 48 FPS, then the final success log raises
`UnicodeEncodeError`. Python exits 1 despite the valid encoded file. The native
failure path would prevent publication and remove its owned work directory.
This models a non-UTF-8 Windows environment; no fresh-machine native GUI test
was performed.

Source: `time_remap.py:251–257`; `lib.rs:2698–2713`.

Force UTF-8 text streams consistently, independently of Windows codepage and
inherited `PYTHONIOENCODING`. Combine isolation with explicit `-X utf8` or an
equivalent stream policy; `-I` ignores Python environment options. Logging must
not turn an otherwise validated result into failure. Decode streaming UTF-8
incrementally: current Rust pumps convert each arbitrary chunk independently,
which can replace a character split at a read boundary (`lib.rs:1443,1541`).

Acceptance: accented, Chinese/Japanese and emoji input/output/profile paths pass
under non-UTF-8 Windows settings; Unicode logs split at every byte boundary
remain intact and no good output is lost due to logging.

### V13-06 — P1: a separate microphone/game audio track disappears silently

**Reproduced through real RIFE/FFmpeg.** A valid two-AAC-track MP4 completes
successfully, but the output has **one** audio track. The encoder maps `1:a:0`,
and both Python/Rust contracts collapse audio integrity to a presence Boolean.
The original remains intact, but the reported successful output loses content.

Source: `time_remap.py:59–60,184,249–250`; `lib.rs:2315–2317`.

Default to preserving all supported audio tracks with their channels, language,
titles and timing. Any mixdown/selection must be an explicit choice before the
job, with its result validated. Extend both render pipelines' contracts to
per-track start time, duration and identity; audit Smoothie's selection behavior
with the same fixtures instead of assuming it preserves multiple tracks.

Acceptance: game + microphone, mono/stereo/multichannel, silent and unsupported
audio fixtures follow the declared policy; no successful output silently drops
a track. Playback speed keeps pitch and all tracks synchronized.

### V13-07 — P1: valid nonzero video start times fail interpolation

**Reproduced through real RIFE/FFmpeg.** A fixture contains 12 frames at 24 FPS,
video start time 2.000 s, video duration 0.500 s, and audio starting at zero.
The decoder's `fps` filter produces 60 input frames/120 interpolated frames;
the complete-CFR check rejects the result.

Source: `rife_worker.py:99–102`; `time_remap.py:46–60,239–242`.

Define a common audio/video timeline using stream start times, edit lists and
PTS. Normalize the intended interval explicitly for the decoder, audio and
subtitles. Resetting video timestamps alone would risk desynchronization.

Acceptance: positive/negative offsets, delayed audio, MP4 edit lists, VFR and
fractional FPS preserve the declared timeline and A/V offsets. Real damaged
media remains distinguishable from a valid offset recording.

### V13-08 — P1: updating can interrupt RIFE installation

**Reproduced in compiled Svelte/Edge with mocked native IPC.** Start Install
Environment, navigate to About, open the available update, then select
Update & Relaunch. The UI invokes updater installation and restart while the
bootstrap remains pending. Closing the native app terminates registered jobs.

Source: `App.svelte:374,1245–1266,1364–1391`;
`lib.rs:3004–3010,3035–3041`.

Use a single operation coordinator covering installation/repair, rendering,
verification, update installation and shutdown. Enforce admission at the
native boundary where applicable, not only through a disabled button. Read-only
update checks may continue; installing/restarting must wait for idle or an
explicitly completed cancellation.

Acceptance: all operation pairs have tested admission rules. An update cannot
restart during bootstrap/render/verification; a failed update leaves the app
and prior runtime usable. Test the real NSIS update from v1.2.5/6/7.

### V13-09 — P1: interpolation failure details disappear

**Reproduced in mounted Svelte/Edge with mocked failure.** When RIFE fails before
producing an output, `jobError` is assigned but not rendered in the settings
workspace. Its only toast disappears after four seconds. No Copy Logs action
remains. Installation failures have a persistent error; render failures need
the same product contract.

Source: `App.svelte:1024–1034,1647–1651,1669–1708,1570–1572`.

Persist the last operation result until dismissed/retried. Display a concise
cause and next action, with copy/export diagnostics available for every failure.
Keep a bounded per-job diagnostic file across restart, including stage, engine,
runtime profile, device/driver and sanitized command/error details. Do not upload
recordings or include secrets/complete environment variables in support bundles.

Acceptance: CUDA/OOM/decode/encoder/permission/install errors remain actionable
after navigation/restart; exported diagnostics identify the exact failed stage.

### V13-10 — P1: configuration and runtime recovery are unreachable

**Reproduced initialization; UI reachability source-confirmed.** A snapshot
`loadError` is ignored by initialization and the recovery panel stays hidden.
`openRuntimeSetup` has no remaining UI caller. Stale external overrides can
block bundled fallback. A new user also cannot select a LUT despite the feature
being exposed in Details.

Source: `App.svelte:119–145,1299–1308`; `lib.rs:903–920,1209–1217`.

Keep About free of the removed advanced-path button. Provide contextual
Interpolation Repair/Reset, recovery for invalid overrides/configuration and a
LUT picker beside the LUT setting. Preserve a corrupt config backup before
reset; migrate preferences independently of runtime paths. Surface startup
load failures instead of silently using a hidden draft.

Acceptance: corrupt JSON, stale paths, schema upgrade and missing components
recover through ordinary app controls; no JSON editing or screenshots of a
hidden settings panel are required.

### V13-11 — P1: installation has no bounded cancellation/retry contract

**Source-confirmed; missing Cancel reproduced in mounted UI.** Web requests have
no explicit timeout/retry policy, setup has seven broad phase labels but no byte
progress, disk budget or cancel action. Model acquisition depends on Google
Drive. PowerShell 5.1 documents `TimeoutSec=0` as an indefinite default;
[Microsoft request documentation](https://learn.microsoft.com/en-us/powershell/module/microsoft.powershell.utility/invoke-webrequest?view=powershell-5.1).

Source: `bootstrap-python.ps1:42`; `bootstrap-rife.ps1:87–110`;
`App.svelte:1552–1564`.

Provide bounded connect/idle deadlines, retry/backoff, reusable verified cache,
byte progress, disk budgeting for download/extraction/rollback, and safe
Cancel/Retry. Distinguish offline/proxy/TLS/hash/quota/locked-file failures.
Use stable verified distribution artifacts; an offline interpolation pack is
optional unless offline setup becomes part of the product promise.

Acceptance: disconnect, stalled response, throttled network, Drive HTML/403,
wrong hash, disk-full and cancel at every phase produce recoverable states,
with the old runtime preserved and no hidden continuing process.

### V13-12 — P1: Python dependency acquisition changes with machine/date

**Machine setting reproduced; resolver drift source-confirmed.** Controlled
`PIP_NO_INDEX=1` makes a nonmutating private-Python pip dry run fail. Bootstrap
inherits pip configuration, upgrades unconstrained pip and locks only direct
dependencies. Project/model-code archives are not fully hash-manifested.

Source: `bootstrap-rife.ps1:73–80,87–89,129–139`.

Create a complete, versioned Windows wheel/project/model-code manifest including
all transitive dependencies, artifacts and hashes. Install the tested manifest,
not the resolver's current choices. Isolate package-selection configuration;
support intentional proxy/certificate configuration through a documented
transport contract. Verify the code shipped with the model, not just weights.
See [pip secure installs](https://pip.pypa.io/en/stable/topics/secure-installs/).

Acceptance: independent empty profiles resolve identical manifests; arbitrary
user pip configuration cannot change the runtime; valid cached installation
works and any artifact mismatch leaves the previous generation active.

### V13-13 — P1: output recovery can hide substantial loss

**Exact FFmpeg output-stage filter reproduced; not a full Smoothie failure.**
The current tail filter makes 50 frames/2.000 s at 25 FPS from one real 0.040 s
frame. All decoded hashes are identical: **49 clones, 98% frozen output**.
These metadata meet the current validation contract. Recovery is bounded by
the planned duration, but not by the amount of real video lost.

Source: `lib.rs:2287–2298,2302–2343`.

Keep the requested short tail hold. Count real/padded frames, report recovered
duration in the result and define a recovery budget. A proposed starting
budget is at most 1 second and 2% of intended duration; calibrate it against the
known NVIDIA fixture and document the final policy. More extensive loss requires
an explicit recovered-result status/decision rather than ordinary success.
Fatal engine errors and files without a valid decoded image remain failures.

There is also a **source-confirmed count-validation gap**: inconsistent or absent
`nb_frames` becomes `None` (`lib.rs:1721–1727`), and exact-count comparison then
does not run (`2336–2342`). Require a decoded-count fallback when an expected
count exists, plus per-track A/V validation, without relaxing existing checks.

Acceptance: the reported NVIDIA short tail still succeeds with a visible hold
count; a one-frame/long-duration result is not silently called normal. Missing
metadata cannot bypass expected-frame verification.

### V13-14 — P1: multiple app processes can share an installation unsafely

**Source-confirmed; no destructive concurrent installation executed.**
`CONFIG_WRITE_LOCK` and `JobRegistry` are process-local; runtime paths are shared
per user. There is no single-instance plugin or cross-process environment lock.
Two processes can enter installation or overwrite configuration independently.

Source: `lib.rs:21,271,1079–1083,2484,3003`; `Cargo.toml`.

Choose a single-instance application contract or enforce cross-process operation
locks (runtime installation/configuration still need durable serialization).
Use stale-owner recovery and atomic activation. A second launch should focus
the existing window or clearly report the active operation.

Acceptance: simultaneous launches and Repair requests do not race pip, staging,
model promotion or preference writes. A crashed lock owner can be recovered.

### V13-15 — P1: rapid Pause clicks produce contradictory states

**Reproduced with actual handlers and deferred IPC.** Two clicks send
`pause_render` twice. The backend pause is idempotent; both responses toggle the
UI flag, leaving the process paused while the interface shows it running.

Source: `App.svelte:654–665`; `lib.rs:2601–2646`.

Serialize control commands, disable the control while pending, and set state
from a backend acknowledgement instead of toggling a local Boolean. Stress-test
pause/resume/cancel during descendant startup. Watchdogs must respect paused time.

Acceptance: repeated/rapid clicks, late responses and failure do not invert the
displayed state or leave an unresponsive Resume/Cancel operation.

### V13-16 — P2: invalid preferences can be saved as successful

**Reproduced with actual handlers.** Speed `0` saves and closes Details despite
the input's declared minimum `0.5`. Preferences are stored as arbitrary JSON;
render-time validation rejects them later.

Source: `App.svelte:99–110,358–365`;
`lib.rs:2434–2447`, `UiSettings`.

Use typed validated preferences and a versioned migration. Validate before
announcing Saved, restore/reject invalid stored enum/range/type values, and keep
the dialog open with the field error.

Acceptance: blank/zero/nonfinite speed, invalid FPS/factor/encoder/precision and
old preference schemas never persist as apparently valid settings.

### V13-17 — P1: release qualification does not cover a fresh installation

**Source/artifact-confirmed and unqualified.** CI excludes the binary runtime
payload and does not run an installer, native UI, updater migration or full GPU
bootstrap. v1.2.7 `RELEASE-VALIDATION.json` explicitly says installation and full
GUI navigation on a fresh Windows user profile were not tested. Its GPU evidence
is inherited from the byte-identical v1.2.6 runtime on the developer machine.

Source: `.github/workflows/ci.yml`; published/local release validation record.

Add an exact-artifact release pipeline: acquire/verify payload, build NSIS,
install on disposable clean Windows profiles, perform native smoke tests and
GPU qualification, exercise upgrades/uninstall, then verify hashes/signatures
and the updater manifest before publication. This complements source-only PR CI.

Acceptance: the matrix below has per-artifact evidence and no known P1 failure.
An unavailable required GPU/VM row stays unqualified; it is not marked passed
because the developer-machine tests passed.

### V13-18 — P1: release acquisition and installation promises are incomplete

**Source/artifact-confirmed.** `prepare-release.py` requires an already-staged
ignored Smoothie archive and copies FFmpeg from an ignored local installation.
The build identifier is checked and hashes are recorded, but acquisition is not
fully scripted/pinned. A source checkout lacks the media payload.

Generated v1.2.7 NSIS uses `downloadBootstrapper` (installer script line 52).
It needs network access when WebView2 is missing, contrary to the unconditional
README wording about no extra downloads. Tauri supports an offline WebView2
installer; Microsoft also describes the standalone deployment path.
[Tauri installation modes](https://v2.tauri.app/distribute/windows-installer/),
[Microsoft offline deployment](https://learn.microsoft.com/en-us/microsoft-edge/webview2/concepts/distribution).

The installer is **NotSigned** under Authenticode. Its updater signature is
present and separately verified; it does not establish a Windows publisher.
Treat Authenticode/reputation as a distribution-quality decision, not a proof
that the updater is unsigned.

Implement immutable acquisition URLs/hashes/licenses for the complete payload,
toolchain pinning and an automated staging verifier. Choose the offline install
promise explicitly: recommended v1.3 ships the Evergreen standalone WebView2
installer and tests no-WebView2/offline launch. Record exact hashes and
provenance; byte-identical builds are a separate requirement if pursued.

Acceptance: a clean runner can recreate the validated payload without personal
folders; Render installs/launches according to the documented network contract.
Installer, installed executable and updater signatures are reported distinctly.

### V13-19 — P2: dependency and frontend hardening are not release gates

**Scans reproduced; exploits not demonstrated.** npm reports eight affected
development package entries; its production graph scan reports zero. RustSec
reports `quick-xml 0.39.4` with two XML denial-of-service advisories and
`rustls 0.23.43` with the TLS handshake advisory. The Windows dependency tree
places rustls on the updater's reqwest path; quick-xml enters through
plist/tauri-utils. Neither scanner proves application-level exploitability.
[XML duplicate checks](https://rustsec.org/advisories/RUSTSEC-2026-0194.html),
[XML namespace allocation](https://rustsec.org/advisories/RUSTSEC-2026-0195.html),
[TLS advisory and patched versions](https://rustsec.org/advisories/RUSTSEC-2026-0285.html).

Cargo also reports six unmaintained-crate entries and three unsoundness entries.
`anyhow 1.0.102` is present in the Windows tree; its reported issue has a patch
in `>=1.0.103`. [anyhow advisory](https://rustsec.org/advisories/RUSTSEC-2026-0190.html).
`event-listener` and `glib` are absent from the inspected Windows target tree.
Passing `--target-os windows` to this lockfile scan does not replace
dependency-tree/reachability analysis. The current plist consumer uses a plain
XML Reader; inspection did not find the affected namespace/attribute-checking
calls. Keep the quick-xml matches visible without claiming a demonstrated
vulnerable application path.
The app's CSP is currently `null`; no XSS exploit was found in this audit.

Update affected dependency groups deliberately, with compatible upstream
versions and regression tests. Add scheduled/admission scans, SBOM/inventory and
documented exceptions with reachability rationale. Apply a restrictive tested
CSP/IPC policy compatible with local media previews and fonts. Avoid an
unreviewed major-version `audit fix --force` before a release.

Acceptance: known applicable advisories are resolved or explicitly reviewed,
all checks remain green, previews/updater work, and no unsupported claim of
zero security risk is attached to a scanner result.

### V13-20 — P2: crash cleanup and stalled subprocess handling are incomplete

**Source-confirmed; crash/stall matrix unqualified.** Normal cancellation is
well covered. Abrupt termination cannot run `Drop` cleanup; reservation lock
files/work directories have no restart-recovery policy. Probe/render/descendant
waits have no application-level stall deadline. Current exact-count validation
does not itself establish full decodability or synchronized per-track audio.

Source: `lib.rs:175–185,617–622,1602–1638,2346–2358,2361–2408`;
Python subprocess waits.

Record owned operation journals with owner identity and recovery markers; clean
only safely identified orphan artifacts. Add bounded probe and stage-aware,
pause-aware watchdogs with useful diagnostics. Retain explicit output checks
and add decode/track integrity checks at an appropriate cost.

Acceptance: force-kill/restart, encoder/worker stall, sleep/resume and cancel
leave no active descendants, false completion or deletion of unrelated files.

### V13-21 — P2: bulk machine-specific runtime lives in Roaming AppData

**Source-confirmed; redirected-profile failures are an inference.**
`rife_install_root` uses `app_data_dir`, which the installed Tauri/dirs Windows
implementation resolves to RoamingAppData. The several-GB CUDA/Python runtime
is machine-specific and includes absolute virtual-environment base paths.

Source: `lib.rs:1079–1083`; installed Tauri path/dirs implementations.

Store managed runtimes/cache in LocalAppData and keep small preferences in the
configuration location. Migrate by constructing/verifying a local generation;
blindly moving a venv can break its base interpreter paths. Preserve the old
working generation and preferences until migration succeeds.

Acceptance: upgrade and redirected/quota-limited profiles do not place large
runtime writes in RoamingAppData or break the private interpreter.

### V13-22 — P2: product accessibility and diagnostics presentation need qualification

**Source-confirmed choices; native behavior unqualified.** The window is fixed
at 920×540 and non-resizable. High-DPI/small work areas, keyboard navigation,
focus/modal behavior and reduced-motion preferences have no native release
coverage. Startup version has a hardcoded `1.0.3` fallback.

Source: `tauri.conf.json` main window; `App.svelte:41,119–145` and modal markup.

Support a responsive usable work area with scrolling/resizing where necessary,
keyboard actions/focus restoration and persistent status. Resolve version
independently of runtime initialization. Keep product language English.

Acceptance: ordinary workflows work at 100/125/150/200% scaling and the declared
minimum work area, with keyboard-only controls and no inaccessible recovery.

### V13-23 — P2: shared contracts and state are too concentrated

**Source-confirmed maintainability issue.** `App.svelte` has 4,149 lines including
styles and `lib.rs` has 3,600 including tests. Runtime/setup/UI states are many
independent flags; process/media contracts are duplicated in Rust and Python.
The observed updater/pause/validation defects cross those boundaries.

Extract operation state, typed preferences, runtime management, diagnostics and
media contracts into testable modules; split page components after the new
regression fixtures exist. Keep rendering engines intact during this work.
Use one serialized operation state and backend-owned facts. Avoid a broad
rewrite that drops the existing safeguards or changes algorithms by accident.

Acceptance: mounted UI tests and native integration tests cover actual state
transitions and commands, not only extracted functions; refactoring preserves
all validated output/audio/process behavior.

### V13-24 — P1: the pinned Torch checkpoint loader has applicable advisories

**Official advisories and affected code confirmed; no exploit executed.** The
managed runtime installs Torch 2.5.1, and the worker calls
`torch.load(..., weights_only=True)`. PyTorch's critical CVE-2025-32434 advisory
describes code execution through crafted checkpoints in versions through 2.5.1,
including that mode; it is patched in 2.6.0. A later high-severity advisory,
CVE-2026-24747, affects that unpickler through 2.9.1 and is patched in 2.10.0.
Upgrading only to 2.6 would therefore not address both known loader issues.
See the official [2025 loader advisory](https://github.com/pytorch/pytorch/security/advisories/GHSA-53q9-r3pm-6pq6)
and [2026 unpickler advisory](https://github.com/pytorch/pytorch/security/advisories/GHSA-63cw-57p8-fm3p).

Source: `bootstrap-rife.ps1:79–80`; `rife_worker.py:32`; presence-only
runtime/model adoption in `lib.rs:1346–1372,2486–2506`.

The SHA-verified official model in a fresh managed installation mitigates the
crafted-model path. Ongoing readiness/external adoption do not perform that
integrity check. Selecting external Python model code is already a trust decision;
this audit does not claim that loading arbitrary external code is newly safe.

The separate Python inventory scan covers 29 installed packages. Exact
`+cu121` versions were skipped by PyPI's lookup; a second lookup deliberately
uses their base versions for advisory review, not dependency resolution or
proof about every wheel build. It also identifies setuptools 65.5.0 matches.
Raw provider entries contain duplicates and advisories for other APIs; do not
call their total a count of exploitable app defects.

Use a currently patched, tested Torch profile, review the other Python package
matches and enforce provenance/integrity before loading managed model/code
artifacts. Review the interpreter/native-library inventory too; npm/Rust/PyPI
scanners do not cover every bundled binary. Do not rely on strict
state-dictionary checks after loading to prevent a loader vulnerability.

Acceptance: no candidate managed profile contains either affected loader range
without a reviewed patched backport; tampered managed artifacts are rejected
before load; and the patched profile passes the promised GPU/precision/media
matrix. No malicious checkpoint is needed to test integrity rejection.

## Proposed v1.3 product contract

| Area | Required behavior |
| --- | --- |
| Supported platform | Windows x64 versions explicitly qualified by the release matrix; no implicit all-Windows/all-NVIDIA promise |
| Render | Bundled verified tools, software encoder usable without RIFE/NVIDIA; documented WebView2/network requirement |
| Interpolation | Opt-in managed environment with profile/hardware preflight, progress/cancel/retry and contextual Repair |
| Unsupported hardware/media | Clear preflight explanation and usable supported functions; no falsely Ready status |
| Audio | Default preserve supported tracks; pitch preserved during render speed; explicit track/mix policy |
| Timing/integrity | Common A/V timeline, exact planned cadence/count, per-track checks, measured tail recovery |
| Lifecycle | One coherent operation state; native admission; acknowledged pause/resume; updates only when safe |
| Recovery | Repair/reset without manual Python/config editing; no restoration of the unwanted About button |
| Diagnostics | Persistent operation error and safe copy/export support bundle; no media uploads |
| Distribution | Clean-runner payload acquisition, installer smoke tests, upgrade verification, integrity/provenance records |

Rendering speed remains 1.07 by default with Rubber Band. Interpolation slow-motion
currently uses `atempo` (`time_remap.py:190`); this is distinct from the working
render-speed path. For consistent product behavior, propose Rubber Band for
retimed interpolation audio too, with extreme factors tested before replacing
that filter. The user has explicitly preferred Rubber Band; do not silently
substitute another algorithm.

## Implementation order and reviewable milestones

| Milestone | Work | Completion evidence |
| --- | --- | --- |
| M1: turn findings into regressions | Promote isolated fixtures into portable handler/mounted UI/media/bootstrap tests; parameterize current developer-relative integration paths | The relevant tests fail on v1.2.7 for the documented reason; no real user recordings required |
| M2: repair installation and runtime contract | V13-01/02/03/04/05/11/12/14/21/24: patched immutable profiles, full health, environment/UTF-8 policy, transaction/lock/cache/local storage, contextual repair | Repeated fresh/repair/interruption tests and actual GPU inference per promised profile |
| M3: protect media content | V13-06/07/13/20: per-track timeline/mapping, recovery accounting, count fallback, stage watchdog/restart cleanup | Generated fixtures validate tracks, timing, decoded frames and no-overwrite behavior |
| M4: make the UI truthful | V13-08/09/10/15/16/22/23: operation coordinator, persistent results, native acknowledged controls, validated preferences, contextual LUT/recovery, responsive pages | Mounted and native interaction tests; failed/cancelled jobs remain useful and retryable |
| M5: qualify the exact release | V13-17/18/19: reproducible acquisition, reviewed dependencies, clean installer/updater/GPU matrix, release manifest | All promised rows green on the same candidate artifact; signed updater/hash verification after publication |

Implementation should proceed through these reviewed milestones rather than
publishing each small fix as a finished v1.3. Source CI and disposable tests can
be automated; GPU/OS qualification requires the corresponding machines/runners.
Missing infrastructure must be reported as a pending release gate.

## Mandatory qualification matrix

These are **proposed release gates, currently pending unless specifically
marked as local evidence above**. Exact driver/OS/profile versions and artifact
hashes must accompany each run.

| Dimension | Required scenarios | Acceptance |
| --- | --- | --- |
| Installation | Clean standard Windows x64 user; no Python/FFmpeg/dev folders; WebView2 present/absent; online/offline per promise; spaces/non-Latin profile | Install, launch and real Render without developer-machine dependencies |
| Upgrade/uninstall | v1.2.5, 1.2.6 and 1.2.7 to candidate; good/broken runtime; invalid/stale config; uninstall/reinstall | Preferences and user media preserved; managed environment repaired; signatures validated |
| GPU | Promised RTX 20/30/40/50 profiles; hybrid laptop; multi-GPU; old driver; no NVIDIA; low VRAM | Preflight correctly accepts/rejects, a real model runs on accepted devices, Render remains usable |
| Setup faults | Disconnect/stall/slow link; proxy/certificate; wrong hash; partial archive/model; disk-full; locked file; cancel/kill at every phase | Bounded, informative failure; repeatable repair; last good generation preserved |
| Media | H.264/H.265 SDR; 24/25/30/60/120/360 FPS; fractional/VFR; positive/negative PTS; rotation/SAR; short/long clips | Intended cadence/timeline and dimensions; supported/rejected formats declared |
| Audio/subtitles | No audio, game+mic tracks, codec/channel variants, delayed/long/short audio, text/bitmap subtitles | Declared tracks/content/timing preserved or explicit preflight rejection |
| Speed/tail | Speed 0.5/0.8/1.0/1.07/2.0; known NVIDIA short tail; absent frames; excessive loss | Pitch policy unchanged, exact duration/count, recovery measured and clearly reported |
| Controls | Double Pause, startup pause, resume/cancel, page changes, second launch, close/restart, update conflicts | Backend/UI state agree, no concurrent installation corruption or orphan processes |
| Paths/resources | Non-UTF-8 codepage; accents/CJK/emoji; long paths and UNC if promised; read-only destinations; collisions; free-space exhaustion | Safe actionable outcome, no overwrite or deletion of unrelated files |
| Native UI | Minimum work area; 100/125/150/200% DPI; keyboard/focus; error/retry/export; update modal | Every supported workflow and recovery action remains accessible |
| Endurance | Repeated generated jobs; long 360-FPS input; sleep/resume; interrupted shutdown | Stable resource use, no leaked processes/locks or false success |

## Performance work that supports reliability

No new universal speedup percentage is claimed by this audit. The additional
GPU runs establish correctness/failures, not a controlled benchmark. Existing
streaming and lazy-preview work should remain the baseline.

1. **Bound memory by workload.** Record RAM/VRAM for 1080p/1440p/4K and each
   precision/profile. Add a tested memory strategy with a clear capacity estimate
   and controlled OOM result/retry. Tiling or reduced precision needs visual
   quality/scene tests and must not silently reduce output quality.
2. **Avoid wasted installation/render work.** Compatibility, disk, codec and
   model checks before large downloads/full jobs have more immediate value than
   speculative rendering micro-optimizations. Cache health checks by fingerprint.
3. **Measure pipeline stages.** Separate decode, inference, transfer, encode,
   validation and preview costs. Test software/NVENC throughput on named hardware;
   do not equate an offered encoder option with tested availability.
4. **Protect responsiveness.** Keep progress/log frequency bounded, previews
   deferred while busy, health/validation asynchronous and controls acknowledged.
   Record idle CPU, UI interaction latency and memory/process counts on the
   reference machines rather than assuming a fixed window proves responsiveness.
5. **Detect leaks.** Repeated short jobs and long clips should show stable RAM,
   VRAM and descendant-process counts after cleanup. Consider retaining a model
   worker only after profiling shows startup is significant and a lifecycle test
   proves it safe; it is not a prerequisite for reliability.

## Release acceptance

Publish v1.3 only when all known P1 findings are resolved, every promised matrix
row has evidence for the exact candidate, existing safeguards and the new failing
fixtures pass, and installer/updater/payload checks are attached to the release.
P2 items need either completion or a reviewed, explicit limitation that does not
contradict the finished-product contract. No unsupported hardware is silently
counted as supported and no unexecuted matrix row is labeled passed.

This produces a testable reliability commitment. It does not claim that every
possible Windows configuration or damaged recording can be rendered successfully.

## Local evidence and repeatability

Machine-independent summaries and source baseline are in
[AUDIT-v1.3-EVIDENCE.json](AUDIT-v1.3-EVIDENCE.json). Raw fixtures are intentionally
kept under ignored `src-tauri/target/`, not packaged or published with user paths.

Local reproduction helpers:

- `target/v1.3-audit-portability/readiness-repro.rs` and
  `model-retry-repro.ps1`: original resolver/AST with isolated filesystem stubs.
- `target/v1.3-audit/product-handler-repros.cjs` and
  `product-browser-repros.cjs`: original handlers/compiled Svelte with mocked IPC.
- `target/v1.3-audit/pipeline/audit_pipeline_probe.py` and
  `tail_policy_probe.py`: generated media and the working private GPU environment.
- `target/v1.3-audit/npm-audit*.json`, `cargo-audit*.json`: dated advisory snapshots.

Existing checks can be repeated with the documented source-only `TAURI_CONFIG`
from CI; runtime/GPU checks require explicit fixture/runtime paths. The raw
helpers currently reference this developer workspace and should be parameterized
when promoted into CI. They are audit evidence, not an already-portable v1.3 suite.
