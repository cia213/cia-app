# cia render runtime release notes

This document describes the requirements for the Windows installer. Runtime
payloads are excluded from Git, so a source checkout cannot silently publish
an unverified binary or model.

## Contents of the next installer

| Payload | Location in release staging | Purpose | Release requirement |
| --- | --- | --- | --- |
| smoothie-rs portable runtime and `vs-plugins` | `src-tauri/target/release-kit-v<VERSION>/runtime/smoothie/` | **RENDER** | Preserve every upstream licence/notice file and record the exact upstream release used. |
| FFmpeg / FFprobe | `src-tauri/target/release-kit-v<VERSION>/runtime/ffmpeg/` | Media analysis and encoding | The selected build reports GPLv3. Include its licence, exact provenance, and matching source-availability information with the public installer. |
| `bootstrap-rife.ps1` and `bootstrap-python.ps1` | `src-tauri/resources/bootstrap/` | Optional RIFE setup | Verified scripts restricted to cia render's per-user app-data directory. |

The staging copy contains no personal LUT or user configuration. Its neutral
recipe uses no absolute path, LUT, or colour adjustment, and its audio/video
timescale is 1.0. The application then applies the settings selected for each job.

v1.2.4 and v1.2.5 bundle the previous official Python 3.11.9 Windows installer,
SHA-256 `5EE42C4EEE1E6B4464BB23722F90B45303F79442DF63083F05322F1785F5FDDE`.
The current fix replaces it with extraction of a private Python installation
after the explicit RIFE installation action.

## Downloaded only after INSTALL ENVIRONMENT

- Python 3.11.9 x64: [official NuGet package](https://api.nuget.org/v3-flatcontainer/python/3.11.9/python.3.11.9.nupkg), SHA-256 `9283876D58C017E0E846F95B490DA3BCA0FC0A6EE1134B2870677CFB7EEC3C67`. The PSF licence in `tools/LICENSE.txt` is retained.
- CUDA PyTorch 2.5.1 and torchvision 0.20.1 in a private virtual environment.
- Practical-RIFE, commit `17d8c7a1005b37f4c97bfee04e316aaec7fdc536`.
- Official RIFE 4.26 model: `flownet.pkl` SHA-256 is checked before reporting readiness.

These environments and models are neither Git files nor public release assets.
Installation stops before any download if no CUDA-capable NVIDIA GPU is detected.

## Before publishing an installer

1. Record the source URL, version, SHA-256, and licence for every staged binary.
2. Preserve the notices supplied with Smoothie and its plugins.
3. For FFmpeg, include the GPLv3 text and matching source or source-availability information required for that exact build.
4. Verify rendering without configuration and optional interpolation on a clean Windows user profile. Clearly state when this check has not been performed.
5. Verify that no test video, LUT, model, personal configuration, or local path is in the installer or repository.
