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


def configure_text_streams():
    """Logs must not depend on the Windows codepage or invalidate good output."""
    for stream in (sys.stdout, sys.stderr):
        if hasattr(stream, "reconfigure"):
            stream.reconfigure(encoding="utf-8", errors="replace")


def child_environment():
    environment = dict(os.environ)
    for key in ("PYTHONHOME", "PYTHONPATH", "PYTHONSTARTUP", "PYTHONUSERBASE", "PYTHONIOENCODING"):
        environment.pop(key, None)
    environment.update(PYTHONNOUSERSITE="1", PYTHONUTF8="1", PYTHONIOENCODING="utf-8")
    return environment


def strip_unc(value):
    value = str(value or "")
    if value.startswith("\\\\?\\UNC\\") or value.startswith("\\\\?\\UNC/"):
        return "\\\\" + value[8:]
    return value[4:] if value.startswith("\\\\?\\") else value


def run_command(args, cwd=None, reject_stderr=False, timeout=None):
    result = subprocess.run(args, capture_output=True, text=True, encoding="utf-8",
                            errors="replace", shell=False, cwd=cwd, timeout=timeout)
    if result.returncode or (reject_stderr and result.stderr.strip()):
        raise RuntimeError(f"Media command failed: {result.stderr[-4000:]}")
    return result.stdout


def duration_tag(tags):
    value = next((value for key, value in tags.items() if key.lower() == "duration"), None)
    try:
        hours, minutes, seconds = map(float, str(value).split(":"))
        duration = hours * 3600 + minutes * 60 + seconds
        return duration if math.isfinite(duration) else None
    except (TypeError, ValueError):
        return None


def packet_video_interval(video_path, ffprobe_exe, index, rate):
    """Only ambiguous containers need this bounded timestamp-only fallback."""
    text = run_command([ffprobe_exe, "-v", "error", "-select_streams", str(index),
                        "-show_packets", "-show_entries", "packet=pts_time,duration_time",
                        "-of", "csv=p=0", video_path], reject_stderr=True, timeout=60)
    first, last = float("inf"), float("-inf")
    for line in text.splitlines():
        values = line.split(",")
        try:
            start = float(values[0])
            length = float(values[1]) if len(values) > 1 and values[1] != "N/A" else 1 / float(rate)
        except ValueError:
            continue
        if math.isfinite(start) and math.isfinite(length) and length > 0:
            first, last = min(first, start), max(last, start + length)
    if not math.isfinite(first) or not math.isfinite(last) or last <= first:
        raise ValueError("The video timeline could not be determined. Remux the recording before interpolation.")
    return first, last - first


def get_video_info(video_path, ffprobe_exe, count_frames=False):
    arguments = [ffprobe_exe, "-v", "error"]
    if count_frames:
        arguments.append("-count_frames")
    data = json.loads(run_command([*arguments, "-show_streams", "-show_format", "-of", "json", video_path],
                                  reject_stderr=count_frames))
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
    def finite_number(value, default=0):
        try:
            result = float(value)
            return result if math.isfinite(result) else default
        except (ValueError, TypeError):
            return default
    video_start = finite_number(video.get("start_time"), finite_number(data.get("format", {}).get("start_time")))
    duration = finite_number(video.get("duration"))
    endpoint = duration_tag(video.get("tags", {}))
    if duration <= 0 and endpoint is not None:
        duration = endpoint - video_start
    if duration <= 0 and rate > 0:
        video_start, duration = packet_video_interval(video_path, ffprobe_exe,
                                                       int(video.get("index", data["streams"].index(video))), rate)
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
    audio_tracks = []
    for stream in data.get("streams", []):
        if stream.get("codec_type") != "audio":
            continue
        tags = {key.lower(): value for key, value in stream.get("tags", {}).items()}
        title = tags.get("title") or tags.get("handler_name") or ""
        if title == "SoundHandler":
            title = ""
        audio_start = finite_number(stream.get("start_time"))
        audio_duration = finite_number(stream.get("duration"))
        audio_endpoint = duration_tag(tags)
        if audio_duration <= 0 and audio_endpoint is not None:
            audio_duration = max(0, audio_endpoint - audio_start)
        audio_tracks.append({"index": int(stream.get("index", data["streams"].index(stream))), "codec": stream.get("codec_name"),
                             "channels": int(stream.get("channels", 0)),
                             "channel_layout": stream.get("channel_layout", ""),
                             "sample_rate": int(stream.get("sample_rate", 48000)),
                             "language": tags.get("language", "und"), "title": title,
                             "start_time": audio_start, "duration": audio_duration,
                             "disposition": stream.get("disposition", {})})
    return {"fps": float(rate), "rate": str(rate), "width": width, "height": height,
            "video_index": int(video.get("index", data["streams"].index(video))),
            "video_start_time": video_start, "audio_tracks": audio_tracks,
            "duration": duration, "has_audio": any(s.get("codec_type") == "audio" for s in data["streams"]),
            "audio_codec": next((s.get("codec_name") for s in data["streams"] if s.get("codec_type") == "audio"), None),
            "subtitles": [s for s in data["streams"] if s.get("codec_type") == "subtitle"],
            "transfer": video.get("color_transfer", "unknown"),
            "sar": str(sar), "nominal_rate": str(rational(video.get("r_frame_rate", "0/1"))),
            "nb_frames": int(video["nb_frames"]) if str(video.get("nb_frames", "")).isdigit() else None,
            "decoded_frames": int(video["nb_read_frames"]) if str(video.get("nb_read_frames", "")).isdigit() else None}


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


def normalize_srt(text, video_start, video_duration, factor):
    """Clip subtitle cues to the same video interval used for every audio track."""
    def seconds(timestamp):
        hours, minutes, secs, millis = map(int, re.split(r"[:,]", timestamp))
        return hours * 3600 + minutes * 60 + secs + millis / 1000

    def timestamp(value):
        total = max(0, round(value * factor * 1000))
        hours, rem = divmod(total, 3600000)
        minutes, rem = divmod(rem, 60000)
        secs, millis = divmod(rem, 1000)
        return f"{hours:02}:{minutes:02}:{secs:02},{millis:03}"

    normalized = []
    for block in re.split(r"\r?\n\s*\r?\n", text.strip()):
        lines = block.splitlines()
        timing = next((index for index, line in enumerate(lines) if " --> " in line), None)
        if timing is None:
            continue
        match = re.match(r"(\d{2,}:\d{2}:\d{2},\d{3}) --> (\d{2,}:\d{2}:\d{2},\d{3})(.*)", lines[timing])
        if not match:
            raise ValueError("The extracted subtitle contains an invalid cue timestamp")
        begin = max(0, seconds(match[1]) - video_start)
        end = min(video_duration, seconds(match[2]) - video_start)
        if end <= begin:
            continue
        normalized.append("\n".join([str(len(normalized) + 1),
                                    f"{timestamp(begin)} --> {timestamp(end)}{match[3]}",
                                    *lines[timing + 1:]]))
    return "\n\n".join(normalized) + ("\n" if normalized else "")


def audio_filter(track, video_start, video_duration, output_duration, factor):
    """Represent relative delay as silence; preserve pitch during time stretching."""
    audio_start = track["start_time"]
    audio_end = audio_start + track["duration"] if track["duration"] > 0 else float("inf")
    if audio_start >= video_start + video_duration or audio_end <= video_start:
        # Even a non-overlapping track retains its identity as a silent bounded track.
        filters = ["atrim=duration=0.01", "asetpts=PTS-STARTPTS", "volume=0"]
    else:
        start = max(video_start, audio_start)
        end = video_start + video_duration
        filters = [f"atrim=start={start:.9f}:end={end:.9f}", "asetpts=PTS-STARTPTS"]
        delay_samples = max(0, round((audio_start - video_start) * track["sample_rate"]))
        if delay_samples:
            filters.append(f"adelay={delay_samples}S:all=1")
    if factor != 1:
        filters.append(f"rubberband=tempo={1 / factor:.9f}:pitch=1.0")
    filters.extend(["apad", f"atrim=duration={output_duration:.9f}", "asetpts=PTS-STARTPTS"])
    return ",".join(filters)


def disposition_argument(track):
    return "+".join(name for name in ("default", "forced", "hearing_impaired", "visual_impaired", "original", "comment")
                    if track.get("disposition", {}).get(name)) or "0"


def canonical_audio_tracks(tracks):
    """MP4 enables a playback track even when the source selects none."""
    selected = any(track.get("disposition", {}).get("default") for track in tracks)
    return [dict(track, disposition=dict(track.get("disposition", {}),
                                         default=int(bool(track.get("disposition", {}).get("default")) or
                                                     (index == 0 and not selected))))
            for index, track in enumerate(tracks)]


def validate_audio_tracks(source_tracks, result_tracks, duration):
    if len(result_tracks) != len(source_tracks):
        raise RuntimeError("Output audio track count differs from the source")
    for index, (source, result) in enumerate(zip(canonical_audio_tracks(source_tracks), result_tracks)):
        if source["channels"] != result["channels"]:
            raise RuntimeError(f"Output audio track {index + 1} changed its channel count")
        if source["channel_layout"] and source["channel_layout"] != result["channel_layout"]:
            raise RuntimeError(f"Output audio track {index + 1} cannot preserve its channel layout in this profile")
        for field in ("language", "title"):
            if source[field] != result[field]:
                raise RuntimeError(f"Output audio track {index + 1} changed its {field}")
        for flag in ("default", "forced", "hearing_impaired", "visual_impaired", "original", "comment"):
            if bool(source.get("disposition", {}).get(flag)) != bool(result.get("disposition", {}).get(flag)):
                raise RuntimeError(f"Output audio track {index + 1} changed its {flag} disposition")
        if abs(result["start_time"]) > 0.030 or abs(result["duration"] - duration) > 0.060:
            raise RuntimeError(f"Output audio track {index + 1} has an invalid timeline")


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
                       model_dir=None, precision="fp32", encoder="libx264", work_dir=None,
                       runtime_manifest=None, device_index=0):
    configure_text_streams()
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
    if info["width"] % 2 or info["height"] % 2:
        raise ValueError("H.264 4:2:0 output requires even video dimensions; resize the source explicitly first")
    if info["transfer"] in ("smpte2084", "arib-std-b67"):
        raise ValueError("HDR interpolation requires tone mapping; convert to SDR first")
    if any(s.get("codec_name") not in {"subrip", "ass", "ssa", "mov_text", "text", "webvtt"} for s in info["subtitles"]):
        raise ValueError("Bitmap subtitles cannot be preserved in MP4; use text subtitles")
    fps = Fraction(info["rate"]) * (factor if mode == "boost" else 1)
    duration = info["duration"] * (factor if mode == "slowmo" else 1)
    source_frames = info["nb_frames"] if info["nb_frames"] is not None and Fraction(info["nominal_rate"]) == Fraction(info["rate"]) else max(1, round(info["duration"] * info["fps"]))
    expected_frames = source_frames * factor
    output_duration = expected_frames / float(fps)
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
        command = [ffmpeg_exe, "-v", "warning", "-copyts", "-n", "-f", "rawvideo", "-pix_fmt", "rgb24",
                   "-s:v", f"{info['width']}x{info['height']}", "-r", str(fps), "-i", "pipe:0"]
        next_input = 1
        if info["has_audio"]:
            command.extend(["-i", str(source)])
            next_input += 1
        subtitles = []
        for index, stream in enumerate(info["subtitles"]):
            text = run_command([ffmpeg_exe, "-v", "error", "-copyts", "-i", str(source), "-map", f"0:{stream['index']}", "-f", "srt", "pipe:1"])
            subtitle = directory / f"subtitle-{index}.srt"
            normalized = normalize_srt(text, info["video_start_time"], info["duration"], factor if mode == "slowmo" else 1)
            if not normalized:
                print(f"[cia render] Subtitle track {index + 1} has no cues inside the selected video interval.", flush=True)
                continue
            subtitle.write_text(normalized, encoding="utf-8")
            command.extend(["-i", str(subtitle)])
            subtitles.append((next_input, stream))
            next_input += 1
        command.extend(["-map", "0:v:0", *encoder_arguments(encoder, crf, preset),
                        "-vf", f"pad=ceil(iw/2)*2:ceil(ih/2)*2,scale=in_range=full:out_range=tv:out_color_matrix=bt709,format=yuv420p,setsar={info['sar']},setparams=range=limited:color_primaries=bt709:color_trc=bt709:colorspace=bt709",
                        "-color_primaries", "bt709", "-color_trc", "bt709", "-colorspace", "bt709", "-color_range", "tv"])
        delivery_tracks = canonical_audio_tracks(info["audio_tracks"])
        if delivery_tracks and not any(track["disposition"].get("default") for track in info["audio_tracks"]):
            print("[cia render] MP4 selects the first audio track for default playback; all audio tracks are retained.", flush=True)
        for index, track in enumerate(delivery_tracks):
            command.extend(["-map", f"1:{track['index']}"])
            aligned_copy = (mode == "boost" and track["codec"] in {"aac", "mp3", "ac3", "eac3", "alac"}
                            and abs(info["video_start_time"]) < 0.001 and abs(track["start_time"]) < 0.001
                            and abs(track["duration"] - output_duration) < 0.030)
            if aligned_copy:
                command.extend([f"-c:a:{index}", "copy"])
            else:
                command.extend([f"-c:a:{index}", "aac", f"-b:a:{index}", "192k", f"-filter:a:{index}",
                                audio_filter(track, info["video_start_time"], info["duration"], output_duration,
                                             factor if mode == "slowmo" else 1)])
            command.extend([f"-metadata:s:a:{index}", f"language={track['language']}",
                            f"-disposition:a:{index}", disposition_argument(track)])
            if track["title"]:
                command.extend([f"-metadata:s:a:{index}", f"title={track['title']}",
                                f"-metadata:s:a:{index}", f"handler_name={track['title']}"])
        for index, (input_index, stream) in enumerate(subtitles):
            command.extend(["-map", f"{input_index}:s:0", f"-metadata:s:s:{index}",
                            f"language={stream.get('tags', {}).get('language', 'und')}",
                            f"-disposition:s:{index}", disposition_argument({"disposition": stream.get("disposition", {})})])
        if subtitles:
            command.extend(["-c:s", "mov_text"])
        command.extend(["-map_metadata", "-1", "-movflags", "+faststart", "-progress", "pipe:1", "-stats_period", "0.5", str(partial)])
        worker_command = [sys.executable, "-I", "-X", "utf8", "-u", str(worker_path), "--video", str(source), "--ffmpeg", ffmpeg_exe,
                          "--rife-dir", str(rife), "--model-dir", str(model), "--width", str(info["width"]),
                          "--height", str(info["height"]), "--fps", info["rate"], "--factor", str(factor), "--precision", precision,
                          "--expected-frames", str(expected_frames), "--video-index", str(info["video_index"])]
        worker_command.extend(["--device-index", str(device_index)])
        if runtime_manifest:
            worker_command.extend(["--runtime-manifest", str(runtime_manifest)])
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
            worker = subprocess.Popen(worker_command, cwd=rife, env=child_environment(), stdout=subprocess.PIPE, stderr=subprocess.PIPE)
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
            result = get_video_info(str(partial), ffprobe_exe, count_frames=True)
            expected_duration = counts["output_frames"] / float(fps)
            if abs(result["duration"] - expected_duration) > max(0.08, 2 / float(fps)):
                raise RuntimeError("Encoded video duration differs from generated frames")
            if result["decoded_frames"] != counts["output_frames"] or Fraction(result["rate"]) != fps:
                raise RuntimeError("Encoded frame count or cadence differs from RIFE output")
            validate_audio_tracks(info["audio_tracks"], result["audio_tracks"], expected_duration)
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
    parser.add_argument("--runtime-manifest", default=None)
    parser.add_argument("--device-index", type=int, default=0)
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
