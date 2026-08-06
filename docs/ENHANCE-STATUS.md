# ENHANCE status

Date: 2026-08-04  
Scope: evidence archive only; no ENHANCE page or Topaz runtime is bundled.

## Runtime discovery

Topaz Video AI's local ffmpeg was found at:

```text
C:\Program Files\Topaz Labs LLC\Topaz Video AI\ffmpeg.exe
```

`ffmpeg -hide_banner -filters` reports:

```text
T.. tvai_up V->V Apply Topaz Video AI upscale models
```

`ffmpeg -hide_banner -h filter=tvai_up` accepts the required option names:

```text
model, scale, w, h, device, instances, vram,
estimate, preblur, noise, details, halo, blur, compression, blend
```

The latest requested families are present in the local Topaz model directory:

```text
C:\ProgramData\Topaz Labs LLC\Topaz Video AI\models\ddv-3.json
C:\ProgramData\Topaz Labs LLC\Topaz Video AI\models\ddv-v3-*.tz
C:\ProgramData\Topaz Labs LLC\Topaz Video AI\models\prob-4.json
C:\ProgramData\Topaz Labs LLC\Topaz Video AI\models\prob-v4-*.tz
```

The mapping used in the bounded test was therefore `ddv-3` for
deinterlace-to-FHD and `prob-4` for upscale-to-4K. No model ID was invented.

## Bounded D1 results

Input: `C:\Users\cia\time-remap-app\test_clip_topaz_source_2s.mp4`.

The full user presets and the minimal `tvai_up` parameter sets were both
tested. All four commands failed before encoding:

```text
DDV3_MAPPED_EXIT=-22    Model not found: ddv-3
DDV3_MINIMAL_EXIT=-22   Model not found: ddv-3
PROB4_MAPPED_EXIT=-22   Model not found: prob-4
PROB4_MINIMAL_EXIT=-22  Model not found: prob-4
```

Their outputs exist only as zero-byte placeholders:

```text
test_clip_topaz_ddv-3_fhd_mapped.mp4  0 bytes
test_clip_topaz_ddv-3_minimal.mp4     0 bytes
test_clip_topaz_prob-4_4k_mapped.mp4  0 bytes
test_clip_topaz_prob-4_minimal.mp4    0 bytes
```

`ffprobe` reports `moov atom not found` for each output. There is no image,
audio, resolution, or trial watermark to inspect.

## Conclusion

The filter and its option schema are installed, but this standalone Topaz
ffmpeg process does not resolve either installed model ID. The failure occurs
before the encoder and remains with the minimal parameter sets, so CIA RENDER
has no preset-level fix to apply.

The remaining cause is inside the Topaz runtime's model-resolution path (for
example a runtime model registry, licensing/session state, or protected model
access). CIA RENDER must not attempt to bypass that behavior.

ENHANCE is deferred until after portability work. If it is later implemented,
it will be an optional runtime-probed feature: available only when a local,
licensed Topaz runtime can successfully execute its own model presets. No
Topaz binary, model, key, or license data belongs in this repository.

## Follow-up probes (2026-08-06)

The Topaz Video AI 6 GUI was started, confirmed responsive, then left open
while the minimal `ddv-3` command was retried. It still failed with the same
first error:

```text
DDV3_GUI_SESSION_EXIT=-22
Model not found: ddv-3
```

The GUI was then closed normally.

Its previous successful render log was found at:

```text
C:\Users\cia\AppData\Roaming\Topaz Labs LLC\Topaz Video AI\logs\2026-08-02-23-41-36-Main.tzlog
```

That log records a successful `ddv-3` export, including `Model found in list
ddv-3`, `License heartbeat successful`, and `Video Processor setup
successfully for model ddv-3`. Its ffmpeg command matches the preset above;
the only command-line delta is the GUI's additional `-nostdin -y -nostats`
arguments.

One final standalone test added exactly those three arguments and nothing
else. It also failed before encoding:

```text
DDV3_GUI_DELTA_EXIT=-22
Model not found: ddv-3
```

This confirms that the missing behavior is not a preset option or a GUI
command-line flag. It is private state managed by the Topaz GUI/runtime, and
is out of scope for CIA RENDER to reproduce or bypass.

## Alternative engine gate: Real-ESRGAN ncnn Vulkan (2026-08-06)

Decision: **red — do not implement the ENHANCE page with this runtime.**

The alternative was deliberately tested as an isolated engine, with no Tauri
or Svelte changes. The executable was the official portable Windows archive
referenced by the Real-ESRGAN project:

```text
Archive: C:\Users\cia\Downloads\realesrgan-ncnn-vulkan-20220424-windows.zip
SHA-256: ABC02804E17982A3BE33675E4D471E91EA374E65B70167ABC09E31ACB412802D
Runtime: C:\CIA RENDER\runtimes\realesrgan\realesrgan-ncnn-vulkan.exe
```

The test source is the exact short file used for the user's Topaz comparison:

```text
C:\Users\cia\Downloads\124124124124124-3x-RIFE-4.26-360fps_smoothie.mp4
```

Raw source probe:

```text
video: H.264, 1920x1080, 30/1 fps, 10.500000 s, 315 frames
audio: AAC, 48000 Hz, stereo
```

The NVIDIA query and the Vulkan executable both identify the same discrete
device as GPU 0; GPU 1 is the AMD integrated GPU:

```text
nvidia-smi: 0, NVIDIA GeForce RTX 3050 Laptop GPU, 4096 MiB, driver 610.62
Vulkan [0]: NVIDIA GeForce RTX 3050 Laptop GPU
Vulkan [1]: AMD Radeon(TM) Graphics
```

`realesrgan-x4plus` was used with `-g 0 -s 2 -t 128`/`-t 256`, which produces
the required exact 3840x2160 output from the FHD source. The official archive
does not contain `realesr-general-x4v3`; its included models are
`realesrgan-x4plus`, `realesrgan-x4plus-anime`, and AnimeVideo v3 variants.
No unverified model was substituted or downloaded.

The first frame succeeded and was verified with ffprobe:

```text
command: realesrgan-ncnn-vulkan.exe -i frame_000001.png -o frame_000001.png \
  -n realesrgan-x4plus -s 2 -g 0 -t 128 -j 1:1:1 -f png -v
exit: 0
elapsed: 24.927 s
output: 3840x2160, rgb24, 7,167,258 bytes
```

A second, warm measurement using larger 256-pixel tiles also succeeded but
remained far below the gate:

```text
command: realesrgan-ncnn-vulkan.exe -i frame_000001.png -o frame_000001_t256.png \
  -n realesrgan-x4plus -s 2 -g 0 -t 256 -j 1:1:1 -f png
exit: 0
elapsed: 19.821 s/frame
effective throughput: 0.05045 fps
estimate for 315 frames: 6,243.6 s (104.1 min)
```

This is below the agreed 0.5 fps stop threshold, predicts more than one hour
for this 10.5-second reference clip, and is roughly forty times slower than
the user's measured 4K Topaz export (2m42s). Therefore no 100–300-frame probe,
no full render, no MP4 sample, and no page implementation were started: the
bounded gate already failed and additional work would not alter that decision.

The two successful stills remain available solely as technical evidence:

```text
C:\Users\cia\Downloads\cia-render-realesrgan-probe-20260806\upscaled\frame_000001.png
C:\Users\cia\Downloads\cia-render-realesrgan-probe-20260806\upscaled\frame_000001_t256.png
```

They cannot validate temporal flicker; no user visual validation is requested
because Real-ESRGAN ncnn Vulkan was rejected on measured performance first.
The model-weight licence still requires audit before any future redistribution.
