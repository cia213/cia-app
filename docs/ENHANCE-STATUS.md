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
