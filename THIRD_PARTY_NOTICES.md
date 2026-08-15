# Third-party notices

cia app bundles the render payload described below. The optional RIFE
environment is downloaded only after the user selects **INSTALL ENVIRONMENT**.
Every public release must preserve the notices that ship alongside its staged
runtime payload; see `docs/RUNTIME-RELEASE-NOTES.md`.

| Component | Role | Licence / notice status in V1 |
| --- | --- | --- |
| Practical-RIFE | Optional local frame interpolation runtime | MIT licence in the upstream source (hzwer, 2021). Source and RIFE 4.26 weights are fetched after explicit user action; model-weight distribution is not implied by the installer. |
| smoothie-rs | Bundled local render runtime | Bundled from a staged upstream portable runtime. Preserve its included `LICENSE.txt` and all plugin notices; record exact provenance for each release. |
| VapourSynth | Supplied within the bundled Smoothie runtime | Its own LGPL terms and notices are preserved in the staged runtime payload. |
| FFmpeg | Bundled media probing and encoding tools | Current staged build identifies itself as GPLv3. Public distribution requires the corresponding GPL notice and source-availability obligations for that exact build. |
| Python 3.11.9 | Optional RIFE bootstrap | Official Windows installer bundled under the Python Software Foundation License; SHA-256 recorded in runtime release notes. |
| Tauri | Desktop application framework | Apache-2.0 OR MIT according to installed dependency metadata. |
| Svelte and Vite | Frontend framework and build tooling | MIT according to installed dependency metadata. |
| IBM Plex Sans and Mono | Bundled frontend typography | SIL Open Font License 1.1 according to installed Fontsource metadata. |
| Flowframes | Workflow reference | Referenced in ABOUT only; no Flowframes code or binary is distributed. |

Project names and marks remain the property of their respective owners. CIA
RENDER's local ABOUT marks are compact interface identifiers, not a claim of
endorsement.
