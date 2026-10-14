# Third-party notices

cia render bundles the render payload described below. The optional RIFE
environment is downloaded only after the user selects **INSTALL ENVIRONMENT**.
Every public release must preserve the notices that ship alongside its staged
runtime payload; see `docs/RUNTIME-RELEASE-NOTES.md`.

| Component | Role | Licence / notice status in v1.3.0 |
| --- | --- | --- |
| Practical-RIFE | Optional local frame interpolation runtime | MIT licence in the upstream source (hzwer, 2021). Source and RIFE 4.26 weights are fetched after explicit user action; model-weight distribution is not implied by the installer. |
| smoothie-rs | Bundled local render runtime | GPL-3.0, portable release `Nightly_2025.11.30_14-36`, commit `3da73de9df0c991801c4f8474b320445fb25a208`. Its GPL text, embedded Python licence and all archive notices are retained; provenance and file hashes are in `resources/RELEASE-PAYLOAD.json`. |
| VapourSynth | Supplied within the bundled Smoothie runtime | Its own LGPL terms and notices are preserved in the staged runtime payload. |
| FFmpeg / FFprobe 8.1.1 essentials | Bundled media probing and encoding tools | Gyan.dev GPLv3 build. The verified archive's GPL text and exact build configuration/hashes are retained. Matching source and source-availability information accompany the release. Linked libraries, including Rubber Band, retain their own terms. |
| Python 3.11.9 x64 | Optional RIFE bootstrap | Python Software Foundation License. The bootstrap downloads the hash-verified official NuGet package and retains `LICENSE.txt`; it does not distribute the previous Python Windows installer. |
| PyTorch 2.10.0+cu128 / torchvision 0.25.0+cu128 | Optional CUDA tensor/runtime dependencies | Upstream BSD-style terms and included third-party notices apply. Windows wheels are downloaded after the interpolation setup action; CUDA components retain NVIDIA's applicable terms. |
| NumPy 1.26.4 | Optional RIFE array dependency | BSD-3-Clause and bundled third-party notices. The wheel remains within the downloaded private environment. |
| Other optional Python packages | Pinned dependencies of the RIFE profile | filelock 4.1.1, fsspec 2026.9.0, Jinja2 3.1.6, MarkupSafe 3.0.4, mpmath 1.3.0, networkx 3.6.1, Pillow 12.3.0, pip 26.2.1, setuptools 83.0.0, sympy 1.14.0, and typing_extensions 4.16.0 retain the licence texts and notices supplied in their verified wheels. Exact artifact URLs and SHA-256 values are recorded in the bundled profile and lockfile. |
| Microsoft Edge WebView2 | Desktop web runtime | The NSIS installer includes Microsoft's offline Evergreen installer. Microsoft's runtime redistribution and licence terms apply; WebView2 is not MIT-licensed cia render source. |
| Tauri | Desktop application framework | Apache-2.0 OR MIT according to installed dependency metadata. |
| Svelte and Vite | Frontend framework and build tooling | MIT according to installed dependency metadata. |
| IBM Plex Sans and Mono | Bundled frontend typography | SIL Open Font License 1.1 according to installed Fontsource metadata. |
| Flowframes | Workflow reference | Referenced in ABOUT only; no Flowframes code or binary is distributed. |

Project names and marks remain the property of their respective owners. CIA
RENDER's local ABOUT marks are compact interface identifiers, not a claim of
endorsement.

Original third-party licence texts remain unchanged; these notices do not replace
them.

The optional profile is defined in
`src-tauri/resources/bootstrap/rife-profile.json` and
`src-tauri/resources/bootstrap/rife-requirements.lock`. Its project archive and
RIFE 4.26 model are pinned by hash. These downloaded environments and model weights
are not Git files or public cia render release assets. Inclusion of the setup
scripts does not grant additional rights to redistribute upstream models or
CUDA packages.
