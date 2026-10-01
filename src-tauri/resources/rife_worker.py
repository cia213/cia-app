"""Bounded RIFE inference worker. stdout contains RGB frames; stderr is telemetry.

Uses the user's Practical-RIFE model/code without modifying that installation.
Scene/static-frame behavior follows Practical-RIFE's video inference algorithm.
"""
import argparse
from collections import deque
import importlib
import os
from pathlib import Path
from queue import Queue, Empty, Full
import subprocess
import sys
import threading
import time
import types


def read_frame(pipe, size):
    chunks = bytearray()
    while len(chunks) < size:
        chunk = pipe.read(size - len(chunks))
        if not chunk:
            if chunks:
                raise RuntimeError("Decoder returned an incomplete RGB frame")
            return None
        chunks.extend(chunk)
    return bytes(chunks)


def load_model_weights(model, torch, model_dir):
    weights = torch.load(str(Path(model_dir) / "flownet.pkl"), map_location="cpu", weights_only=True)
    if not isinstance(weights, dict) or not weights:
        raise RuntimeError("The RIFE checkpoint must contain a nonempty state dictionary")
    normalized = {}
    for key, value in weights.items():
        if not isinstance(key, str):
            raise RuntimeError("Invalid RIFE checkpoint key")
        key = key.removeprefix("module.")
        if key in normalized:
            raise RuntimeError("The RIFE checkpoint contains duplicate parameter keys")
        normalized[key] = value
    # Upstream load_model(rank=-1, strict=False) can silently discard plain keys
    # or accept missing tensors. Validate the complete configured network.
    network_keys = set(model.flownet.state_dict())
    # Official checkpoints also contain training-only teacher/time-estimator
    # tensors that the inference network intentionally does not instantiate.
    normalized = {key: value for key, value in normalized.items()
                  if key in network_keys or not key.startswith(("teacher.", "caltime."))}
    model.flownet.load_state_dict(normalized, strict=True)


def main(args):
    # Avoid the Windows CRT transforming binary stdout newline bytes.
    if os.name == "nt":
        import msvcrt
        msvcrt.setmode(sys.stdout.fileno(), os.O_BINARY)
    sys.path.insert(0, str(Path(args.rife_dir).resolve()))
    package = types.ModuleType("train_log")
    package.__path__ = [str(Path(args.model_dir).resolve())]
    sys.modules["train_log"] = package

    import numpy as np
    import torch
    from torch.nn import functional as F
    from model.pytorch_msssim import ssim_matlab

    if not torch.cuda.is_available():
        raise RuntimeError("RIFE requires a CUDA-capable NVIDIA GPU")
    torch.set_grad_enabled(False)
    torch.backends.cudnn.enabled = True
    torch.backends.cudnn.benchmark = True
    device = torch.device("cuda")
    if args.precision == "fp16":
        torch.set_default_dtype(torch.float16)
    # Some model versions print during construction/loading: keep RGB stdout clean.
    binary_stdout = sys.stdout.buffer
    original_stdout = sys.stdout
    sys.stdout = sys.stderr
    try:
        Model = importlib.import_module("train_log.RIFE_HDv3").Model
        model = Model()
        if getattr(model, "version", 0) < 3.9:
            raise RuntimeError("This worker requires a Practical-RIFE 4.x timestep model")
        load_model_weights(model, torch, args.model_dir)
        model.eval()
        model.device()
    finally:
        sys.stdout = original_stdout

    width, height = args.width, args.height
    size = width * height * 3
    # Total prefetched source frames <= 64 MiB, apart from current tensors.
    capacity = max(1, min(8, (64 * 1024 * 1024) // size))
    queue = Queue(maxsize=capacity)
    stop = threading.Event()
    errors = []
    decoder_logs = deque(maxlen=32)
    decoder = subprocess.Popen(
        [args.ffmpeg, "-v", "error", "-xerror", "-err_detect", "explode", "-i", args.video, "-map", f"0:{args.video_index}",
         "-an", "-vf", f"fps={args.fps}", "-f", "rawvideo", "-pix_fmt", "rgb24", "pipe:1"],
        stdout=subprocess.PIPE, stderr=subprocess.PIPE,
    )

    def put(frame):
        while not stop.is_set():
            try:
                queue.put(frame, timeout=0.1)
                return
            except Full:
                continue

    def decode():
        try:
            while not stop.is_set():
                frame = read_frame(decoder.stdout, size)
                if frame is None:
                    break
                put(np.frombuffer(frame, dtype=np.uint8).reshape(height, width, 3).copy())
            code = decoder.wait()
            if code and not stop.is_set():
                raise RuntimeError(f"Video decoder failed ({code}): {''.join(decoder_logs)}")
        except Exception as exc:
            errors.append(exc)
        finally:
            put(None)

    def drain_decoder_logs():
        for line in iter(decoder.stderr.readline, b""):
            decoder_logs.append(line.decode("utf-8", "replace"))

    log_thread = threading.Thread(target=drain_decoder_logs, daemon=True)
    read_thread = threading.Thread(target=decode, daemon=True)
    log_thread.start()
    read_thread.start()
    input_count = 0
    output_count = 0
    started = time.monotonic()
    last_log = started

    def get():
        nonlocal input_count
        while True:
            try:
                frame = queue.get(timeout=0.2)
                if frame is None:
                    if errors:
                        raise errors[0]
                    return None
                input_count += 1
                return frame
            except Empty:
                if errors:
                    raise errors[0]

    def emit(frame):
        nonlocal output_count, last_log
        binary_stdout.write(np.ascontiguousarray(frame).tobytes())
        output_count += 1
        now = time.monotonic()
        if now - last_log >= 0.5:
            last_log = now
            print(f"[cia render] RIFE frame={output_count} total_frames={args.expected_frames} input_frames={input_count} elapsed={now-started:.3f}", file=sys.stderr, flush=True)

    padding = (0, ((width + 127) // 128) * 128 - width,
               0, ((height + 127) // 128) * 128 - height)

    def tensor(frame):
        value = torch.from_numpy(frame.transpose(2, 0, 1).copy()).to(device).unsqueeze(0).float() / 255.0
        value = F.pad(value, padding)
        return value.half() if args.precision == "fp16" else value

    def image(value):
        return (value[0] * 255.0).byte().cpu().numpy().transpose(1, 2, 0)[:height, :width]

    def similarity(left, right):
        return float(ssim_matlab(
            F.interpolate(left, (32, 32), mode="bilinear", align_corners=False)[:, :3],
            F.interpolate(right, (32, 32), mode="bilinear", align_corners=False)[:, :3],
        ).item())

    try:
        with torch.inference_mode():
            lastframe = get()
            if lastframe is None:
                raise RuntimeError("The video decoder returned no frames")
            right = tensor(lastframe)
            pending = None
            while True:
                frame = pending if pending is not None else get()
                pending = None
                if frame is None:
                    break
                left, right = right, tensor(frame)
                score = similarity(left, right)
                at_end = False
                if score > 0.996:
                    following = get()
                    if following is None:
                        at_end = True
                        following = lastframe
                    else:
                        pending = following
                    right = model.inference(left, tensor(following), scale=1.0)
                    score = similarity(left, right)
                    frame = image(right)
                emit(lastframe)
                for i in range(1, args.factor):
                    middle = left if score < 0.2 else model.inference(left, right, i / args.factor, 1.0)
                    emit(image(middle))
                lastframe = frame
                if at_end:
                    break
            # Complete the final interval so boost/slowmo preserve expected duration.
            while output_count < input_count * args.factor:
                emit(lastframe)
            binary_stdout.flush()
        read_thread.join(timeout=10)
        if read_thread.is_alive():
            raise RuntimeError("Video decoder did not finish")
        if errors:
            raise errors[0]
        print(f"CIA_WORKER_DONE input_frames={input_count} output_frames={output_count}", file=sys.stderr, flush=True)
    finally:
        stop.set()
        if decoder.poll() is None:
            decoder.kill()
        decoder.wait()
        read_thread.join(timeout=5)
        log_thread.join(timeout=5)
        decoder.stdout.close()
        decoder.stderr.close()


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("--video", required=True)
    parser.add_argument("--video-index", type=int, default=0)
    parser.add_argument("--ffmpeg", required=True)
    parser.add_argument("--rife-dir", required=True)
    parser.add_argument("--model-dir", required=True)
    parser.add_argument("--precision", choices=("fp32", "fp16"), default="fp32")
    parser.add_argument("--width", type=int, required=True)
    parser.add_argument("--height", type=int, required=True)
    parser.add_argument("--factor", type=int, required=True)
    parser.add_argument("--fps", required=True)
    parser.add_argument("--expected-frames", type=int, default=0)
    options = parser.parse_args()
    try:
        main(options)
    except Exception as error:
        print(f"[RIFE ERROR] {error}", file=sys.stderr, flush=True)
        sys.exit(1)
