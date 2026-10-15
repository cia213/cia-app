# Install and first launch

Download the Windows x64 installer from the
[releases page](https://github.com/cia213/cia-app/releases). It contains cia render,
its UI and fonts, smoothie-rs and plugins, FFmpeg/FFprobe, and the WebView2 offline
installer. **RENDER** uses the bundled tools without Python, RIFE, a runtime-path
wizard, or a machine-wide `PATH` dependency.

## Optional interpolation environment

**INTERPOLATION** downloads its private environment only when you select
**INSTALL ENVIRONMENT**. The v1.3.0 profile is Practical-RIFE 4.26, Python 3.11.9
x64, PyTorch 2.10.0 with CUDA 12.8, and torchvision 0.25.0. The download is several
gigabytes; the Torch wheel alone is approximately 2.9 GB.

Installation checks these requirements before downloading:

- Windows x64 and a supported CUDA-capable NVIDIA GPU.
- NVIDIA driver 570.65 or newer, at least 2 GiB GPU memory, and compute capability
  7.5, 8.0, 8.6, 8.9, 9.0, 10.0, or 12.0.
- At least 14 GiB free space on the managed-runtime drive. Previous generations
  and the verified download cache also occupy space.

This compatibility list is the installer contract. It is not a claim that each
GPU family has been physically tested; see [validation](VALIDATION-v1.3.0.md).
Unsupported hardware leaves **RENDER** available. FP32 and software H.264 remain
the interpolation defaults. FP16 and NVIDIA H.264 encoding are explicit options;
available VRAM and encoder limits also depend on the video's dimensions.

The private Python installation is extracted from a hash-verified official NuGet
package. System Python, registry registrations, file associations and the user
`PATH` are preserved. Python runs with an isolated import path so unrelated
`PYTHONHOME`, `PYTHONPATH`, user packages and pip settings cannot select the runtime.

The environment lives under
`%LOCALAPPDATA%\io.github.cia213.ciaapp\runtimes\rife`. Each installation creates
a separate generation. The active pointer changes only after dependency and
code/model integrity checks, a CUDA computation and actual RIFE model inference
pass. The previous active generation remains available when a candidate fails.
A full health check can take tens of seconds on the first check.

## Repair and diagnostics

Once interpolation is available, the page shows your clip workspace without an
**ENVIRONMENT READY** label or routine **RECHECK** / **REPAIR ENVIRONMENT** buttons.
Environment verification still runs in the background. First-time setup remains
available when the optional environment is missing; recovery actions appear only
when a problem needs attention. Older environments using Torch 2.5.1 are not
accepted as the v1.3.0 profile; repair creates the new verified environment.
Downloads display byte progress, retry transient failures, and reuse only verified
cached artifacts. **CANCEL SETUP** stops installation without promoting its
unfinished generation.

An actual operation failure opens a popup with its error and reporting actions.
Cancelling an operation does not open a bug popup. Failure messages and bounded
logs remain available after navigation and restart. **COPY LOGS** masks common
credentials but may include local paths. **COPY REPORT** also masks local paths
and includes the application version and operation details.

The optional GitHub report action opens a prefilled issue draft in your browser.
Review the content and submit it on GitHub to send it to the developer; opening
the draft does not submit a report. A long draft may contain only an excerpt;
use **COPY REPORT** for the complete report. There is no automatic telemetry or
background transmission of diagnostics. Native process logs stay under the
application's local app-data `diagnostics` directory.

**RESET RIFE CONFIGURATION** clears RIFE path overrides so the managed environment
can be selected again. If configuration itself is invalid, **RESET CONFIGURATION**
backs up the old file and restores bundled-tool defaults. These actions preserve
your source videos. The About page has no advanced-runtime-paths button.

Configuration and UI preferences are stored per user in `config.json` under the
application's app-data directory. They are excluded from Git, installers and
release assets. Runtime paths and preferences are validated before saving.

## Render speed and short video tails

Render **SPEED** defaults to **1.07** and can be set from **0.50** to **2.00**.
**1.00** keeps the original playback speed. Audio uses **Rubber Band** with pitch
fixed at 1, preserving voice pitch while changing duration. The selected speed is
shown in render details and used consistently for output duration and progress.

If Smoothie reaches a short undecodable end of the video, the render may hold the
last valid image to reach the planned duration. The completion screen reports
the held frames and duration. Recovery is limited to approximately 2% of planned
frames, rounded up to a frame, with an upper bound of one second; an empty or much
shorter video fails instead. A held image does not reconstruct lost motion.

## Output names

The Rust backend reserves output paths and validates each result before publishing
it. The UI uses the filename returned by the backend.

| Operation | Example |
| --- | --- |
| RIFE at 360 FPS | `clip-360fps.mp4` |
| Smoothie at 30 FPS | `clip_render30fps.mp4` |
| Auto-chain RIFE 360 to Smoothie 30 | `clip-360fps_render30fps.mp4` |

Existing destinations are preserved. If a name is occupied, the next numbered
variant is selected: `clip_render30fps (1).mp4`, then `(2)`, and so on.

The delivery profile is SDR H.264. HDR requires a separate tone-mapping workflow.
The interpolation pipeline retains supported text subtitles; bitmap subtitles
are rejected. Source audio tracks, channels, language/title metadata and timing
are checked in the supported media path.
