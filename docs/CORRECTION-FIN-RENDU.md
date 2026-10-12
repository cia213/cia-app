# Holding the last frame at the end of a render

An NVIDIA capture nominally recorded at 360 FPS reproduced the duration failure:
a 64.8533 s source produced a 64.52 s Smoothie render at 25 FPS. FFprobe decoded
all 23,316 source frames; packet cadence and durations are variable. The render's
time deficit therefore does not establish that the final second of the source
file is corrupted.

The render now calculates the target frame count from source duration, speed,
and requested FPS. After the speed change, FFmpeg normalizes the cadence, holds
the last available frame with `tpad=stop_mode=clone`, and limits output to the
target frame count. Duration, geometry, audio, and cadence checks remain active;
the expected frame count is also verified.

Audio is bounded independently to the same duration. It continues during the
held frame and receives silence at the end if necessary. Rubber Band remains
the speed filter, with `pitch=1.0`. Short tracks no longer cause the entire
output to be cut short by `-shortest`.

## Local verification

| Full-capture test | Video | Audio | Frames at 25 FPS |
| --- | --- | --- | --- |
| Before fix, ×1.00 | 64.52 s | 64.856 s | 1,613 |
| With fix, ×1.00 | 64.84 s | 64.84 s | 1,621 |
| With fix, ×1.07 | 60.60 s | 60.60 s | 1,515 |

All three tests use Smoothie and the bundled FFmpeg with NVENC. Corrected
durations are rounded to the nearest frame; at 25 FPS, the difference from the
theoretical duration remains below 20 ms. The original capture's SHA-256 is
unchanged.

The `smoothie_encoder_clones_missing_tail_without_cutting_audio` test uses
lossless x264 to verify that ten added frames are identical to the final valid
frame, that 35 frames at 25 FPS produce 1.4 s, that audio is retained to the end,
and that the silent-input case also completes. The complete Smoothie test also
covers colour adjustment, LUT, aspect ratio, ×1.07/×0.80 speed, audio pitch,
no-clobber output, and cleanup. All 16 ordinary Rust tests and Clippy pass.

This fix handles a video stream that ends too early when at least one valid
frame is available. A file with no decodable frame or an explicit engine failure
still produces an error; missing original frames are not reconstructed.
