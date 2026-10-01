"""Paired, end-to-end RIFE benchmark; GPU execution requires --run.

Run with the application venv's Python. Artifacts are retained in a unique
workspace directory. The frozen legacy script only receives sources owned by
this benchmark, because its default intermediate naming can overwrite files.
Results measure whole subprocess lifetime, including model loading, decode,
interpolation, audio and final encode. CRF and NVENC CQ are not equivalent.
"""
from __future__ import annotations

import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import statistics
import subprocess
import time
from fractions import Fraction
from datetime import datetime, timezone


UI_DEFAULT = Path(__file__).resolve().parents[1]
APP_DEFAULT = UI_DEFAULT.parent / "time-remap-app"


def sha256(path):
    digest = hashlib.sha256()
    with Path(path).open("rb") as source:
        for part in iter(lambda: source.read(1024 * 1024), b""):
            digest.update(part)
    return digest.hexdigest()


def source_hashes(directory):
    return {str(p.relative_to(directory)): sha256(p)
            for p in sorted(Path(directory).rglob("*.py"))}


def execute(args, *, cwd=None, env=None, timeout=240):
    result = subprocess.run([str(a) for a in args], cwd=cwd, env=env,
                            capture_output=True, timeout=timeout)
    if result.returncode:
        raise RuntimeError(f"Command exited {result.returncode}: {args}\n"
                           f"{result.stderr.decode('utf-8', 'replace')[-4000:]}")
    return result


def write_json(path, result):
    Path(path).write_text(json.dumps(result, indent=2, ensure_ascii=False) + "\n",
                          encoding="utf-8")


def freeze_baseline(ui, app, rife):
    current_revision = execute(["git", "rev-parse", "HEAD"], cwd=ui).stdout.decode().strip()
    manifest = ui / "docs/BASELINE-PATCH.json"
    # Re-running after the patch is committed must retain the pre-patch source.
    revision = (json.loads(manifest.read_text(encoding="utf-8"))["baseline_revision"]
                if manifest.exists() else current_revision)
    snapshot = app / "audit-baseline" / "time_remap.py"
    snapshot.parent.mkdir(parents=True, exist_ok=True)
    contents = execute(["git", "show", f"{revision}:src-tauri/resources/time_remap.py"],
                       cwd=ui).stdout
    # Freeze the original manifest revision (or initial HEAD), never the edited
    # working copy or the post-patch commit when this harness is run again.
    if snapshot.exists() and snapshot.read_bytes() != contents:
        raise RuntimeError("Existing baseline differs from current HEAD; retain it and choose a new baseline explicitly")
    snapshot.write_bytes(contents)
    model_files = list((rife / "train_log").glob("*.pkl"))
    metadata = {
        "baseline_revision": revision,
        "observed_worktree_revision": current_revision,
        "baseline_script": str(snapshot),
        "baseline_script_sha256": sha256(snapshot),
        "legacy_inference_sha256": sha256(rife / "inference_video.py"),
        "model_weights": {p.name: sha256(p) for p in model_files},
        "benchmark_contract": {
            "matched": "Legacy FP32/libx264 vs patch FP32/libx264, same factor, source, CRF and preset",
            "paired_order": "Alternate AB then BA across repetitions",
            "timing": "Whole process wall time including Python startup/model loading, audio and encoding",
            "endpoint_policy": "Legacy usually (N-1)*factor+1; patch N*factor with final-frame hold",
            "quality": "Decoded full-resolution SSIM/PSNR on common frame indices; differences expected from removing legacy lossy MPEG-4 intermediate",
            "optional_profiles": "Patch FP16/x264 and FP16/NVENC, compared against patch FP32/x264; CQ 18 does not equal CRF 18",
            "gpu_guard": "GPU execution occurs only with explicit --run",
        },
        "prior_audit_review": [
            "Prior numbers measured separate components, not application end-to-end gain; do not sum them.",
            "Eight-preview-process estimate omitted local luminance scoring; measure the complete replacement before claiming 51%.",
            "FP16 and NVENC speed figures lacked quality comparisons. FP16 needs an independent same-pipeline FP32 reference.",
            "Original Smoothie unmatched filter measurements are unsuitable for speed comparisons; only *_matched values are usable.",
            "Two RIFE runs and short clips include warm-up/model load and do not prove sustained long-video throughput.",
            "Legacy sample had no audio; end-to-end application comparisons must include audio and verify stream duration.",
            "Legacy mp4v intermediate changes visual fidelity. A faster raw-frame patch is a pipeline improvement, not quality-equivalent byte output.",
        ],
    }
    return snapshot, metadata


def probe(path, ffprobe, env):
    result = execute([ffprobe, "-v", "error", "-count_frames", "-show_streams",
                      "-show_format", "-of", "json", path], env=env)
    data = json.loads(result.stdout)
    fields = ("codec_type", "codec_name", "width", "height", "r_frame_rate",
              "avg_frame_rate", "duration", "nb_read_frames", "sample_rate",
              "channels", "color_space", "color_transfer", "color_primaries")
    return {"streams": [{k: s[k] for k in fields if k in s}
                        for s in data.get("streams", [])],
            "format": {k: data["format"][k] for k in ("duration", "size")
                       if k in data.get("format", {})}}


def video_stream(info):
    return next(s for s in info["streams"] if s["codec_type"] == "video")


def integrity(output, source_info, *, factor, mode, legacy):
    source = video_stream(source_info)
    video = video_stream(output)
    frames = int(source["nb_read_frames"])
    rate = Fraction(source["avg_frame_rate"])
    target_rate = rate * factor if mode == "boost" else rate
    # Legacy explicitly rounds output FPS and may expose the historical writer
    # completion race; preserve/report raw measurements rather than hide it.
    if legacy:
        target_rate = Fraction(round(float(target_rate)))
    expected = (frames - 1) * factor + 1 if legacy else frames * factor
    count = int(video["nb_read_frames"])
    actual_rate = Fraction(video["avg_frame_rate"])
    video_duration = float(video.get("duration", count / float(actual_rate)))
    expected_duration = expected / float(target_rate)
    audio = [s for s in output["streams"] if s["codec_type"] == "audio"]
    expected_audio = any(s["codec_type"] == "audio" for s in source_info["streams"])
    errors = []
    if (video.get("width"), video.get("height")) != (source["width"], source["height"]):
        errors.append("dimensions changed")
    if count != expected:
        errors.append(f"frame count {count}, expected {expected}")
    if abs(float(actual_rate) - float(target_rate)) > 1e-5:
        errors.append(f"rate {actual_rate}, expected {target_rate}")
    if abs(video_duration - expected_duration) > 1.01 / float(target_rate):
        errors.append("video duration outside one-frame tolerance")
    if bool(audio) != expected_audio:
        errors.append("audio presence changed")
    expected_audio_duration = float(source.get("duration", frames / float(rate)))
    if mode == "slowmo":
        expected_audio_duration *= factor
    for stream in audio:
        if "duration" in stream and abs(float(stream["duration"]) - expected_audio_duration) > max(0.12, 2 / float(rate)):
            errors.append("audio duration outside 120-ms/two-source-frame tolerance")
    return {"passed": not errors, "errors": errors, "frames": count,
            "expected_frames": expected, "video_duration": video_duration,
            "expected_video_duration": expected_duration,
            "expected_audio_duration": expected_audio_duration}


def decode_check(output, ffmpeg, env):
    execute([ffmpeg, "-v", "error", "-xerror", "-i", output,
             "-map", "0:v:0", "-map", "0:a?", "-f", "null", "-"], env=env)


def compare_videos(first, second, first_info, second_info, ffmpeg, env):
    first_video, second_video = video_stream(first_info), video_stream(second_info)
    if (first_video["width"], first_video["height"]) != (second_video["width"], second_video["height"]):
        return {"error": "dimensions differ; SSIM/PSNR skipped"}
    common = min(int(first_video["nb_read_frames"]), int(second_video["nb_read_frames"]))
    metrics = {"common_frames": common,
               "interpretation": "Decoded output agreement; neither output is ground-truth interpolation"}
    for metric, regex in (("ssim", r"All:([\d.]+)"), ("psnr", r"average:([\d.]+|inf)")):
        graph = (f"[0:v]trim=end_frame={common},settb=AVTB,setpts=N/(30*TB)[a];"
                 f"[1:v]trim=end_frame={common},settb=AVTB,setpts=N/(30*TB)[b];"
                 f"[a][b]{metric}=shortest=1:repeatlast=0")
        result = execute([ffmpeg, "-hide_banner", "-i", first, "-i", second,
                          "-filter_complex", graph, "-an", "-f", "null", "-"], env=env)
        matches = re.findall(regex, result.stderr.decode("utf-8", "replace"))
        if not matches:
            raise RuntimeError(f"Missing {metric} summary")
        metrics[metric] = float(matches[-1]) if matches[-1] != "inf" else "infinity"
    return metrics


def timed_pipeline(command, *, cwd, env, log, timeout):
    start = time.perf_counter()
    with Path(log).open("wb") as destination:
        process = subprocess.Popen([str(a) for a in command], cwd=cwd, env=env,
                                   stdout=destination, stderr=subprocess.STDOUT)
        try:
            code = process.wait(timeout=timeout)
        except subprocess.TimeoutExpired:
            # Windows taskkill terminates this benchmark's known process tree.
            if os.name == "nt":
                subprocess.run(["taskkill", "/PID", str(process.pid), "/T", "/F"],
                               capture_output=True, check=False)
            else:
                process.kill()
            process.wait()
            raise
    elapsed = time.perf_counter() - start
    if code:
        tail = Path(log).read_text(encoding="utf-8", errors="replace")[-4000:]
        raise RuntimeError(f"Pipeline exited {code}: {tail}")
    return elapsed


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--ui", type=Path, default=UI_DEFAULT)
    parser.add_argument("--app", type=Path, default=APP_DEFAULT)
    parser.add_argument("--source", type=Path)
    parser.add_argument("--rife-dir", type=Path)
    parser.add_argument("--python", type=Path)
    parser.add_argument("--repeats", type=int, default=3)
    parser.add_argument("--frames", type=int, nargs="+", default=[48, 120])
    parser.add_argument("--modes", nargs="+", choices=["boost", "slowmo"], default=["boost"])
    parser.add_argument("--factor", type=int, default=2)
    parser.add_argument("--crf", type=int, default=18)
    parser.add_argument("--preset", default="fast")
    parser.add_argument("--profiles", action="store_true", help="Also benchmark patch FP16 and NVENC")
    parser.add_argument("--timeout", type=float, default=240)
    parser.add_argument("--label", default="paired", help="Record whether this series is final or exploratory")
    parser.add_argument("--run", action="store_true", help="Explicitly authorize GPU workloads")
    args = parser.parse_args()
    if args.repeats < 1 or args.factor not in range(2, 11) or min(args.frames) < 2:
        parser.error("repeats >= 1, factor 2..10, frames >= 2 required")
    ui, app = args.ui.resolve(), args.app.resolve()
    rife = (args.rife_dir or app / "Practical-RIFE").resolve()
    source = (args.source or app / "test_clip_topaz_source_2s.mp4").resolve()
    python = (args.python or app / "venv/Scripts/python.exe").resolve()
    ffmpeg = ui / "src-tauri/resources/runtime/ffmpeg/ffmpeg.exe"
    ffprobe = ffmpeg.with_name("ffprobe.exe")
    patch = ui / "src-tauri/resources/time_remap.py"
    baseline, metadata = freeze_baseline(ui, app, rife)
    metadata.update({"source": str(source), "source_sha256": sha256(source),
                     "python": str(python), "ffmpeg": str(ffmpeg),
                     "ffmpeg_sha256": sha256(ffmpeg)})
    write_json(ui / "docs/BASELINE-PATCH.json", metadata)
    if not args.run:
        print("Baseline frozen. No GPU workload started. Use --run to execute benchmark.")
        return
    stamp = datetime.now(timezone.utc).strftime("%Y%m%dT%H%M%S")
    artifacts = app / "audit-benchmark" / f"{stamp}-{os.getpid()}"
    artifacts.mkdir(parents=True, exist_ok=False)
    # Legacy inference's internal audio transfer deletes cwd/temp, even though
    # it subsequently fails on an undefined variable. Isolate its entire cwd
    # rather than trusting safe source naming alone. Copy only its dependencies.
    owned_rife = artifacts / "owned-rife"
    owned_rife.mkdir()
    shutil.copy2(rife / "inference_video.py", owned_rife / "inference_video.py")
    for component in ("model", "train_log"):
        shutil.copytree(rife / component, owned_rife / component,
                        ignore=shutil.ignore_patterns("__pycache__", "*.pyc"))
    frozen_patch_directory = artifacts / "patch-source"
    frozen_patch_directory.mkdir()
    for filename in ("time_remap.py", "rife_worker.py"):
        shutil.copy2(patch.with_name(filename), frozen_patch_directory / filename)
    frozen_patch = frozen_patch_directory / "time_remap.py"
    environment = os.environ.copy()
    environment["PATH"] = str(ffmpeg.parent) + os.pathsep + environment.get("PATH", "")
    environment["Path"] = environment["PATH"]
    report = {**metadata, "date_utc": datetime.now(timezone.utc).isoformat(),
              "label": args.label,
              "patch_script_sha256": sha256(frozen_patch),
              "patch_worker_sha256": sha256(frozen_patch.with_name("rife_worker.py")),
              "tested_patch_snapshot": str(frozen_patch_directory),
              "runtime_model_code_sha256": source_hashes(owned_rife),
              "artifacts": str(artifacts),
              "owned_rife_copy": str(owned_rife),
              "repetitions": args.repeats, "measurements": [], "comparisons": [],
              "caveats": ["Short local samples; model-load overhead included; no long-video throughput claim",
                          "No thermal control; alternating order reduces but does not eliminate cache/thermal bias",
                          "CQ and CRF do not imply equal quality; NVENC may produce a larger file",
                          "SSIM/PSNR measure output agreement, not correctness or subjective interpolation quality"]}
    try:
        report["gpu"] = execute(["nvidia-smi", "--query-gpu=name,memory.total,driver_version",
                                 "--format=csv,noheader"], env=environment).stdout.decode().strip()
    except (OSError, RuntimeError):
        report["gpu"] = "unavailable"
    report_path = artifacts / "results.json"
    write_json(report_path, report)
    full_source = probe(source, ffprobe, environment)
    report["source_info"] = full_source
    source_rate = Fraction(video_stream(full_source)["avg_frame_rate"])
    for frames in args.frames:
        clip = artifacts / f"owned_source_{frames}.mp4"
        # All benchmark legacy sources have unique stems and live in this owned
        # directory. Never let legacy inference touch the real source's names.
        execute([ffmpeg, "-v", "error", "-i", source, "-frames:v", frames,
                 "-t", str(float(Fraction(frames, 1) / source_rate)),
                 "-map", "0:v:0", "-map", "0:a?", "-c:v", "libx264", "-crf", "18",
                 "-preset", "fast", "-c:a", "aac", "-b:a", "192k", clip], env=environment)
        input_info = probe(clip, ffprobe, environment)
        if int(video_stream(input_info)["nb_read_frames"]) != frames:
            raise RuntimeError("Requested benchmark clip extends beyond available source frames")
        for mode in args.modes:
            variants = {"legacy_fp32_x264": (baseline, []),
                        "patch_fp32_x264": (frozen_patch, ["--precision", "fp32", "--encoder", "libx264"])}
            if args.profiles:
                variants.update({"patch_fp16_x264": (frozen_patch, ["--precision", "fp16", "--encoder", "libx264"]),
                                 "patch_fp16_nvenc": (frozen_patch, ["--precision", "fp16", "--encoder", "h264_nvenc"])})
            retained = {}
            for repetition in range(args.repeats):
                names = list(variants)
                if repetition % 2:
                    names.reverse()
                for name in names:
                    script, extra = variants[name]
                    output = artifacts / f"{frames}_{mode}_{name}_{repetition}.mp4"
                    log = output.with_suffix(".log")
                    command = [python, script, "--video", clip, "--mode", mode,
                               "--factor", args.factor, "--crf", args.crf,
                               "--preset", args.preset, "--scene_threshold", "0.05",
                               "--output", output, "--ffmpeg", ffmpeg, "--ffprobe", ffprobe,
                               "--rife-dir", owned_rife, *extra]
                    elapsed = timed_pipeline(command, cwd=artifacts, env=environment,
                                             log=log, timeout=args.timeout)
                    info = probe(output, ffprobe, environment)
                    validation = integrity(info, input_info, factor=args.factor,
                                           mode=mode, legacy=name.startswith("legacy"))
                    decode_check(output, ffmpeg, environment)
                    row = {"frames": frames, "mode": mode, "variant": name,
                           "repetition": repetition, "seconds": elapsed,
                           "output_bytes": output.stat().st_size, "output_info": info,
                           "integrity": validation, "log": str(log), "command": [str(a) for a in command]}
                    report["measurements"].append(row)
                    retained[name] = (output, info)
                    write_json(report_path, report)
                    print(json.dumps({k: row[k] for k in ("frames", "mode", "variant", "repetition", "seconds", "integrity")}), flush=True)
            for name in list(variants)[1:]:
                reference = "legacy_fp32_x264" if name == "patch_fp32_x264" else "patch_fp32_x264"
                first, first_info = retained[reference]
                second, second_info = retained[name]
                report["comparisons"].append({"frames": frames, "mode": mode,
                                              "reference": reference, "candidate": name,
                                              **compare_videos(first, second, first_info, second_info, ffmpeg, environment)})
            rows = [r for r in report["measurements"] if r["frames"] == frames and r["mode"] == mode]
            medians = {name: statistics.median(r["seconds"] for r in rows if r["variant"] == name)
                       for name in variants}
            baseline_seconds = medians["legacy_fp32_x264"]
            report.setdefault("summary", []).append({"frames": frames, "mode": mode,
                "median_seconds": medians,
                "time_reduction_percent_vs_legacy": {name: 100 * (1 - seconds / baseline_seconds)
                                                      for name, seconds in medians.items()},
                "speedup_vs_legacy": {name: baseline_seconds / seconds for name, seconds in medians.items()}})
            write_json(report_path, report)
    print(f"Results and retained videos: {report_path}")


if __name__ == "__main__":
    main()
