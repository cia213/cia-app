# cia app V1 - runtime and distribution notes

## What the NSIS installer contains

- cia app Tauri executable and local Svelte assets;
- IBM Plex font assets included by the frontend build;
- the cia app RIFE orchestration script;
- Start Menu shortcut and a current-user uninstaller.

## What remains external in V1

| Component | Reason | First-launch behaviour |
| --- | --- | --- |
| Python/CUDA environment | Current development environment is about 4.9 Gio and is not reproducibly packaged. | Select a Python executable. |
| Practical-RIFE and `flownet.pkl` | Code and model distribution must be versioned and audited independently. | Select the RIFE folder and model. |
| FFmpeg / FFprobe | Licence obligations depend on the chosen build. | Select both executables. |
| smoothie-rs runtime | Bundle contains additional software requiring a release-level licence audit. | Select root/executable; recipe is detected when present. |
| LUTs and media | User content and provenance-specific assets. | Optional local paths only. |

This is an intentional V1 boundary. The installer is functional on a clean
machine: it starts, displays setup, validates paths, persists configuration,
and enables each render operation only when its real local runtime is ready.
It never claims that missing render engines are installed.

## Configuration contract

The configuration is written atomically in the per-user app-data directory:

```json
{
  "schemaVersion": 1,
  "rife": {
    "pythonExecutable": null,
    "script": null,
    "directory": null,
    "modelFile": null
  },
  "smoothie": {
    "root": null,
    "executable": null,
    "recipe": null,
    "lutFile": null
  },
  "mediaTools": {
    "ffmpeg": null,
    "ffprobe": null
  }
}
```

`script: null` selects cia app's bundled RIFE orchestration script. The
optional override exists for advanced local workflows. Runtime paths are
validated at startup; only explicit saved paths are used at render time.

## Public repository rules

Do commit source, manifests, lockfiles, documentation, notices and CI.

Do not commit or release Python environments, model weights, media, LUTs,
personal configuration, build outputs, logs, external runtime binaries or
unreviewed plugins. Git LFS is not a substitute for distribution rights.

The GitHub Actions workflow builds the UI and Rust tests without requiring a
personal render runtime. Publication of a release remains a separate,
explicitly authorised action.
