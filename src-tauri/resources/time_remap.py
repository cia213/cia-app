import argparse
import json
import os
import subprocess
import sys
import time


def run_command(args, cwd=None):
    result = subprocess.run(args, capture_output=True, text=True, shell=False, cwd=cwd)
    if result.returncode != 0:
        raise RuntimeError(f"Command failed: {args}\nError: {result.stderr}")
    return result.stdout


def get_video_info(video_path, ffprobe_exe):
    command = [
        ffprobe_exe, "-v", "quiet", "-print_format", "json",
        "-show_streams", "-show_format", video_path,
    ]
    data = json.loads(run_command(command))
    video = next(stream for stream in data["streams"] if stream["codec_type"] == "video")
    audio = next((stream for stream in data["streams"] if stream["codec_type"] == "audio"), None)
    subtitles = next((stream for stream in data["streams"] if stream["codec_type"] == "subtitle"), None)
    numerator, denominator = map(float, video["r_frame_rate"].split("/"))
    fps = numerator / denominator if denominator else 30.0
    return {
        "fps": fps,
        "width": int(video["width"]),
        "height": int(video["height"]),
        "duration": float(data["format"]["duration"]),
        "has_audio": audio is not None,
        "has_subtitles": subtitles is not None,
    }


def detect_scene_changes(video_path, threshold, ffmpeg_exe):
    command = [
        ffmpeg_exe, "-hide_banner", "-i", video_path,
        "-filter_complex", f"select='gt(scene,{threshold})',showinfo",
        "-f", "null", "-",
    ]
    result = subprocess.run(command, capture_output=True, text=True, shell=False)
    timestamps = []
    for line in result.stderr.splitlines():
        if "showinfo" in line and "pts_time:" in line:
            for part in line.split():
                if part.startswith("pts_time:"):
                    timestamps.append(float(part.split(":")[1]))
    return sorted(set(timestamps))


def build_atempo_filter(speed):
    filters = []
    current = speed
    while current < 0.5:
        filters.append("atempo=0.5")
        current /= 0.5
    while current > 2.0:
        filters.append("atempo=2.0")
        current /= 2.0
    filters.append(f"atempo={current:.6f}")
    return ",".join(filters)


def process_time_remap(video_path, mode="slowmo", factor=2.0, scene_threshold=0.05,
                       blend_cuts=0, crf=18, preset="fast", output_path=None,
                       ffmpeg_exe="ffmpeg", ffprobe_exe="ffprobe", rife_dir=None):
    source = os.path.abspath(video_path)
    if not os.path.isfile(source):
        raise FileNotFoundError(f"Video not found: {source}")
    if output_path is None:
        raise ValueError("--output is required; the caller owns output naming")

    info = get_video_info(source, ffprobe_exe)
    input_fps = info["fps"]
    interpolation_factor = int(factor) if factor.is_integer() else factor
    if mode == "boost":
        output_fps = round(input_fps * factor)
        target_duration = info["duration"]
    else:
        output_fps = round(input_fps)
        target_duration = info["duration"] * factor

    final_output = os.path.abspath(output_path)
    output_directory = os.path.dirname(final_output)
    base_name = os.path.splitext(os.path.basename(source))[0]
    rife_directory = os.path.abspath(
        rife_dir or os.path.join(os.path.dirname(__file__), "Practical-RIFE")
    )
    python = sys.executable

    print("=== TIME-REMAP PIPELINE (RIFE 4.26) ===")
    print(f"Input Video     : {source}")
    print(f"Input Specs     : {info['width']}x{info['height']} @ {input_fps:.2f} FPS ({info['duration']:.2f}s)")
    print(f"Pipeline Mode   : {mode.upper()} ({interpolation_factor}x)")
    print(f"Output Specs    : {output_fps} FPS (Target duration: {target_duration:.2f}s)")
    cuts = detect_scene_changes(source, scene_threshold, ffmpeg_exe)
    print(f"Scene Detection : {len(cuts)} scene cuts detected (Threshold: {scene_threshold}, Blend cuts: {blend_cuts})")

    started = time.time()
    print("\n[*] Running RIFE 4.26 Frame Interpolation...")
    rife_command = [
        python, os.path.join(rife_directory, "inference_video.py"),
        "--video", source, "--multi", str(interpolation_factor),
    ]
    subprocess.run(rife_command, shell=False, cwd=rife_directory, check=True)

    raw_interpolation = os.path.join(
        output_directory, f"{base_name}_{interpolation_factor}X_{round(input_fps * factor)}fps.mp4"
    )
    if not os.path.exists(raw_interpolation):
        raw_interpolation = os.path.join(
            rife_directory, f"{base_name}_{interpolation_factor}X_{round(input_fps * factor)}fps.mp4"
        )
    if not os.path.isfile(raw_interpolation):
        raise FileNotFoundError(f"Raw RIFE output missing: {raw_interpolation}")

    print(f"\n[*] Finalizing output with FFmpeg (CRF {crf}, Preset {preset})...")
    command = [ffmpeg_exe, "-y", "-i", raw_interpolation]
    video_filter = []
    audio_flags = []
    subtitle_flags = []
    if info["has_audio"] or info["has_subtitles"]:
        command.extend(["-i", source])

    command.extend(["-map", "0:v:0"])
    if info["has_audio"]:
        command.extend(["-map", "1:a:0?"])
        if mode == "slowmo":
            audio_flags = ["-c:a", "aac", "-b:a", "192k", "-filter:a", build_atempo_filter(1.0 / factor)]
            video_filter = ["-vf", f"setpts={factor}*PTS"]
        else:
            audio_flags = ["-c:a", "copy"]
    elif mode == "slowmo":
        video_filter = ["-vf", f"setpts={factor}*PTS"]

    if info["has_subtitles"]:
        command.extend(["-map", "1:s?"])
        subtitle_flags = ["-c:s", "copy"]

    command.extend([
        "-c:v", "libx264", "-crf", str(crf), "-preset", preset,
        "-pix_fmt", "yuv420p", "-color_primaries", "bt709",
        "-color_trc", "bt709", "-colorspace", "bt709",
        "-x264opts", "colorprim=bt709:transfer=bt709:colormatrix=bt709",
    ])
    command.extend(video_filter)
    command.extend(["-r", str(output_fps)])
    command.extend(audio_flags)
    command.extend(subtitle_flags)
    command.extend(["-map_metadata", "-1", final_output])
    run_command(command)
    if os.path.exists(raw_interpolation):
        os.remove(raw_interpolation)

    elapsed = time.time() - started
    print(f"\n[OK] COMPLETE: Output saved to {final_output} (Processing time: {elapsed:.1f}s)")
    return final_output


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description="cia app RIFE orchestration")
    parser.add_argument("--video", required=True)
    parser.add_argument("--mode", choices=["slowmo", "boost"], default="slowmo")
    parser.add_argument("--factor", type=float, default=2.0)
    parser.add_argument("--scene_threshold", type=float, default=0.05)
    parser.add_argument("--blend-cuts", type=int, default=0)
    parser.add_argument("--crf", type=int, default=18)
    parser.add_argument("--preset", default="fast")
    parser.add_argument("--output", required=True)
    parser.add_argument("--ffmpeg", required=True)
    parser.add_argument("--ffprobe", required=True)
    parser.add_argument("--rife-dir", required=True)
    args = parser.parse_args()
    process_time_remap(
        video_path=args.video,
        mode=args.mode,
        factor=args.factor,
        scene_threshold=args.scene_threshold,
        blend_cuts=args.blend_cuts,
        crf=args.crf,
        preset=args.preset,
        output_path=args.output,
        ffmpeg_exe=args.ffmpeg,
        ffprobe_exe=args.ffprobe,
        rife_dir=args.rife_dir,
    )
