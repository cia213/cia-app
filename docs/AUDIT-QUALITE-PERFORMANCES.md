# cia render code quality and performance audit

**Date: September 30, 2026 · version 1.2.3 · audited commit: `241f350`.**

> Historical audit of the code **before fixes**. The findings, line numbers, and measurements below describe that revision, not the current patch. The fixes and their validation are documented in [PATCH-QUALITE-PERFORMANCES.md](PATCH-QUALITE-PERFORMANCES.md). The microbenchmarks in this audit are not measured gains for the patched application.

Scope: Svelte/Tauri sources, bundled Python script, standalone `time-remap-app` script, local RIFE engine, and local Smoothie runtime. Binary resources ignored by Git are assessed as components of the local installation, without assuming that another release contains exactly the same files.

## Conclusion

The normal workflows work in the tests performed: the frontend builds, all seven Rust tests pass, and the RIFE/Smoothie engines produce videos in the benchmarks. Output name checks, versioned configuration, log streaming, and separate runtimes provide a sound foundation.

The priorities, however, are **ownership of jobs and temporary files**, followed by **consistency between displayed settings and the settings actually applied**. The main performance gains lie in repeated video passes and engine/encoder choices, more than in frontend size.

The connection between the two audits is direct: a better-defined pipeline prevents accidental concurrent processing, unnecessary decoding, uncontrolled intermediate files, and full restarts after errors.

## Checks and methodology

| Check | Result |
| --- | --- |
| `npm run build` | Passed; generated JS ≈ 126.79 kB, gzip ≈ 39.67 kB |
| `cargo test --manifest-path src-tauri/Cargo.toml` | 7 tests passed, no failures |
| `cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings` | Failed: 3 diagnostics, described below |
| Syntax of both Python scripts using `ast.parse` | Valid |
| NVIDIA / PyTorch | RTX 3050 Laptop, 4 GiB VRAM; PyTorch 2.5.1+cu121; CUDA available |
| Local RIFE model | SHA-256 matches the 4.26 hash expected by the bootstrap |
| Benchmarks | Two runs per RIFE variant; three per preview/FFmpeg variant; additional Smoothie measurements |

Performance source: `test_clip_topaz_source_2s.mp4`, 1680 × 1050, 120 FPS, 242 frames, and ≈ 2.017 s. For RIFE: temporary 48-frame excerpt, 0.4 s, same resolution, ×2 factor. All benchmark video outputs are created in temporary directories and then deleted. The sources and production code remain unchanged.

Timings include process startup, model loading, and writing. These are **local microbenchmarks**, with caches and a non-randomized execution order; they do not measure steady-state throughput on a long video. No comparative FP16/FP32 visual assessment, no peak RAM/VRAM measurement, and no complete 4K/HDR/VFR benchmark. Percentages cannot be added across stages.

Raw data is available in [AUDIT-MESURES.json](AUDIT-MESURES.json) and [AUDIT-PREUVES.json](AUDIT-PREUVES.json). Reproduction scripts are `C:/Users/cia/time-remap-app/audit_quality_performance.py` and `audit_quality_checks.py`; run these scripts with the local virtual environment's Python. The second script adds Smoothie measurements with identical filters.

## Code quality audit — priority issues

Priorities: **P1** = priority fix, with a risk to data/results or loss of processing control; **P2** = functional reliability and resource management; **P3** = maintenance and secondary optimization. "Reproduced" means a check was executed; "code inspection" means a path was confirmed in the code without running the complete scenario.

### Q1 — P1: history navigation can make an active job disappear

**Source:** [App.svelte, `applyHistoryEntry`](https://github.com/cia213/cia-app/blob/241f350025e7e8504d0f5eae1f3c46ea283778cd/src/App.svelte#L232), history buttons around L1248, and `anyProcessing` L335. **Evidence: reproduced by executing the extracted function with Node.**

If a render is active and the user returns to an entry with no selected video, `applyHistoryEntry` sets `isSmoothieProcessing` or `isProcessing` to `false`. No process stop is requested. History buttons and Alt+arrow shortcuts remain available during rendering. `anyProcessing` becomes false and can allow another operation while the first continues. Logs and `activeRenderJobId` are shared, compromising subsequent tracking and cancellation.

**Fix:** separate navigation state from job state; keep the active job in a dedicated store, attached to an immutable entry (source, settings, destination). Add a global backend guard if only one operation is allowed. Navigation can change the page without changing whether a job exists.

**Verification:** start operation A, navigate backward, attempt B, then cancel A; A must remain visible and controllable, and B must respect the concurrency policy.

### Q2 — P1: the RIFE intermediate file can overwrite and then delete an existing file

**Sources:** [bundled script L134](https://github.com/cia213/cia-app/blob/241f350025e7e8504d0f5eae1f3c46ea283778cd/src-tauri/resources/time_remap.py#L134), search L181 and deletion L289; `Practical-RIFE/inference_video.py` L134–138. **Evidence: complete path inspected, without a destructive test.**

Rust reserves the final destination, but the script does not pass `--output` to RIFE. RIFE opens its default name, derived from the source, with `cv2.VideoWriter`. If `clip_2X_240fps.mp4` already exists, opening it can overwrite it. After encoding, the script deletes that same file. The lock on `clip-240fps.mp4` does not protect it.

**Fix:** a unique temporary directory per job, an explicit intermediate path passed to `--output`, and cleanup limited to files owned by that job. Remove discovery by glob/modification time. Keep a validated result if the next stage fails.

**Verification:** create the historical RIFE filename beforehand with known contents; its hash must remain unchanged after success, failure, and cancellation.

### Q3 — P1: incomplete process lifecycle management

**Sources:** [lib.rs L1600](https://github.com/cia213/cia-app/blob/241f350025e7e8504d0f5eae1f3c46ea283778cd/src-tauri/src/lib.rs#L1600), L1688, and [close button L1279](https://github.com/cia213/cia-app/blob/241f350025e7e8504d0f5eae1f3c46ea283778cd/src/App.svelte#L1279). **Evidence: code inspection.**

Engines are launched before `register_job`. An empty or already-used identifier therefore causes registration to fail after startup. Early returns through `?` do not guarantee that the process stops or is removed from the registry. No handling of window closure/application shutdown or Windows ownership of the process tree is defined. Child processes can continue running or remain suspended after closing.

**Fix:** reserve the job before spawning; centralize launching in a manager with guaranteed finalization. On Windows, consider a Job Object to own the process tree and terminate descendants on closure. `kill_on_drop` can supplement protection of the parent, but is insufficient to guarantee that the whole tree stops.

**Verification:** identifier rejection, read error, closure during processing, and closure while paused; no engine or lock should remain orphaned.

### Q4 — P1 for the local runtime: the Smoothie recipe distorts some aspect ratios

**Source:** `src-tauri/resources/runtime/smoothie/recipe.ini`, L25. **Evidence: expression inspected; dimensions verified in Smoothie measurements with identical filters.**

Width and height are rounded independently to stepped values, then `setsar=1/1` is applied. Thus, 1680 × 1050 (16:10) becomes 1920 × 1080 (16:9). This processing also increases the number of pixels to encode. Vertical videos or videos outside 16:9 can also change proportions.

**Fix:** preserve dimensions by default; offer an explicit resolution that preserves the aspect ratio, with even dimensions and padding if needed. Also audit `defaults.ini`, which contains different fallback arguments, including an audio filter `rubberband=tempo=1.07`.

**Verification:** 16:10, 4:3, 16:9, and vertical sources; compare display aspect ratio, audio duration, and final dimensions. This finding concerns the local runtime ignored by Git, not every published release.

### Q5 — P2: settings are visible but not applied

**Sources:** [Svelte overrides L1017](https://github.com/cia213/cia-app/blob/241f350025e7e8504d0f5eae1f3c46ea283778cd/src/App.svelte#L1017), [Smoothie runtime L786](https://github.com/cia213/cia-app/blob/241f350025e7e8504d0f5eae1f3c46ea283778cd/src-tauri/src/lib.rs#L786), [Smoothie launch L1650](https://github.com/cia213/cia-app/blob/241f350025e7e8504d0f5eae1f3c46ea283778cd/src-tauri/src/lib.rs#L1650), [RIFE runtime L763](https://github.com/cia213/cia-app/blob/241f350025e7e8504d0f5eae1f3c46ea283778cd/src-tauri/src/lib.rs#L763), Python script L129–136.

| Setting | Observed behavior | Fix |
| --- | --- | --- |
| `sceneThreshold` / `blendCuts` | The former changes the count, the latter is displayed; timestamps do not control RIFE and no fade is applied | Implement the behavior and tests, or remove/disable the setting |
| Color correction | Brightness/saturation/contrast are passed, but no `color grading;enabled;yes` override; the local recipe disables this section | Define activation and parameters together |
| Selected recipe | The path is saved/displayed but not passed to smoothie-rs with `--recipe` | Add the effective path to `SmoothieRuntimePaths` and the command |
| Selected LUT | `lutFile` is stored, but its path is absent from the render command | Pass the path with actual validation |
| Selected RIFE model | `modelFile` is validated, but no `--model` is passed; inference also imports code from `train_log` | Define a consistent model installation containing code and weights, then use that path |

The Smoothie engine does expose `--recipe` in its help. Color settings and the effective recipe are checked using `--return-recipe` in the supplementary evidence. Path validation alone does not mean that a setting is used.

### Q6 — P2: input and frame rate validation are too permissive

**Sources:** [lib.rs L924](https://github.com/cia213/cia-app/blob/241f350025e7e8504d0f5eae1f3c46ea283778cd/src-tauri/src/lib.rs#L924), L1221, [Python L96](https://github.com/cia213/cia-app/blob/241f350025e7e8504d0f5eae1f3c46ea283778cd/src-tauri/resources/time_remap.py#L96), and `inference_video.py` L68. **Evidence: code inspection.**

Rust accepts any finite positive factor; the RIFE CLI requires an integer and the UI provides 2 to 10. CRF, preset, and threshold are not validated by a shared structure. Preferences are unrestricted JSON objects. FFprobe output can be interpreted as video with zero dimensions/duration instead of being rejected. Frame rate uses `r_frame_rate` followed by rounding, while RIFE reads FPS through OpenCV; recent file discovery resolves the name without defining a VFR policy or ensuring an accurate rational frame rate.

**Fix:** typed models and backend validation before launch; rational FPS, valid duration/stream; an explicit VFR policy (preserve timestamps or normalize once). Accept an integer ×2…×10 while that is the actual contract.

**Verification:** out-of-range parameters, 23.976/29.97 FPS, variable frame rate, audio-only files, and unreadable sources. Compare duration, timestamps, and frame count.

### Q7 — P2: a nonempty file is considered a valid result

**Sources:** [lib.rs L1262](https://github.com/cia213/cia-app/blob/241f350025e7e8504d0f5eae1f3c46ea283778cd/src-tauri/src/lib.rs#L1262); RIFE `build_read_buffer` L152–162 and finalization L275–279. **Evidence: code inspection; normal output measured.**

The RIFE reader hides every exception with `except: pass`, which can turn a decoding error into end-of-video. Finalization waits until the queue is empty without joining the writer thread; an empty queue does not guarantee that the last `write` has finished before `release`. Rust checks only file existence and size. A truncated output can therefore be declared complete.

**Fix:** propagate worker errors, explicitly join the writer, verify that `VideoWriter` opens, then check duration, frame rate, expected frames, and streams with FFprobe. Reserve exhaustive decoding for reference tests or a thorough validation mode.

RIFE benchmarks produce 95 frames for 48 inputs ×2, namely `(48−1)×2+1`, rather than 96. This behavior explains 0.395833 s instead of 0.4 s; the last-interval policy must be decided and tested rather than marking this difference as corruption.

### Q8 — P2: loading errors and stale preview tasks

**Sources:** [loadVideo L821](https://github.com/cia213/cia-app/blob/241f350025e7e8504d0f5eae1f3c46ea283778cd/src/App.svelte#L821), [loadSmoothie L985](https://github.com/cia213/cia-app/blob/241f350025e7e8504d0f5eae1f3c46ea283778cd/src/App.svelte#L985), preview loading L461–502. **Evidence: code inspection.**

Images are protected by request identifiers, but `analyze_video` responses are not. Importing A then B quickly can leave A's metadata with B's path, or a late error from A can clear B. An old preview scan also continues launching its FFmpeg processes even when its result will be discarded.

**Fix:** a selection identifier for analysis and previews; cancellation of obsolete processes on the Rust side; defer optional previews while an engine is working. Processing must retain an immutable copy of its settings, including the auto-render option.

### Q9 — P2: fragile configuration and save messages

**Sources:** [lib.rs L448](https://github.com/cia213/cia-app/blob/241f350025e7e8504d0f5eae1f3c46ea283778cd/src-tauri/src/lib.rs#L448), L1339; [App.svelte L93](https://github.com/cia213/cia-app/blob/241f350025e7e8504d0f5eae1f3c46ea283778cd/src/App.svelte#L93), L157, and L324. **Evidence: code inspection.**

`save_ui_preferences` starts from an empty configuration if loading fails, then writes that configuration: an invalid or temporarily inaccessible file can cause existing paths to be lost. The temporary filename contains only the PID; concurrent writes, particularly from asynchronous installation, are not coordinated. On the UI side, `persistUiPreferences` swallows the error and the caller then displays "settings saved".

**Fix:** do not overwrite a configuration that could not be read; handle repair/migration explicitly. Serialize updates with a lock, use a unique temporary file, then propagate success/failure through to the user message.

### Q10 — P2: special filenames, subtitles, and colors are not covered

**Sources:** [Python glob L182](https://github.com/cia213/cia-app/blob/241f350025e7e8504d0f5eae1f3c46ea283778cd/src-tauri/resources/time_remap.py#L182), subtitle mapping L215, encoder L218. **Evidence: filename with brackets reproduced; other cases inspected in code.**

- `clip [v1]` is treated as a glob pattern. Reproduction shows an existing file and zero matches. An explicit intermediate path addresses both this issue and Q2.
- Subtitles are copied into an MP4 without checking their codec; SubRip/ASS do not undergo a suitable conversion. In slow motion, their timestamps are not stretched.
- The pipeline forces `yuv420p` and BT.709 tags without HDR conversion or a 10-bit policy. Changing tags does not perform tone mapping.

**Fix:** a documented input/output contract, compatible codecs, retimed subtitles or explicit exclusion, and color processing consistent with the source. Dedicated tests are needed before claiming universal support.

### Q11 — P2: the final preview is not extracted from the result

**Source:** [loadOutputPreview L505](https://github.com/cia213/cia-app/blob/241f350025e7e8504d0f5eae1f3c46ea283778cd/src/App.svelte#L505), live preview L513. **Evidence: code inspection.**

`loadOutputPreview` effectively ignores the `path` and `duration` parameters and reuses the source/live image. The live preview uses `tmix` on the source and does not necessarily reproduce the Smoothie recipe, LUT, or color corrections. The result can appear visually unchanged even when the actual file differs.

**Fix:** extract an image from the actual final file after success; present the source preview during processing as an approximation, or use a preview faithful to the recipe. Avoid rendering a complete video to produce a single thumbnail.

## Maintenance and coverage — P3

- **Code concentration:** `App.svelte` ≈ 3918 lines, `lib.rs` ≈ 1950 lines. Separate jobs, configuration, media tools, previews, installation, and updater. Keep navigation and presentation separate from execution.
- **Two diverging scripts:** share a tested orchestration implementation; make the standalone script a thin entry point. Fixes to the bundled script are not automatically present in the standalone script.
- **Clippy:** immediately invoked closure (`lib.rs:264`), `repeat().take()` (`1097`), command with too many arguments (`1539`). These are maintenance diagnostics, not evidence of three rendering errors.
- **Telemetry:** `live-log` carries text without a `jobId`. Regexes depend on the active page and tool format. Prefer typed events `{jobId, phase, frame, total, fps, elapsed, error}` and keep text for diagnostics.
- **Log memory:** Python retains all RIFE stderr (`all_stderr`), Rust collects all lines (`pump_and_collect`), even though the UI limits them to 500. Keep a bounded queue and optionally a diagnostic file per job.
- **CI/distribution:** tests do not cover engines end to end, the frontend, or interruptions. The workflow builds an installer while its payloads are ignored and their preparation is absent from the steps. Separate source verification from reproducible release construction. Update `PORTABILITY-PLAN.md`, which has become historical.

## Performance audit — measurements

| Variant | Local median time | Interpretation |
| --- | ---: | --- |
| RIFE FP32, scale 1 | 14.11 s | Baseline; 48 frames at 1680 × 1050, ×2 |
| RIFE FP16, scale 1 | 11.50 s | ≈ 18.5% less time, ≈ 1.23×; quality not compared |
| RIFE FP16, scale 0.5 | 10.38 s | ≈ 26.4% less time than FP32; changes computation, quality not compared |
| Previews: 8 grayscale scans + 8 PNGs | 5.48 s | Initial cover image excluded |
| Previews: 8 PNGs only | 2.67 s | ≈ 51.3% less; local luminance calculation still to be added |
| Scene detection on the 2 s source | 0.502 s | Full pass currently unused for RIFE computation |
| FFmpeg x264 medium, CRF 18 | 5.03 s | Video only, 120 FPS source; file ≈ 11.89 MB |
| FFmpeg x264 fast, CRF 18 | 4.12 s | ≈ 18.1% less; file ≈ 12.19 MB |
| FFmpeg NVENC p5, CQ 18 | 1.20 s | ≈ 76.1% less; file ≈ 22.93 MB; quality not equivalent to CRF 18 |
| Smoothie x264, reference filters | 2.98 s | 30 FPS blending, intensity 1, AAC audio, and recipe scaling |
| Smoothie NVENC, same filters/audio | 1.98 s | ≈ 33.6% less; visual quality not compared |

The initial `smoothie_baseline`/`smoothie_nvenc` measurements in the main JSON do not use the same scaling: **they are excluded from the speed comparison**. The `*_matched` measurements in `AUDIT-PREUVES.json` retain the same filters and audio settings; only the encoder changes. The figures and limitations of this comparison are included in the supplementary section below.

## Recommended optimizations linked to code quality issues

### F1 — High priority, low effort: remove the unused scene pass

The scene scan decodes the entire video before starting RIFE and stores logs in memory. The timestamps are not used. The RIFE engine already has internal similarity/cut logic, independent of the displayed threshold.

Remove this pass until the options are implemented, or incorporate the requirement into existing decoding/inference. This reduces the time before the first frame without changing the RIFE computation currently performed. For this clip: ≈ 0.5 s per job. The Python timer starts **after** this scan, so the reported time omits its cost.

### F2 — High priority, medium effort: decode each preview only once

The current import can launch 1 FFmpeg for the cover, 8 for luminance, and 8 for PNGs. Calculating luminance from the image already extracted reduces seeks and startups. Cache results (canonical path + size + modification + options), reuse identical images, and cancel old selections.

The measurement prototype drops from 5.48 to 2.67 s for the reduction from 16 to 8 calls, without yet measuring local grayscale calculation. Rendering must take priority over optional previews. Limit FFmpeg concurrency: launching every scan in parallel can increase RAM use and disk/CPU contention.

### F3 — High priority, medium effort: add a validated FP16 profile

RIFE exposes `--fp16`, but the application does not pass it. On the tested RTX 3050, the FP16 variant keeps the same dimensions and frame count and reduces time by 18.5%. Offer FP32/FP16 or a tested automatic choice, with fallback and compatibility validation.

Do not promise visual identity: assess fast motion, fine textures, edges, occlusions, and fades. `scale=0.5` preserves final dimensions but changes motion estimation; make it a distinct profile rather than an invisible setting. The [Practical-RIFE](https://github.com/hzwer/Practical-RIFE/blob/main/README.md) project documents the scale option for high resolutions.

### F4 — High priority, medium effort: make the encoder configurable

RIFE forces `libx264`; so does the local Smoothie recipe. Offer a CPU profile and an NVENC profile, with availability checks, specific parameters, and explicit fallback. Measure time, bitrate, and quality at comparable targets: CRF 18 and CQ 18 do not mean the same thing.

The measured reduction concerns encoding, not the entire interpolation. A GPU encoding gain can also change contention with inference if stages are pipelined. The official [NVIDIA FFmpeg](https://docs.nvidia.com/video-technologies/video-codec-sdk/13.0/ffmpeg-with-nvidia-gpu/index.html) guide describes the NVENC/NVDEC paths; their presence in `-encoders` alone is insufficient, whereas the encoding trials here succeeded.

### F5 — High priority for stability, medium effort: bound RIFE buffers by memory

`inference_video.py:198–199` creates two queues of 500 RGB frames. At 1920 × 1080, 1000 frames represent approximately **5.8 GiB**; at 3840 × 2160, approximately **23.2 GiB**, excluding tensors and other allocations. These are theoretical limits if the queues fill, not observed peaks.

Size the queues according to resolution and memory budget, for example a few frames for 4K, then profile throughput and RAM. Larger queues do not automatically make a GPU faster. Retain a small decoding/writing lead to avoid making it wait; verify throughput after reducing buffers.

### F6 — High potential, high effort: remove unnecessary intermediate encodes

```mermaid
flowchart LR
    A[Source] --> B[RIFE decoding]
    B --> C[CUDA inference]
    C --> D[Intermediate MPEG-4 MP4]
    D --> E[FFmpeg decoding]
    E --> F[Interpolated H.264 MP4]
    F --> G[Smoothie decoding]
    G --> H[Frame blending]
    H --> I[Final encoding]
```

The file named "raw" is a compressed MPEG-4 MP4 (`mp4v`), confirmed by FFprobe. Automatic chaining adds an H.264 encode and then another decode before Smoothie. This consumes CPU/disk resources and applies multiple lossy compressions.

Pragmatic step: keep a single intermediate explicitly chosen for Smoothie and avoid producing an unneeded delivery H.264. Retain a mode that preserves a usable RIFE result if Smoothie fails.

More ambitious step: pass frames to the final encoder or a blending engine through a pipe, or integrate RIFE into a compatible VapourSynth pipeline. A raw RGB pipe at 1080p240 represents approximately **1.49 GB/s** (≈ 5.97 GB/s at 4K240); removing an MP4 without controlling copies can move the bottleneck rather than eliminate it. Plan for backpressure, cancellation, color formats, and audio synchronization. No quantified gain is claimed before a prototype.

### F7 — Medium priority: limit GPU copies and synchronization

Each frame is transferred from CPU to GPU, then each result returns through `.cpu().numpy()`. SSIM checks in Python conditions and these transfers can introduce synchronization points. Profile before modifying; try reused buffers, grouped transfers, and CUDA streams where temporal dependencies are preserved.

`non_blocking=True` is already present, but does not prove actual overlap. The [PyTorch transfer guide](https://docs.pytorch.org/tutorials/intermediate/pinmem_nonblock.html) explains the conditions and possible costs of pinned memory; avoid adding `pin_memory()` to every frame without measuring.

### F8 — Medium priority: persistent RIFE worker and inference_mode

Launching Python and loading CUDA imports and the model for each file penalizes short clips. A persistent worker could amortize these costs, with an explicit state reset between jobs and GPU release when idle. Measure it on a series of clips, not only on one long video.

RIFE already disables gradients and enables cuDNN benchmark: these options should not be presented as new optimizations. Test `torch.inference_mode()` in place of global no-grad in a dedicated path, checking compatibility and results. The [PyTorch inference_mode](https://docs.pytorch.org/docs/stable/generated/torch.autograd.grad_mode.inference_mode.html) documentation describes the removal of additional tracking overhead; the gain here has not been measured.

`torch.compile`/TensorRT remain later possibilities: compilation cost, warp/model compatibility, variable shapes, Windows packaging, and benefits on this GPU must be demonstrated. Avoid rushing an engine migration before eliminating redundant passes.

### F9 — Low to medium priority: reduce interface work

`GlowSlider.svelte:47` schedules animation frames indefinitely after convergence, and `appendLog` rebuilds up to 500 lines for every event. Stop stationary animations, batch log display updates, and publish typed progress at a bounded frequency. Do not truncate useful errors.

Production JS size is already reasonable. These changes improve resource consumption and responsiveness; they are less likely to substantially shorten GPU inference than F1–F6.

## Recommended implementation order

| Batch | Contents | Success condition |
| --- | --- | --- |
| 1 — Reliability | Q1/Q2/Q3, input validation, joined writer, output ownership | Navigation/closure/cancellation without orphaned jobs or overwrites; existing files preserved |
| 2 — Consistency | Video aspect ratio, effective recipe/LUT/model, coherent color and scene options, final preview | Settings observable in the result and tests for advertised formats |
| 3 — Measurable gains | F1/F2/F3/F4/F5, per-stage instrumentation | Total and per-phase times compared; bounded RAM/VRAM; no content regression |
| 4 — Architecture | F6/F7/F8, stores/modules, typed events | Gains confirmed on a varied corpus, short startup, and long videos |

For each batch: short and long clips, 720p/1080p/4K, different frame rates, difficult motion, audio and subtitles, plus cancellation and insufficient disk space. Measure wall time, startup, each stage's FPS, peak CPU/RAM/VRAM, I/O, output size, and integrity. Precision/encoder variants also require visual comparison; PSNR/SSIM alone are insufficient to validate interpolation artifacts.

## Supplementary Smoothie measurements

See `matched_smoothie_measurements` in [AUDIT-PREUVES.json](AUDIT-PREUVES.json) for repetitions, sizes, dimensions, streams, and durations. These measurements retain the recipe's scale filter to isolate the encoder; this does not validate that aspect ratio choice for the product.

Across three repetitions: x264 **2.978 s** versus NVENC **1.978 s**, or **33.6% less time**. Both variants produce 60 frames at 30 FPS, 1920 × 1080, with audio and a container duration of 2.005 s. Sizes: approximately 6.91 MB versus 5.07 MB. This result does not establish equal quality between CRF 18 and CQ 18.

Recipe verification also shows `brightness/saturation/contrast = 1.5` with `color grading.enabled = no`: the values change, but the section remains disabled. The history reproduction returns `isSmoothieProcessing = false` after navigation, although its initial state was true and no stop command was called.
