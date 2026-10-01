"""RIFE -> bounded RGB pipe -> final encode; no lossy intermediate or scene scan."""
import argparse
from collections import deque
from contextlib import nullcontext
from fractions import Fraction
import json
import math
import os
from pathlib import Path
import re
import subprocess
import sys
import tempfile
import threading
import time


def strip_unc(value):
    value = str(value or "")
    if value.startswith("\\\\?\\UNC\\") or value.startswith("\\\\?\\UNC/"):
        return "\\\\" + value[8:]
    return value[4:] if value.startswith("\\\\?\\") else value


def run_command(args, cwd=None):
    result = subprocess.run(args, capture_output=True, text=True, encoding="utf-8",
                            errors="replace", shell=False, cwd=cwd)
    if result.returncode:
        raise RuntimeError(f"Media command failed: {result.stderr[-4000:]}")
    return result.stdout


def get_video_info(video_path, ffprobe_exe):
    data = json.loads(run_command([ffprobe_exe, "-v", "error", "-show_streams", "-show_format", "-of", "json", video_path]))
    video = next((s for s in data.get("streams", []) if s.get("codec_type") == "video" and not s.get("disposition", {}).get("attached_pic")), None)
    if video is None:
        raise ValueError("The selected file has no video stream")
    def rational(value, default=Fraction(0)):
        try:
            return Fraction(str(value).replace(":", "/"))
        except (ValueError, ZeroDivisionError):
            return default
    rate = rational(video.get("avg_frame_rate", "0/1"))
    if rate <= 0:
        rate = rational(video.get("r_frame_rate", "0/1"))
    duration = float(video.get("duration") or data.get("format", {}).get("duration") or 0)
    width, height = int(video.get("width", 0)), int(video.get("height", 0))
    sar = rational(video.get("sample_aspect_ratio", "1:1"), Fraction(1))
    if sar <= 0:
        sar = Fraction(1)
    rotation = next((float(s.get("rotation", 0)) for s in video.get("side_data_list", []) if "rotation" in s), 0)
    if abs(rotation) % 180 == 90:
        width, height = height, width
        sar = 1 / sar
    if rate <= 0 or width <= 0 or height <= 0 or not math.isfinite(duration) or duration <= 0:
        raise ValueError("Video dimensions, cadence and duration must be valid")
    return {"fps": float(rate), "rate": str(rate), "width": width, "height": height,
            "video_index": int(video.get("index", data["streams"].index(video))),
            "duration": duration, "has_audio": any(s.get("codec_type") == "audio" for s in data["streams"]),
            "audio_codec": next((s.get("codec_name") for s in data["streams"] if s.get("codec_type") == "audio"), None),
            "subtitles": [s for s in data["streams"] if s.get("codec_type") == "subtitle"],
            "transfer": video.get("color_transfer", "unknown"),
            "sar": str(sar), "nominal_rate": str(rational(video.get("r_frame_rate", "0/1"))),
            "nb_frames": int(video["nb_frames"]) if str(video.get("nb_frames", "")).isdigit() else None}


def build_atempo_filter(speed):
    if not math.isfinite(speed) or speed <= 0:
        raise ValueError("Audio tempo must be finite and positive")
    filters = []
    while speed < 0.5:
        filters.append("atempo=0.5")
        speed /= 0.5
    while speed > 2:
        filters.append("atempo=2")
        speed /= 2
    return ",".join([*filters, f"atempo={speed:.9f}"])


def ensure_tools_in_path(*tools):
    parts = [strip_unc(p) for p in os.environ.get("PATH", "").split(os.pathsep) if p]
    for tool in tools:
        directory = str(Path(strip_unc(tool)).resolve().parent)
        if directory not in parts:
            parts.insert(0, directory)
    os.environ["PATH"] = os.pathsep.join(parts)
    os.environ["Path"] = os.environ["PATH"]


def retime_srt(text, factor):
    def scale(match):
        h, m, s, ms = map(int, match.groups())
        total = round(((h * 3600 + m * 60 + s) * 1000 + ms) * factor)
        hours, rem = divmod(total, 3600000)
        minutes, rem = divmod(rem, 60000)
        seconds, millis = divmod(rem, 1000)
        return f"{hours:02}:{minutes:02}:{seconds:02},{millis:03}"
    return re.sub(r"(?m)^\d{2,}:\d{2}:\d{2},\d{3} --> \d{2,}:\d{2}:\d{2},\d{3}.*$",
                  lambda line: re.sub(r"(\d{2,}):(\d{2}):(\d{2}),(\d{3})", scale, line[0]), text)


def encoder_arguments(encoder, crf, preset):
    if encoder == "h264_nvenc":
        return ["-c:v", encoder, "-preset", "p5", "-rc", "vbr", "-cq", str(crf), "-b:v", "0"]
    if encoder != "libx264":
        raise ValueError("Unsupported encoder")
    return ["-c:v", "libx264", "-crf", str(crf), "-preset", preset]


def validate_options(mode, factor, crf, preset, precision, encoder):
    if mode not in ("boost", "slowmo"):
        raise ValueError("Unsupported interpolation mode")
    if not math.isfinite(float(factor)) or int(factor) != factor or not 2 <= factor <= 10:
        raise ValueError("Interpolation factor must be an integer from 2 to 10")
    if not 0 <= crf <= 51:
        raise ValueError("Quality must be from 0 to 51")
    if preset not in ("ultrafast", "superfast", "veryfast", "faster", "fast", "medium", "slow", "slower", "veryslow"):
        raise ValueError("Unsupported x264 preset")
    if precision not in ("fp32", "fp16") or encoder not in ("libx264", "h264_nvenc"):
        raise ValueError("Unsupported precision or encoder")


def process_time_remap(video_path, mode="slowmo", factor=2.0, scene_threshold=None,
                       blend_cuts=None, crf=18, preset="medium", output_path=None,
                       ffmpeg_exe="ffmpeg", ffprobe_exe="ffprobe", rife_dir=None,
                       model_dir=None, precision="fp32", encoder="libx264", work_dir=None):
    validate_options(mode, factor, crf, preset, precision, encoder)
    factor = int(factor)
    if blend_cuts not in (None, 0):
        raise ValueError("Custom cut blending is unsupported; RIFE uses built-in cut handling")
    ensure_tools_in_path(ffmpeg_exe, ffprobe_exe)
    source = Path(strip_unc(video_path)).resolve()
    if not source.is_file():
        raise FileNotFoundError(f"Video not found: {source}")
    if output_path is None:
        raise ValueError("--output is required")
    destination = Path(strip_unc(output_path)).resolve()
    if destination.exists():
        raise FileExistsError("The output already exists; select a new destination")
    rife = Path(strip_unc(rife_dir) if rife_dir else Path(__file__).parent / "Practical-RIFE").resolve()
    model = Path(strip_unc(model_dir) if model_dir else rife / "train_log").resolve()
    for filename in ("flownet.pkl", "RIFE_HDv3.py", "IFNet_HDv3.py"):
        if not (model / filename).is_file():
            raise ValueError(f"Model code and weights are required: missing {filename}")
    worker_path = Path(__file__).with_name("rife_worker.py")
    if not worker_path.is_file():
        raise FileNotFoundError("The bundled RIFE worker is missing")
    info = get_video_info(str(source), ffprobe_exe)
    if info["transfer"] in ("smpte2084", "arib-std-b67"):
        raise ValueError("HDR interpolation requires tone mapping; convert to SDR first")
    if any(s.get("codec_name") not in {"subrip", "ass", "ssa", "mov_text", "text", "webvtt"} for s in info["subtitles"]):
        raise ValueError("Bitmap subtitles cannot be preserved in MP4; use text subtitles")
    fps = Fraction(info["rate"]) * (factor if mode == "boost" else 1)
    duration = info["duration"] * (factor if mode == "slowmo" else 1)
    expected_frames = max(1, round(duration * float(fps)))
    print(f"[cia render] RIFE {precision}, encoder={encoder}, FPS={fps}, duration={duration:.3f}s", flush=True)
    print("[cia render] Built-in scene handling; no additional scene scan", flush=True)
    started = time.monotonic()
    context = nullcontext(str(work_dir)) if work_dir else tempfile.TemporaryDirectory(prefix=".cia-render-", dir=destination.parent)
    with context as directory:
        directory = Path(directory)
        directory.mkdir(parents=True, exist_ok=True)
        partial = directory / "encoded.mp4"
        if partial.exists():
            raise FileExistsError("The job work directory already contains an encoded output")
        command = [ffmpeg_exe, "-v", "warning", "-n", "-f", "rawvideo", "-pix_fmt", "rgb24",
                   "-s:v", f"{info['width']}x{info['height']}", "-r", str(fps), "-i", "pipe:0"]
        next_input = 1
        if info["has_audio"]:
            command.extend(["-i", str(source)])
            next_input += 1
        subtitles = []
        for index, stream in enumerate(info["subtitles"]):
            text = run_command([ffmpeg_exe, "-v", "error", "-i", str(source), "-map", f"0:{stream['index']}", "-f", "srt", "pipe:1"])
            subtitle = directory / f"subtitle-{index}.srt"
            subtitle.write_text(retime_srt(text, factor if mode == "slowmo" else 1), encoding="utf-8")
            command.extend(["-i", str(subtitle)])
            subtitles.append(next_input)
            next_input += 1
        command.extend(["-map", "0:v:0", *encoder_arguments(encoder, crf, preset),
                        "-vf", f"pad=ceil(iw/2)*2:ceil(ih/2)*2,scale=in_range=full:out_range=tv:out_color_matrix=bt709,format=yuv420p,setsar={info['sar']},setparams=range=limited:color_primaries=bt709:color_trc=bt709:colorspace=bt709",
                        "-color_primaries", "bt709", "-color_trc", "bt709", "-colorspace", "bt709", "-color_range", "tv"])
        if info["has_audio"]:
            command.extend(["-map", "1:a:0"])
            if mode == "boost" and info["audio_codec"] in {"aac", "mp3", "ac3", "eac3", "alac"}:
                command.extend(["-c:a", "copy"])
            else:
                command.extend(["-c:a", "aac", "-b:a", "192k"])
            if mode == "slowmo":
                command.extend(["-filter:a", build_atempo_filter(1 / factor)])
        for index, input_index in enumerate(subtitles):
            command.extend(["-map", f"{input_index}:s:0", f"-metadata:s:s:{index}",
                            f"language={info['subtitles'][index].get('tags', {}).get('language', 'und')}"])
        if subtitles:
            command.extend(["-c:s", "mov_text"])
        command.extend(["-map_metadata", "-1", "-movflags", "+faststart", "-progress", "pipe:1", "-stats_period", "0.5", str(partial)])
        worker_command = [sys.executable, "-u", str(worker_path), "--video", str(source), "--ffmpeg", ffmpeg_exe,
                          "--rife-dir", str(rife), "--model-dir", str(model), "--width", str(info["width"]),
                          "--height", str(info["height"]), "--fps", info["rate"], "--factor", str(factor), "--precision", precision,
                          "--expected-frames", str(expected_frames), "--video-index", str(info["video_index"])]
        worker = encoding = None
        threads = []
        worker_errors, encoder_errors = deque(maxlen=80), deque(maxlen=80)
        counts = {}

        def drain(pipe, buffer, is_worker=False):
            for line in iter(pipe.readline, b""):
                line = line.decode("utf-8", "replace").strip()
                if line:
                    buffer.append(line)
                    print(line, file=sys.stderr, flush=True)
                    if is_worker and line.startswith("CIA_WORKER_DONE"):
                        counts.update({key: int(value) for key, value in re.findall(r"(input_frames|output_frames)=(\d+)", line)})

        try:
            worker = subprocess.Popen(worker_command, cwd=rife, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
            threads.append(threading.Thread(target=drain, args=(worker.stderr, worker_errors, True), daemon=True))
            threads[-1].start()
            encoding = subprocess.Popen(command, stdin=worker.stdout, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
            worker.stdout.close()
            threads.append(threading.Thread(target=drain, args=(encoding.stderr, encoder_errors), daemon=True))
            threads[-1].start()
            values = {"frame": "0", "fps": "0", "out_time": "00:00:00", "speed": "0x"}
            for line in iter(encoding.stdout.readline, b""):
                key, _, value = line.decode("utf-8", "replace").strip().partition("=")
                values[key] = value
                if key == "progress":
                    frame = int(values["frame"])
                    percent = min(99, round(frame / expected_frames * 100))
                    print(f"[cia render] ENCODING frame={frame} total_frames={expected_frames} fps={values['fps']} time={values['out_time'].split('.')[0]} speed={values['speed']} pct={percent}%", flush=True)
            encoder_code = encoding.wait()
            worker_code = worker.wait()
            for thread in threads:
                thread.join()
            if worker_code or encoder_code:
                raise RuntimeError(f"RIFE/encoding failed ({worker_code}/{encoder_code}): " + "\n".join([*worker_errors, *encoder_errors])[-6000:])
            if counts.get("output_frames", 0) <= 0:
                raise RuntimeError("RIFE worker did not confirm completion")
            if info["nb_frames"] is not None and Fraction(info["nominal_rate"]) == Fraction(info["rate"]) and counts["input_frames"] != info["nb_frames"]:
                raise RuntimeError("Decoded frame count differs from the complete CFR source")
            if abs(counts["input_frames"] / info["fps"] - info["duration"]) > max(0.08, 2 / info["fps"]):
                raise RuntimeError("Decoded video duration differs from the source")
            result = get_video_info(str(partial), ffprobe_exe)
            expected_duration = counts["output_frames"] / float(fps)
            if abs(result["duration"] - expected_duration) > max(0.08, 2 / float(fps)):
                raise RuntimeError("Encoded video duration differs from generated frames")
            if result["nb_frames"] != counts["output_frames"] or Fraction(result["rate"]) != fps:
                raise RuntimeError("Encoded frame count or cadence differs from RIFE output")
            if info["has_audio"] and not result["has_audio"]:
                raise RuntimeError("Output audio is missing")
            # No-clobber publication, even if another program races our reservation.
            if os.name == "nt":
                os.rename(partial, destination)
            else:
                os.link(partial, destination)
                partial.unlink()
            print(f"[OK] COMPLETE: Output saved to {destination} (Processing time: {time.monotonic()-started:.3f}s)", flush=True)
            return str(destination)
        finally:
            for process in (encoding, worker):
                if process is not None:
                    if process.poll() is None:
                        process.kill()
                    process.wait()
            for thread in threads:
                thread.join(timeout=5)
            for process in (encoding, worker):
                if process is not None:
                    for pipe in (process.stdout, process.stderr):
                        if pipe is not None and not pipe.closed:
                            pipe.close()


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description="cia render streaming RIFE orchestration")
    parser.add_argument("--video", required=True)
    parser.add_argument("--mode", choices=("slowmo", "boost"), default="slowmo")
    parser.add_argument("--factor", type=float, default=2)
    parser.add_argument("--scene_threshold", type=float, default=None, help=argparse.SUPPRESS)
    parser.add_argument("--blend-cuts", type=int, default=None, help=argparse.SUPPRESS)
    parser.add_argument("--crf", type=int, default=18)
    parser.add_argument("--preset", default="medium")
    parser.add_argument("--precision", choices=("fp32", "fp16"), default="fp32")
    parser.add_argument("--encoder", choices=("libx264", "h264_nvenc"), default="libx264")
    parser.add_argument("--output", required=True)
    parser.add_argument("--ffmpeg", required=True)
    parser.add_argument("--ffprobe", required=True)
    parser.add_argument("--rife-dir", required=True)
    parser.add_argument("--model-dir", default=None)
    parser.add_argument("--work-dir", default=None)
    try:
        options = vars(parser.parse_args())
        options["video_path"] = options.pop("video")
        options["output_path"] = options.pop("output")
        options["ffmpeg_exe"] = options.pop("ffmpeg")
        options["ffprobe_exe"] = options.pop("ffprobe")
        process_time_remap(**options)
    except Exception as error:
        print(f"[time_remap fatal error] {error}", file=sys.stderr, flush=True)
        sys.exit(1)
