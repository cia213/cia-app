# cia render runtime release notes

Runtime payloads are excluded from Git. A source checkout does not silently
publish an unverified binary or model. v1.3.0 stages the following Windows x64
payload from pinned, hash-verified upstream archives.

| Payload | Version / provenance | Release staging |
| --- | --- | --- |
| smoothie-rs and `vs-plugins` | `Nightly_2025.11.30_14-36`, source commit `3da73de9df0c991801c4f8474b320445fb25a208` | `src-tauri/target/release-kit-v1.3.0/runtime/smoothie/` |
| FFmpeg / FFprobe | `8.1.1-essentials_build-www.gyan.dev`, Gyan.dev GPLv3 build | `src-tauri/target/release-kit-v1.3.0/runtime/ffmpeg/` |
| VapourSynth | R70, included in the Smoothie payload | Notices and libraries retained under the Smoothie staging directory |
| App Python helpers | `time_remap.py`, `rife_worker.py`, `runtime_health.py` | Tracked resources mapped into the installer |
| Bootstrap and pinned RIFE profile | Two PowerShell scripts, profile JSON and requirements lock | Tracked `src-tauri/resources/bootstrap/` resources |
| Microsoft WebView2 | Evergreen offline installer | Embedded by the Tauri NSIS build |

The staging copy contains no personal LUT, model, source video, user configuration
or developer-home path. Recipe/default settings are neutral; the application
applies each job's selected settings. The optional RIFE environment is downloaded
only after **INSTALL ENVIRONMENT**, rather than bundled into the installer.

## Prepare and build

Prerequisites: Windows x64, Node.js 24, Rust stable, Python 3.11+ and Windows C++
build tools. Python's standard library is sufficient for release preparation.

```powershell
npm ci
python scripts/prepare-release.py
npm run tauri build -- --config src-tauri/target/release-kit-v1.3.0/build.json
```

`prepare-release.py` verifies the Smoothie archive and FFmpeg archive/executable
hashes, preserves upstream notices, creates a clean payload manifest, and writes
the staged Tauri resource configuration. Build tools may download Microsoft's
WebView2 offline installer at build time. User installation does not need to
download it separately.

After extracting the completed NSIS installer with 7-Zip, use
`scripts/verify-release.py --help` for the `--kit`, `--unpacked` and `--compiled`
arguments. Verify the staged and extracted resources against the manifest,
tracked scripts and notices. This checks packaging; it does not replace native
application tests or clean-machine qualification.

Run the source checks and native tests documented in
[VALIDATION-v1.3.0.md](VALIDATION-v1.3.0.md). A source-only CI build overrides the
resource list with tracked scripts; it is not the final release installer.

## Signing and publication

The updater public key stays unchanged. `scripts/build-update.mjs` requires the
matching private key in the local `.tauri` key location and never puts it in Git.
It signs the installer and writes `dist/latest.json` using
`docs/RELEASE-v1.3.0.md`. The public installer asset name is
`cia.render_1.3.0_x64-setup.exe`. Run the helper with the path to the verified
installer, then confirm the manifest URL, version, signature and SHA-256 match
the uploaded release assets. An updater signature is separate from a Windows
Authenticode publisher certificate.
`node scripts/verify-update.mjs <installer> <latest.json>` verifies the installer
and trusted-comment signatures against the configured public key, as well as
the manifest version and exact asset URL.

Publish only the verified installer, updater manifest/signature, payload manifest,
checksums, applicable source archives/notices and sanitized validation evidence.
Private environments, model weights, media fixtures, raw personal paths and
credentials must remain excluded. Pin the GitHub tag to the checked source commit
and verify its Windows CI result.

## Downloaded only after INSTALL ENVIRONMENT

The exact profile is
`rife4.26-torch2.10-cu128-py311-win64-v1`. It includes:

- Python 3.11.9 x64 from the official NuGet package, SHA-256
  `9283876d58c017e0e846f95b490da3bca0fc0a6ee1134b2870677cfb7eec3c67`.
  The package's PSF `tools/LICENSE.txt` is retained.
- Torch 2.10.0+cu128, torchvision 0.25.0+cu128 and the complete pinned Python
  dependency set. `rife-profile.json` records every wheel URL and SHA-256;
  `rife-requirements.lock` requires those hashes.
- Practical-RIFE source commit `17d8c7a1005b37f4c97bfee04e316aaec7fdc536`, with
  its licence and trusted project-file hashes.
- The RIFE 4.26 archive, SHA-256
  `c2452dd2b244947d4be580156bbead60d6b72af5736860f7d6b3f99648c9c4cc`, including
  `flownet.pkl` SHA-256
  `45c7f74156704769dc9f85cfcaf8552e1e926f9399dcfa3a553dee88fac6f53f`.

The managed environment is placed in LocalAppData and promoted atomically after
full integrity, CUDA and model verification. The bootstrap checks GPU capability,
driver 570.65+, 2 GiB VRAM and 14 GiB free disk space before downloading. See
[INSTALL.md](INSTALL.md) for repair and compatibility details.

These environments and models are neither Git files nor public release assets.
Upstream licence texts are preserved. Refer to
[THIRD_PARTY_NOTICES.md](../THIRD_PARTY_NOTICES.md) for the distribution boundaries.
