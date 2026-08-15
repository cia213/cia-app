# CIA RENDER

CIA RENDER is a local Windows desktop workflow for two video operations:

- **INTERPOLATION** — RIFE frame interpolation.
- **SMOOTHIE** — smoothie-rs frame blending and render finishing.

Your media stays on the computer. CIA RENDER does not upload source videos or
silently fetch render engines.

## Install and first launch

The Windows installer contains the CIA RENDER application, its local UI,
fonts, and the RIFE orchestration script. It intentionally does **not** bundle
the multi-gigabyte Python/CUDA environment, RIFE model weights, smoothie-rs,
or FFmpeg.

On first launch, Runtime Setup asks for explicit local paths to:

1. a Python runtime and the Practical-RIFE folder with `flownet.pkl`;
2. `ffmpeg.exe` and `ffprobe.exe`;
3. the smoothie-rs runtime folder and executable.

The setup screen can detect common local installations, but detection is only
a convenience. Once saved, CIA RENDER uses only the paths in its configuration
and never falls back to `PATH` during a render.

Configuration is stored per user in the CIA RENDER app-data directory as
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

The build does not require local video runtimes. A render does.

## Distribution policy

- The current Python/CUDA environment and RIFE model are external runtime
  dependencies until their reproducible packaging and model distribution rights
  are separately audited.
- smoothie-rs and its bundled ecosystem remain external until their release and
  licence inventory are approved for redistribution.
- FFmpeg remains external: its licence obligations depend on the selected build.
- Test videos, LUTs, local configuration, build outputs, runtime binaries and
  model weights are excluded from Git and releases.

See [Runtime and distribution notes](docs/PORTABILITY-PLAN.md) for the exact
V1 boundaries.

## Licence and notices

CIA RENDER source code is MIT licensed. Third-party software keeps its own
licence; see [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md). Nothing in this
repository grants redistribution rights for an external runtime or model.
