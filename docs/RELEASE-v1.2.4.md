# cia render v1.2.4

This version fixes job tracking and replaces the intermediate RIFE video with a
direct stream to FFmpeg.

- One active job, settings retained while navigating, and descendant processes stopped on cancellation and shutdown.
- Outputs published without overwriting existing files; duration, cadence, dimensions, aspect ratio, and audio checked before reporting success.
- RIFE: the selected model is actually loaded; fractional/VFR FPS, rotation, text subtitles, and audio in boost/slowmo are handled.
- Smoothie: aspect ratio preserved, selected recipe used, and colour/LUT settings applied. LUT is limited to SDR sources explicitly tagged BT709.
- Final preview decoded from the result; extra previews loaded on hover; bounded cache and slider animations stopped while idle.
- FP16 and NVENC are explicit options; FP32/x264 remain the defaults.

Local RIFE tests show **6–22% less elapsed time with FP32/x264** and around
**36% with FP16/NVENC** on 48- and 120-frame clips. These measurements include
model loading; accelerated profiles do not produce identical images or
compression. [Method and measurements](https://github.com/cia213/cia-app/blob/main/docs/PATCH-QUALITE-PERFORMANCES.md).

Current profile: SDR 8-bit H.264. HDR and bitmap subtitles are rejected to avoid
an incorrect implicit conversion. Advanced ASS styles are not retained when
converting to MP4 subtitles.

The Windows package includes Smoothie and media tools. RIFE remains an optional
installation requiring an NVIDIA CUDA GPU. The installer and update manifest
use the public key already configured in previous versions.
