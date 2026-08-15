# cia app

cia app is a local Windows desktop workflow for two video operations:

- **RENDER** — smoothie-rs frame blending and render finishing.
- **INTERPOLATION** — optional RIFE frame multiplication.

Your media stays on the computer. cia app does not upload source videos or
silently fetch render engines.

## Install and first launch

The Windows installer contains the cia app application, its local UI,
fonts, smoothie-rs, its required plugins, and FFmpeg/FFprobe. A fresh
installation therefore opens directly into **RENDER** with no runtime-path
wizard and no dependency on a machine-wide `PATH`.

**INTERPOLATION** is intentionally optional: selecting **INSTALL ENVIRONMENT**
downloads an isolated Python 3.11 / CUDA PyTorch / Practical-RIFE 4.26
environment into cia app's per-user app-data folder. This is a large
download, requires a CUDA-capable NVIDIA GPU, and is never downloaded by the
installer or by merely launching the app.

The **RUNTIME** control is an advanced repair panel only. It can point CIA
RENDER to an existing RIFE installation if the optional installer is not the
right fit. It is not part of the normal first-run flow.

The bundled render tools are resolved from the installed application, and the
RIFE installer saves only its per-user app-data paths. cia app never guesses
a runtime path during an active render.

Configuration is stored per user in the cia app app-data directory as
`config.json`. It contains local paths and UI preferences; it is not part of a
Git checkout, installer, or release asset.

## Output names

The Rust backend owns output paths and validates each file after a successful
process. The UI never reconstructs a filename.

| Operation | Example |
| --- | --- |
| RIFE at 360 FPS | `clip-360fps.mp4` |
| Smoothie at 30 FPS | `clip_render30fps.mp4` |
| Auto-chain RIFE 360 → Smoothie 30 | `clip-360fps_render30fps.mp4` |

Existing destinations are never overwritten silently.
When an output already exists, cia app preserves it and selects the first
available numbered variant instead: `clip_render30fps (1).mp4`, then `(2)`,
and so on. The output name is reserved by Rust before the render starts.

## Build from source

Prerequisites: current Node.js, Rust stable, and Windows build tools.

```powershell
npm ci
npm run tauri dev
```

Create an NSIS installer:

```powershell
npm run tauri build
```

The release build also needs the ignored release payload in
`src-tauri/resources/runtime/` (Smoothie and FFmpeg/FFprobe) plus the official
Python installer in `src-tauri/resources/bootstrap/`. These files are excluded
from Git on purpose; see the release notes below before redistributing them.

## Distribution policy

- RIFE's Python/CUDA environment and model are downloaded only after the user
  explicitly requests INTERPOLATION; they are not committed to Git or included
  in the NSIS installer.
- The bundled FFmpeg build reports GPLv3. A public installer must ship its
  matching notices and source-availability information with the exact runtime
  payload used for that release.
- Smoothie's release payload includes its upstream licence files and plugin
  notices. Its exact release provenance must be recorded for every public build.
- Test videos, LUTs, local configuration, build outputs, local runtime binaries
  and model weights are excluded from Git. Release staging is a controlled,
  reviewed exception rather than a source-tree dependency.

See [runtime release notes](docs/RUNTIME-RELEASE-NOTES.md) for the exact V1
payload and release checklist.

## Licence and notices

cia app source code is MIT licensed. Third-party software keeps its own
licence; see [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md). Nothing in this
repository grants redistribution rights for an external runtime or model.
