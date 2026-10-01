"""Small end-to-end RIFE fixtures. GPU tests run only with explicit --run.

Uses unique workspace-owned files; real clips/models are never modified.
Unit checks and FFmpeg fixture generation require no GPU. Real interpolation
checks cover fractional FPS, odd factors, audio, subtitles, rotation and SAR.
"""
import argparse
from fractions import Fraction
import importlib.util
import json
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


def load_pipeline():
    spec = importlib.util.spec_from_file_location("time_remap", PIPELINE)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def run(command, *, check=True, timeout=120):
    result = subprocess.run([str(part) for part in command], capture_output=True,
                            encoding="utf-8", errors="replace", timeout=timeout)
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


def audio_packet_hashes(path):
    result = run([FFPROBE, "-v", "error", "-select_streams", "a:0", "-show_packets",
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

    def test_tempo_chain_matches_requested_slowdown(self):
        for factor in (2, 3, 10):
            filters = self.module.build_atempo_filter(1 / factor)
            product = 1.0
            for item in filters.split(","):
                value = float(item.split("=")[1])
                self.assertGreaterEqual(value, 0.5)
                self.assertLessEqual(value, 2.0)
                product *= value
            self.assertAlmostEqual(product, 1 / factor, places=8)


def generate_fixture(directory, name, *, rate="24/1", frames=6, audio=False,
                     subtitles=False, sar="1/1", transfer=None, vfr=False):
    destination = directory / f"{name}.mp4"
    duration = float(Fraction(frames) / Fraction(rate))
    command = [FFMPEG, "-v", "error", "-f", "lavfi", "-i",
               f"testsrc2=size=96x64:rate={rate}"]
    if audio:
        command.extend(["-f", "lavfi", "-i", "sine=frequency=400:sample_rate=48000"])
    if subtitles:
        subtitle = directory / f"{name}.srt"
        subtitle.write_text("1\n00:00:00,050 --> 00:00:00,150\nRetime this cue\n", encoding="utf-8")
        command.extend(["-i", str(subtitle)])
    command.extend(["-map", "0:v:0", "-frames:v", str(frames)])
    if not vfr:
        command.extend(["-t", str(duration)])
    filters = f"setsar={sar}"
    if vfr:
        filters += ",setpts=PTS+if(gte(N\\,3)\\,2/(24*TB)\\,0)"
    if transfer:
        filters += f",setparams=color_primaries=bt2020:color_trc={transfer}:colorspace=bt2020nc"
    command.extend(["-vf", filters, "-c:v", "libx264", "-crf", "18", "-preset", "fast"])
    if vfr:
        command.extend(["-fps_mode", "vfr"])
    if audio:
        command.extend(["-map", "1:a:0", "-c:a", "aac", "-b:a", "192k"])
    if subtitles:
        command.extend(["-map", f"{2 if audio else 1}:s:0", "-c:s", "mov_text",
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
    if bool(first_audio) != bool(final_audio):
        raise AssertionError("Audio presence changed")
    if first_audio:
        expected_audio = float(first_audio[0]["duration"]) * (factor if mode == "slowmo" else 1)
        if abs(float(final_audio[0]["duration"]) - expected_audio) > 0.12:
            raise AssertionError("Audio retiming outside 120-ms tolerance")
        if mode == "boost" and first_audio[0]["codec_name"] == "aac":
            if audio_packet_hashes(source) != audio_packet_hashes(output):
                raise AssertionError("Compatible AAC boost audio was re-encoded or lost packets")
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
            "aac_packets_identical": bool(first_audio and mode == "boost" and first_audio[0]["codec_name"] == "aac")}


def integration(run_gpu, selected_cases=None):
    directory = APP / "audit-benchmark" / f"integration-{time.time_ns()}"
    directory.mkdir(parents=True, exist_ok=False)
    cases = [
        {"name": "fractional [safe name]", "rate": "24000/1001", "factor": 3, "mode": "boost"},
        {"name": "audio_boost", "audio": True, "factor": 2, "mode": "boost"},
        {"name": "audio_subtitles_slowmo", "audio": True, "subtitles": True, "factor": 3, "mode": "slowmo"},
        {"name": "subtitles_without_audio", "subtitles": True, "factor": 2, "mode": "boost"},
        {"name": "anamorphic", "sar": "4/3", "factor": 2, "mode": "boost"},
        {"name": "rotation", "factor": 2, "mode": "boost"},
        {"name": "variable_rate_normalized", "vfr": True, "factor": 2, "mode": "boost"},
        {"name": "cover_first_picture", "factor": 2, "mode": "boost"},
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
        command = [PYTHON, PIPELINE, "--video", source, "--mode", case["mode"],
                   "--factor", case["factor"], "--crf", "18", "--preset", "fast",
                   "--precision", "fp32", "--encoder", "libx264", "--output", output,
                   "--ffmpeg", FFMPEG, "--ffprobe", FFPROBE, "--rife-dir", RIFE]
        started = time.perf_counter()
        process = run(command, check=False)
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
                                        dimensions=(64, 96) if case["name"] == "rotation" else None,
                                        sar=case.get("sar"), subtitles=case.get("subtitles", False)))
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
    base = [PYTHON, PIPELINE, "--video", source, "--mode", "boost", "--factor", "2",
            "--ffmpeg", FFMPEG, "--ffprobe", FFPROBE, "--rife-dir", RIFE]
    rejected = run([*base, "--output", sentinel], check=False)
    assert rejected.returncode != 0 and sentinel.read_bytes() == b"sentinel-do-not-touch"
    hdr = generate_fixture(directory, "reject-hdr", transfer="smpte2084")
    hdr_output = directory / "reject-hdr-result.mp4"
    hdr_process = run([*base[:2], "--video", hdr, *base[4:], "--output", hdr_output], check=False)
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
    args = parser.parse_args()
    suite = unittest.defaultTestLoader.loadTestsFromTestCase(UnitContractTests)
    result = unittest.TextTestRunner(verbosity=2).run(suite)
    if not result.wasSuccessful():
        raise SystemExit(1)
    integration(args.run, args.cases)
