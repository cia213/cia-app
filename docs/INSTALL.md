# Install and first launch

The Windows installer contains the cia render application, its local UI,
fonts, smoothie-rs, its required plugins, and FFmpeg/FFprobe. A fresh
installation therefore opens directly into **RENDER** with no runtime-path
wizard and no dependency on a machine-wide `PATH`.

**INTERPOLATION** is intentionally optional: selecting **INSTALL ENVIRONMENT**
downloads an isolated Python 3.11 / CUDA PyTorch / Practical-RIFE 4.26
environment into cia render's per-user app-data folder. This is a large
download, requires a CUDA-capable NVIDIA GPU, and is never downloaded by the
installer or by merely launching the app.

The fixed bootstrap extracts Python 3.11.9 x64 from the official NuGet package
after verifying its SHA-256. Its private executable and virtual environment do
not depend on a Python installation registered on the machine. Other Python
installations, registry entries, file associations, and the user `PATH` are not
changed. An incomplete private installation is replaced only after the new copy
has been verified. The package's PSF licence is retained.

v1.2.6 includes the fixed bootstrap. Versions up to v1.2.5 used the official
Python Windows installer, which could fail to create the private directory when
Python was already installed. Update to v1.2.6 and retry INSTALL ENVIRONMENT.
See the [RIFE installation fix](CORRECTION-INSTALLATION-RIFE.md).

When installation fails, the full diagnostic stays visible on the interpolation
page. **COPY LOGS** provides the details needed to identify the failed stage and
retry installation.

The **ABOUT → ADVANCED RUNTIME PATHS** repair panel is an advanced settings tool. It can point cia render
to an existing RIFE installation if the optional installer is not the
right fit. It is not part of the normal first-run flow.

The bundled render tools are resolved from the installed application, and the
RIFE installer saves only its per-user app-data paths. cia render never guesses
a runtime path during an active render.

Configuration is stored per user in the cia render app-data directory as
`config.json`. It contains local paths and UI preferences; it is not part of a
Git checkout, installer, or release asset.

## Output names

The Rust backend owns output paths and validates each file after a successful
process. The UI never reconstructs a filename.

| Operation | Example |
| --- | --- |
| RIFE at 360 FPS | `clip-360fps.mp4` |
| Smoothie at 30 FPS | `clip_render30fps.mp4` |
| Auto-chain RIFE 360 to Smoothie 30 | `clip-360fps_render30fps.mp4` |

Existing destinations are never overwritten silently.
When an output already exists, cia render preserves it and selects the first
available numbered variant instead: `clip_render30fps (1).mp4`, then `(2)`,
and so on. The output name is reserved by Rust before the render starts.
