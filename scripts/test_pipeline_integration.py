"""Small end-to-end RIFE fixtures. GPU tests run only with explicit --run.

Uses unique workspace-owned files; real clips/models are never modified.
Unit checks and FFmpeg fixture generation require no GPU. Real interpolation
checks cover fractional FPS, odd factors, audio, subtitles, rotation and SAR.
"""
import argparse
from fractions import Fraction
import importlib.util
import json
import os
import array
import hashlib
from pathlib import Path
import subprocess
import struct
import time
import unittest


UI = Path(__file__).resolve().parents[1]
APP = UI.parent / "time-remap-app"
PIPELINE = UI / "src-tauri/resources/time_remap.py"
FFMPEG = UI / "src-tauri/resources/runtime/ffmpeg/ffmpeg.exe"
FFPROBE = FFMPEG.with_name("ffprobe.exe")
PYTHON = APP / "venv/Scripts/python.exe"
RIFE = APP / "Practical-RIFE"
MODEL = None
MANIFEST = None
DEVICE_INDEX = 0
PRECISION = "fp32"
ENCODER = "libx264"


def load_pipeline():
    spec = importlib.util.spec_from_file_location("time_remap", PIPELINE)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def run(command, *, check=True, timeout=120, env=None):
    result = subprocess.run([str(part) for part in command], capture_output=True,
                            encoding="utf-8", errors="replace", timeout=timeout, env=env)
    if check and result.returncode:
        raise RuntimeError(result.stderr[-6000:])
    return result


def probe(path):
    return json.loads(run([FFPROBE, "-v", "error", "-count_frames", "-show_streams",
                           "-show_format", "-of", "json", path]).stdout)


def selected_video(info):
    return next(s for s in info["streams"] if s["codec_type"] == "video"
                and not s.get("disposition", {}).get("attached_pic"))


def make_cover_first(directory, source):
    """Create a genuine MP4 whose attached picture is demuxed before video.

    FFmpeg normally writes covr metadata after the video track. Moving the
    same-size udta atom earlier within moov changes stream discovery order;
    it changes neither sample payloads nor chunk offsets. This narrowly scoped
    parser only handles the small, 32-bit atoms in our generated fixture.
    """
    cover = directory / "cover-picture.jpg"
    run([FFMPEG, "-v", "error", "-f", "lavfi", "-i", "color=red:size=48x32",
         "-frames:v", "1", cover])
    output = directory / "cover-first-source.mp4"
    run([FFMPEG, "-v", "error", "-i", cover, "-i", source, "-map", "0:v:0",
         "-map", "1:v:0", "-c", "copy", "-disposition:v:0", "attached_pic", output])
    data = bytearray(output.read_bytes())

    def boxes(start, end):
        offset = start
        while offset < end:
            size, kind = struct.unpack_from(">I4s", data, offset)
            if size < 8 or offset + size > end:
                raise AssertionError("Unexpected MP4 fixture atom")
            yield offset, size, kind
            offset += size

    moved = False
    for offset, size, kind in boxes(0, len(data)):
        if kind == b"moov":
            children = [(child_kind, bytes(data[child:child+child_size]))
                        for child, child_size, child_kind in boxes(offset+8, offset+size)]
            if not any(child_kind == b"udta" for child_kind, _ in children):
                raise AssertionError("MP4 fixture has no cover metadata")
            payload = (b"".join(blob for child_kind, blob in children if child_kind == b"udta")
                       + b"".join(blob for child_kind, blob in children if child_kind != b"udta"))
            data[offset+8:offset+size] = payload
            moved = True
    if not moved:
        raise AssertionError("MP4 fixture has no movie atom")
    output.write_bytes(data)
    streams = probe(output)["streams"]
    if not streams[0].get("disposition", {}).get("attached_pic") or selected_video({"streams": streams})["index"] != 1:
        raise AssertionError("Fixture failed to create a cover-first video")
    return output


def audio_packet_hashes(path, index=0):
    result = run([FFPROBE, "-v", "error", "-select_streams", f"a:{index}", "-show_packets",
                  "-show_data_hash", "sha256", "-show_entries", "packet=data_hash", "-of", "json", path])
    return [packet["data_hash"] for packet in json.loads(result.stdout).get("packets", [])]


class UnitContractTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.module = load_pipeline()

    def test_retime_subtitle_timestamps_only(self):
        source = ("1\n00:00:00,050 --> 00:00:00,150\n"
                  "Speak at 00:00:00,050\n\n"
                  "2\n01:02:03,456 --> 01:02:04,999 X1:2\nText\n")
        result = self.module.retime_srt(source, 3)
        self.assertIn("00:00:00,150 --> 00:00:00,450", result)
        self.assertIn("Speak at 00:00:00,050", result)
        self.assertIn("03:06:10,368 --> 03:06:14,997 X1:2", result)

    def test_identity_subtitle_retime(self):
        text = "1\n00:00:00,101 --> 00:00:01,777\nTest\n"
        self.assertEqual(self.module.retime_srt(text, 1), text)

    def test_invalid_options_fail_before_launch(self):
        for factor in (0, 1, 2.5, 11, float("inf"), float("nan")):
            with self.subTest(factor=factor), self.assertRaises(ValueError):
                self.module.validate_options("boost", factor, 18, "fast", "fp32", "libx264")
        for quality in (-1, 52):
            with self.subTest(quality=quality), self.assertRaises(ValueError):
                self.module.validate_options("boost", 2, quality, "fast", "fp32", "libx264")

    def test_rubberband_matches_requested_slowdown(self):
        track = {"start_time": 0, "duration": 1, "sample_rate": 48000}
        for factor in (2, 3, 10):
            filters = self.module.audio_filter(track, 0, 1, factor, factor)
            self.assertIn(f"rubberband=tempo={1 / factor:.9f}:pitch=1.0", filters)
            self.assertNotIn("atempo", filters)


def generate_fixture(directory, name, *, rate="24/1", frames=6, audio=False,
                     subtitles=False, sar="1/1", transfer=None, vfr=False,
                     audio_tracks=0, video_offset=0, audio_offset=0, container="mp4", audio_duration=None):
    destination = directory / f"{name}.{container}"
    duration = float(Fraction(frames) / Fraction(rate))
    dimensions = "320x192" if ENCODER == "h264_nvenc" else "96x64"
    command = [FFMPEG, "-v", "error", "-f", "lavfi", "-i",
               f"testsrc2=size={dimensions}:rate={rate}:duration={duration}"]
    audio_count = audio_tracks or (1 if audio else 0)
    for index in range(audio_count):
        command.extend(["-f", "lavfi", "-i", "sine=frequency=400:sample_rate=48000"])
        command[-1] = f"sine=frequency={400 + index * 400}:sample_rate=48000:duration={audio_duration or duration + max(video_offset, 0)}"
    if subtitles:
        subtitle = directory / f"{name}.srt"
        def cue_time(value):
            seconds, millis = divmod(round(value * 1000), 1000)
            return f"00:00:{seconds:02},{millis:03}"
        subtitle.write_text(f"1\n{cue_time(max(0, video_offset) + 0.05)} --> {cue_time(max(0, video_offset) + 0.15)}\nRetime this cue\n", encoding="utf-8")
        command.extend(["-i", str(subtitle)])
    command.extend(["-map", "0:v:0", "-frames:v", str(frames)])
    if not vfr and not video_offset and not audio_offset and not audio_duration:
        command.extend(["-t", str(duration)])
    filters = f"setsar={sar}"
    if video_offset:
        filters += f",setpts=PTS+({video_offset})/TB"
    if vfr:
        filters += ",setpts=PTS+if(gte(N\\,3)\\,2/(24*TB)\\,0)"
    if transfer:
        filters += f",setparams=color_primaries=bt2020:color_trc={transfer}:colorspace=bt2020nc"
    command.extend(["-vf", filters, "-c:v", "libx264", "-crf", "18", "-preset", "fast"])
    if vfr or video_offset:
        command.extend(["-fps_mode", "vfr"])
    for index in range(audio_count):
        command.extend(["-map", f"{index + 1}:a:0", f"-c:a:{index}", "aac", f"-b:a:{index}", "192k",
                        f"-metadata:s:a:{index}", f"language={'eng' if index == 0 else 'fra'}",
                        f"-metadata:s:a:{index}", f"handler_name={'Game' if index == 0 else 'Microphone'}"])
        if audio_offset:
            command.extend([f"-filter:a:{index}", f"asetpts=PTS+({audio_offset})/TB"])
        elif video_offset > 0:
            command.extend([f"-filter:a:{index}", f"volume=if(lt(t\\,{video_offset})\\,0\\,1):eval=frame"])
    if subtitles:
        command.extend(["-map", f"{audio_count + 1}:s:0", "-c:s", "mov_text",
                        "-metadata:s:s:0", "language=fra"])
    if transfer:
        command.extend(["-color_trc", transfer])
    command.append(destination)
    run(command)
    if transfer:
        metadata = probe(destination)
        stream = next(s for s in metadata["streams"] if s["codec_type"] == "video")
        assert stream.get("color_transfer") == transfer, "Fixture is missing its HDR transfer metadata"
    return destination


def check_output(source, output, *, factor, mode, dimensions=None, sar=None, subtitles=False):
    input_info, output_info = probe(source), probe(output)
    first = selected_video(input_info)
    final = next(s for s in output_info["streams"] if s["codec_type"] == "video")
    for property_name in ("color_space", "color_transfer", "color_primaries"):
        if final.get(property_name) != "bt709":
            raise AssertionError(f"SDR output is missing BT.709 {property_name}")
    frames = int(first["nb_read_frames"]) * factor
    expected_rate = Fraction(first["avg_frame_rate"]) * (factor if mode == "boost" else 1)
    if int(final["nb_read_frames"]) != frames:
        raise AssertionError(f"Expected {frames} output frames, got {final['nb_read_frames']}")
    if Fraction(final["avg_frame_rate"]) != expected_rate:
        raise AssertionError("Fractional frame rate changed")
    expected_dimensions = dimensions or (first["width"], first["height"])
    if (final["width"], final["height"]) != expected_dimensions:
        raise AssertionError(f"Expected dimensions {expected_dimensions}, got {(final['width'], final['height'])}")
    expected_duration = frames / float(expected_rate)
    if abs(float(final["duration"]) - expected_duration) > 1.01 / float(expected_rate):
        raise AssertionError("Video duration does not match generated frame timeline")
    if sar and Fraction(final.get("sample_aspect_ratio", "1:1").replace(":", "/")) != Fraction(sar):
        raise AssertionError("Sample aspect ratio changed")
    first_audio = [s for s in input_info["streams"] if s["codec_type"] == "audio"]
    final_audio = [s for s in output_info["streams"] if s["codec_type"] == "audio"]
    if len(first_audio) != len(final_audio):
        raise AssertionError("Audio track count changed")
    copied_audio = []
    for index, (initial_audio, result_audio) in enumerate(zip(first_audio, final_audio)):
        if abs(float(result_audio["duration"]) - expected_duration) > 0.060:
            raise AssertionError("Audio retiming outside 60-ms tolerance")
        if initial_audio["channels"] != result_audio["channels"]:
            raise AssertionError("Audio channels changed")
        initial_tags = {key.lower(): value for key, value in initial_audio.get("tags", {}).items()}
        result_tags = {key.lower(): value for key, value in result_audio.get("tags", {}).items()}
        for tag in ("language", "handler_name"):
            if initial_tags.get(tag, "und" if tag == "language" else "") != result_tags.get(tag, "und" if tag == "language" else ""):
                raise AssertionError(f"Audio {tag} changed")
        for flag in ("default", "forced"):
            expected_flag = bool(initial_audio.get("disposition", {}).get(flag))
            if flag == "default" and index == 0 and not any(track.get("disposition", {}).get("default") for track in first_audio):
                expected_flag = True
            if expected_flag != bool(result_audio.get("disposition", {}).get(flag)):
                raise AssertionError(f"Audio {flag} disposition changed")
        initial_duration = float(initial_audio.get("duration", 0))
        if mode == "boost" and initial_audio["codec_name"] == "aac" and abs(float(first.get("start_time", 0))) < 0.001 and abs(float(initial_audio.get("start_time", 0))) < 0.001 and abs(initial_duration - expected_duration) < 0.030:
            if audio_packet_hashes(source, index) != audio_packet_hashes(output, index):
                raise AssertionError("Compatible AAC boost audio was re-encoded or lost packets")
            copied_audio.append(True)
        else:
            copied_audio.append(False)
    final_subtitles = [s for s in output_info["streams"] if s["codec_type"] == "subtitle"]
    if bool(final_subtitles) != subtitles:
        raise AssertionError("Subtitle presence changed")
    if subtitles:
        if final_subtitles[0].get("tags", {}).get("language") != "fra":
            raise AssertionError("Subtitle language metadata was lost")
        text = run([FFMPEG, "-v", "error", "-i", output, "-map", "0:s:0", "-f", "srt", "pipe:1"]).stdout
        times = "00:00:00,150 --> 00:00:00,450" if mode == "slowmo" and factor == 3 else "00:00:00,050 --> 00:00:00,150"
        if times not in text:
            raise AssertionError(f"Subtitle cue was not correctly retimed: {text}")
    run([FFMPEG, "-v", "error", "-xerror", "-i", output, "-map", "0:v:0",
         "-map", "0:a?", "-f", "null", "-"])
    return {"input_info": input_info, "output_info": output_info,
            "video_expected_duration": expected_duration,
            "aac_packets_identical": bool(copied_audio and all(copied_audio))}


def check_audio_signal(output, case):
    import numpy as np
    signal_results = []
    tracks = [stream for stream in probe(output)["streams"] if stream["codec_type"] == "audio"]
    for index, track in enumerate(tracks):
        # Decode binary separately from the text-oriented command helper.
        binary = subprocess.run([str(item) for item in [FFMPEG, "-v", "error", "-i", output,
                                 "-map", f"0:a:{index}", "-ac", "1", "-ar", "48000", "-f", "f32le", "pipe:1"]],
                                capture_output=True, timeout=30)
        if binary.returncode:
            raise AssertionError("Audio signal could not be decoded")
        samples = array.array("f")
        samples.frombytes(binary.stdout)
        slowdown = case["factor"] if case["mode"] == "slowmo" else 1
        delay = max(0, case.get("audio_offset", 0) - case.get("video_offset", 0)) * slowdown
        if delay:
            prefix = samples[:max(1, round(max(0, delay - 0.06) * 48000))]
            rms = (sum(value * value for value in prefix) / len(prefix)) ** 0.5
            if rms > 0.001:
                raise AssertionError("Audio begins before its normalized delay")
        begin = round((delay + 0.06) * 48000)
        end = max(begin + 1, len(samples) - round(0.06 * 48000))
        tone = samples[begin:end]
        if len(tone) < 1000:
            raise AssertionError("Fixture tone interval is too short")
        rms = (sum(value * value for value in tone) / len(tone)) ** 0.5
        if rms < 0.03:
            raise AssertionError("A preserved audio track lost its signal")
        # Time stretching can add small high-frequency zero crossings; measure
        # the actual dominant tone rather than treating every crossing as pitch.
        spectrum = np.abs(np.fft.rfft(np.asarray(tone) * np.hanning(len(tone))))
        frequency = int(np.argmax(spectrum[1:])) * 48000 / len(tone) + 48000 / len(tone)
        expected = 400 + index * 400
        if abs(frequency - expected) > 12:
            raise AssertionError(f"Audio track {index + 1} pitch changed: {frequency} instead of {expected}")
        signal_results.append({"index": index, "frequency": frequency, "rms": rms, "delay": delay})
    return signal_results


def integration(run_gpu, selected_cases=None, output_directory=None):
    directory = (output_directory or UI / "src-tauri/target/pipeline-integration") / f"integration-{time.time_ns()}"
    directory.mkdir(parents=True, exist_ok=False)
    provenance = {"precision": PRECISION, "encoder": ENCODER, "device_index": DEVICE_INDEX,
                  "source_sha256": {str(path.relative_to(UI)).replace("\\", "/"): hashlib.sha256(path.read_bytes()).hexdigest()
                                    for path in (PIPELINE, PIPELINE.with_name("rife_worker.py"),
                                                 PIPELINE.with_name("runtime_health.py"), Path(__file__).resolve())}}
    if MANIFEST:
        manifest = json.loads(MANIFEST.read_text(encoding="utf-8-sig"))
        provenance.update(profile_id=manifest.get("profileId"), runtime_manifest_sha256=hashlib.sha256(MANIFEST.read_bytes()).hexdigest())
    (directory / "provenance.json").write_text(json.dumps(provenance, indent=2), encoding="utf-8")
    cases = [
        {"name": "fractional [safe name]", "rate": "24000/1001", "factor": 3, "mode": "boost"},
        {"name": "audio_boost", "audio": True, "factor": 2, "mode": "boost"},
        {"name": "audio_subtitles_slowmo", "audio": True, "subtitles": True, "factor": 3, "mode": "slowmo"},
        {"name": "subtitles_without_audio", "subtitles": True, "factor": 2, "mode": "boost"},
        {"name": "anamorphic", "sar": "4/3", "factor": 2, "mode": "boost"},
        {"name": "rotation", "factor": 2, "mode": "boost"},
        {"name": "variable_rate_normalized", "vfr": True, "factor": 2, "mode": "boost"},
        {"name": "cover_first_picture", "factor": 2, "mode": "boost"},
        {"name": "nvidia_360fps_boost", "rate": "360/1", "frames": 36, "factor": 2, "mode": "boost"},
        {"name": "multiple_audio_boost", "audio_tracks": 2, "factor": 2, "mode": "boost"},
        {"name": "multiple_audio_slowmo", "audio_tracks": 2, "factor": 3, "mode": "slowmo"},
        {"name": "rubberband_extreme_slowmo", "audio": True, "factor": 10, "mode": "slowmo"},
        {"name": "offset_video_boost", "audio": True, "video_offset": 2, "factor": 2, "mode": "boost"},
        {"name": "offset_video_subtitles", "audio": True, "subtitles": True, "video_offset": 2, "factor": 2, "mode": "boost"},
        {"name": "delayed_audio_slowmo", "audio": True, "audio_offset": 0.1, "factor": 2, "mode": "slowmo"},
        {"name": "mkv_short_video_long_audio", "audio": True, "container": "mkv", "audio_duration": 3, "frames": 12, "factor": 2, "mode": "boost"},
        {"name": "mkv_multiple_audio_titles", "audio_tracks": 2, "container": "mkv", "audio_duration": 3, "frames": 12, "factor": 2, "mode": "boost"},
        {"name": "unicode_\u8a66\u9a13_\U0001f3ac", "audio": True, "factor": 2, "mode": "boost"},
    ]
    results = []
    for case in cases:
        if selected_cases and case["name"] not in selected_cases:
            continue
        options = {k: v for k, v in case.items() if k not in ("factor", "mode")}
        source = generate_fixture(directory, **options)
        if case["name"] == "cover_first_picture":
            source = make_cover_first(directory, source)
        if case["name"] == "rotation":
            rotated = directory / "rotation90.mp4"
            run([FFMPEG, "-v", "error", "-display_rotation:v:0", "90", "-i", source, "-c", "copy", rotated])
            stream = next(s for s in probe(rotated)["streams"] if s["codec_type"] == "video")
            assert any(abs(float(side.get("rotation", 0))) == 90 for side in stream.get("side_data_list", [])), "Fixture is missing rotation metadata"
            source = rotated
        if not run_gpu:
            results.append({"case": case, "fixture": str(source), "status": "prepared"})
            continue
        legacy_rate = round(float(Fraction(selected_video(probe(source))["avg_frame_rate"])) * case["factor"])
        legacy_intermediate = source.with_name(f"{source.stem}_{case['factor']}X_{legacy_rate}fps.mp4")
        sentinel_bytes = b"preexisting-legacy-intermediate-do-not-touch"
        legacy_intermediate.write_bytes(sentinel_bytes)
        output = directory / f"{case['name']}-result.mp4"
        command = [PYTHON, "-I", "-X", "utf8", PIPELINE, "--video", source, "--mode", case["mode"],
                   "--factor", case["factor"], "--crf", "18", "--preset", "fast",
                   "--precision", PRECISION, "--encoder", ENCODER, "--device-index", DEVICE_INDEX, "--output", output,
                   "--ffmpeg", FFMPEG, "--ffprobe", FFPROBE, "--rife-dir", RIFE]
        if MODEL:
            command.extend(["--model-dir", MODEL])
        if MANIFEST:
            command.extend(["--runtime-manifest", MANIFEST])
        environment = dict(os.environ)
        if case["name"].startswith("unicode_"):
            foreign_packages = directory / "foreign-python-packages"
            foreign_packages.mkdir(exist_ok=True)
            (foreign_packages / "torch.py").write_text('raise RuntimeError("Foreign PYTHONPATH replaced managed Torch")\n', encoding="utf-8")
            environment.update(PYTHONHOME="C:/foreign/missing-python", PYTHONPATH=str(foreign_packages),
                               PYTHONIOENCODING="cp1252", PYTHONUTF8="0")
        started = time.perf_counter()
        process = run(command, check=False, env=environment)
        log = directory / f"{case['name']}.log"
        log.write_text(process.stdout + "\n" + process.stderr, encoding="utf-8")
        row = {"case": case, "seconds": time.perf_counter() - started, "returncode": process.returncode,
               "log": str(log), "status": "failed",
               "legacy_intermediate_preserved": legacy_intermediate.read_bytes() == sentinel_bytes}
        if process.returncode == 0:
            try:
                if not row["legacy_intermediate_preserved"]:
                    raise AssertionError("A preexisting historical RIFE intermediate was modified")
                row.update(check_output(source, output, factor=case["factor"], mode=case["mode"],
                                        dimensions=((192, 320) if ENCODER == "h264_nvenc" else (64, 96)) if case["name"] == "rotation" else None,
                                        sar=case.get("sar"), subtitles=case.get("subtitles", False)))
                if case.get("audio") or case.get("audio_tracks"):
                    row["audio_signal"] = check_audio_signal(output, case)
                row["status"] = "passed"
            except (AssertionError, RuntimeError) as error:
                row["error"] = str(error)
        else:
            row["error"] = process.stderr[-3000:]
        results.append(row)
        (directory / "results.json").write_text(json.dumps(results, indent=2), encoding="utf-8")
        print(json.dumps({k: row[k] for k in ("case", "seconds", "status")}), flush=True)
    # Existing output rejection and HDR rejection must occur before GPU loading.
    sentinel = directory / "sentinel-result.mp4"
    sentinel.write_bytes(b"sentinel-do-not-touch")
    source = generate_fixture(directory, "reject-source")
    base = [PYTHON, "-I", "-X", "utf8", PIPELINE, "--video", source, "--mode", "boost", "--factor", "2",
            "--ffmpeg", FFMPEG, "--ffprobe", FFPROBE, "--rife-dir", RIFE]
    if MODEL:
        base.extend(["--model-dir", MODEL])
    if MANIFEST:
        base.extend(["--runtime-manifest", MANIFEST])
    rejected = run([*base, "--output", sentinel], check=False)
    assert rejected.returncode != 0 and sentinel.read_bytes() == b"sentinel-do-not-touch"
    hdr = generate_fixture(directory, "reject-hdr", transfer="smpte2084")
    hdr_output = directory / "reject-hdr-result.mp4"
    hdr_base = list(base)
    hdr_base[hdr_base.index("--video") + 1] = hdr
    hdr_process = run([*hdr_base, "--output", hdr_output], check=False)
    assert hdr_process.returncode != 0 and "HDR" in hdr_process.stderr and not hdr_output.exists(), hdr_process.stderr
    results.extend([{"case": "existing-output-no-clobber", "status": "passed"},
                    {"case": "HDR-explicit-rejection", "status": "passed"}])
    (directory / "results.json").write_text(json.dumps(results, indent=2), encoding="utf-8")
    print(f"Integration artifacts: {directory}")
    if run_gpu and any(item["status"] == "failed" for item in results):
        raise SystemExit(1)


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--run", action="store_true", help="Explicitly enable CUDA interpolation fixtures")
    parser.add_argument("--cases", nargs="+", help="Run only selected fixture names")
    parser.add_argument("--python", type=Path, default=PYTHON)
    parser.add_argument("--ffmpeg", type=Path, default=FFMPEG)
    parser.add_argument("--ffprobe", type=Path, default=FFPROBE)
    parser.add_argument("--rife-dir", type=Path, default=RIFE)
    parser.add_argument("--model-dir", type=Path)
    parser.add_argument("--runtime-manifest", type=Path)
    parser.add_argument("--device-index", type=int, default=0)
    parser.add_argument("--precision", choices=("fp32", "fp16"), default="fp32")
    parser.add_argument("--encoder", choices=("libx264", "h264_nvenc"), default="libx264")
    parser.add_argument("--output-dir", type=Path)
    args = parser.parse_args()
    PYTHON, FFMPEG, FFPROBE, RIFE, MODEL = args.python, args.ffmpeg, args.ffprobe, args.rife_dir, args.model_dir
    MANIFEST, DEVICE_INDEX, PRECISION, ENCODER = args.runtime_manifest, args.device_index, args.precision, args.encoder
    suite = unittest.defaultTestLoader.loadTestsFromTestCase(UnitContractTests)
    result = unittest.TextTestRunner(verbosity=2).run(suite)
    if not result.wasSuccessful():
        raise SystemExit(1)
    integration(args.run, args.cases, args.output_dir)
