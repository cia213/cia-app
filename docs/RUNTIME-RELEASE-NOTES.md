# cia app runtime release notes

This document is the release gate for the Windows installer. Runtime payloads
are intentionally ignored by Git, so a source checkout cannot silently publish
an unreviewed binary or model.

## Included in the installer

| Payload | Location in release staging | Purpose | Release requirement |
| --- | --- | --- | --- |
| smoothie-rs portable runtime and `vs-plugins` | `src-tauri/resources/runtime/smoothie/` | **RENDER** | Preserve every upstream licence/notice file and record the exact upstream release used. |
| FFmpeg / FFprobe | `src-tauri/resources/runtime/ffmpeg/` | Media analysis and local rendering | The current supplied build reports GPLv3. Ship its licence, exact build provenance, and matching source-availability information with the public installer. |
| Python 3.11.9 Windows installer | `src-tauri/resources/bootstrap/python-3.11.9-amd64.exe` | Optional RIFE bootstrap only | Keep the official installer intact and retain its PSF licence. The reviewed SHA-256 is `5EE42C4EEE1E6B4464BB23722F90B45303F79442DF63083F05322F1785F5FDDE`. |
| `bootstrap-rife.ps1` | `src-tauri/resources/bootstrap/` | Optional RIFE setup | Reviewed source; it installs only below cia app's per-user app-data directory. |

The staging copy deliberately removes the user's `colorcia.cube` LUT and
personal settings. Its bundled `recipe.ini` uses no absolute path, no LUT, no
colour adjustment and a 1.0 audio/video timescale.

## Downloaded only after INSTALL ENVIRONMENT

The optional INTERPOLATION setup installs Python into cia app app data,
creates a virtual environment, then downloads CUDA PyTorch 2.5.1 / torchvision
0.20.1, Practical-RIFE commit
`17d8c7a1005b37f4c97bfee04e316aaec7fdc536`, and the official RIFE 4.26 model.
It validates the expected `flownet.pkl` SHA-256 before reporting readiness.

This runtime and its model are not installer payloads, Git content or release
assets. The command fails visibly if a CUDA-capable NVIDIA GPU is unavailable.

## Before publishing an installer

1. Record the source URL, version, SHA-256 and licence for every staged binary.
2. Review all notices copied with Smoothie and its plugins; do not strip them.
3. For the selected FFmpeg build, include GPLv3 text and matching source or a
   valid written offer/source location as required by that build's licence.
4. Verify the installer on a clean Windows user profile: **RENDER** launches and
   renders without configuration; **INTERPOLATION** remains opt-in.
5. Verify no test video, LUT, local path, model or app-data configuration is in
   the installer or repository.
