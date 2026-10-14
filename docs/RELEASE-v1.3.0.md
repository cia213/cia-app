# cia render v1.3.0

This release fixes the environment, timing and operation-state problems identified
in the [v1.2.7 reliability audit](https://github.com/cia213/cia-app/blob/main/docs/AUDIT-v1.3.md).

- **Verified, repairable RIFE setup.** A pinned Python 3.11.9 / PyTorch 2.10.0
  CUDA 12.8 / Practical-RIFE 4.26 profile replaces the older Torch environment.
  Readiness requires verified dependencies, trusted code and model files, CUDA
  execution and model inference. An incomplete installation offers **REPAIR
  ENVIRONMENT** and cannot replace a working generation until verification passes.
- **Clear hardware requirements and download progress.** Setup checks the GPU,
  driver and disk space first, reports unsupported configurations, shows downloaded
  bytes, retries transient failures and supports cancellation. RENDER remains
  usable without the optional interpolation environment.
- **Audio tracks and media timing.** Interpolation and render finishing preserve
  supported audio tracks, channels, language/title metadata and default/forced
  playback flags.
  Video timestamps and audio offsets share a normalized timeline; longer audio
  does not extend a shorter video. Output validation checks actual decoded frame
  counts when metadata is insufficient.
- **Rubber Band speed control.** Render speed remains adjustable from **0.50** to
  **2.00**, with **1.07** by default. Rubber Band preserves voice pitch. Details,
  progress and output duration use the selected speed, including automatic render
  after interpolation.
- **Measured tail recovery.** A short early video end can hold the last valid
  image. The completion screen reports how much was held. Recovery is limited
  to approximately 2% of planned frames, rounded up, and at most one second;
  larger losses and videos with no decoded image fail.
- **Consistent operations and lasting diagnostics.** Setup, renders and updates
  cannot overlap. Pause uses the backend's acknowledgement. Closing the app or
  cancelling stops owned child processes; bounded checks and inactivity detection
  prevent an indefinitely stalled job. Errors and diagnostic excerpts survive
  navigation and restart, with contextual repair and configuration recovery.
- **Windows packaging and interface.** The installer includes the WebView2 offline
  installer and hash-verified render tools. The window can be resized down to
  640 × 440, keyboard focus remains visible, and LUT selection is available in
  render details. The removed About runtime-paths button stays removed. Application
  and release text remain in English.

Actual CUDA/media checks passed on an **RTX 3050 Laptop GPU**, including 18
FP32/software-encoder cases and six FP16/NVENC cases. These cover 360 FPS input,
multiple audio tracks, pitch-preserving slow motion, offsets, subtitles, fractional
rates, geometry and Unicode filenames. Other GPU families and a clean second
Windows machine have not been physically qualified.

See [validation and remaining limits](https://github.com/cia213/cia-app/blob/main/docs/VALIDATION-v1.3.0.md)
and [installation requirements](https://github.com/cia213/cia-app/blob/main/docs/INSTALL.md).
Windows x64; RIFE is optional. The updater uses the existing public signing key.
