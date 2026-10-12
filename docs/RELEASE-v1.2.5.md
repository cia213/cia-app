# cia render v1.2.5

This version fixes render progress after interpolation and duration failures at
the end of NVIDIA captures.

- After interpolation, progress shows the FPS of the file actually being rendered, for example **360 → 25 FPS**. Automatic chaining uses the correct values too.
- **SPEED** appears in render details, adjustable from **×0.50 to ×2.00**, with **×1.07 by default**. Choose ×1.00 to preserve the original speed.
- Audio acceleration uses **Rubber Band while preserving voice pitch**. Duration, progress, and preview follow the selected speed even if settings are changed during processing.
- If Smoothie's video stream ends too early, output holds **the last valid frame until the expected duration**. Audio continues and is no longer cut by the premature end of video.
- Frame count is bounded to the requested cadence and checked before publishing the result. Other duration, aspect ratio, audio, and integrity checks remain active.

The reported NVIDIA case was reproduced: the **64.52 s** render becomes
**64.84 s at 25 FPS**, with eight held frames at the end. At ×1.07, video and audio
last **60.60 s**. Duration is rounded to the nearest frame.
[Details and verification](https://github.com/cia213/cia-app/blob/main/docs/CORRECTION-FIN-RENDU.md).

Holding a frame does not reconstruct lost frames. A file with no decodable frame
or an explicit engine failure still reports an error.

Windows x64; Smoothie and media tools are included. RIFE remains optional.
Update signatures use the same public key as previous versions.
