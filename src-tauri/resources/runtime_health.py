"""Isolated, structured health probe for the optional interpolation runtime."""
import argparse
from contextlib import redirect_stdout
import hashlib
import importlib
import importlib.metadata
import json
from pathlib import Path
import re
import struct
import sys
import types


class HealthError(RuntimeError):
    def __init__(self, message, status="broken"):
        super().__init__(message)
        self.status = status


def sha256(path):
    with Path(path).open("rb") as stream:
        digest = hashlib.sha256()
        for chunk in iter(lambda: stream.read(1024 * 1024), b""):
            digest.update(chunk)
        return digest.hexdigest()


def child_path(root, relative):
    relative_path = Path(relative)
    if relative_path.is_absolute() or ".." in relative_path.parts:
        raise HealthError("The runtime manifest contains an invalid relative path.")
    path = (root / relative_path).resolve()
    if not path.is_relative_to(root.resolve()):
        raise HealthError("A runtime component points outside its managed generation.")
    return path


def torch_version_tuple(version):
    match = re.fullmatch(r"(\d+)\.(\d+)\.(\d+)(?:\+[^\s]+)?", version)
    if not match:
        raise HealthError("The installed PyTorch version could not be verified.")
    return tuple(map(int, match.groups()))


def verify_files(root, files):
    if not isinstance(files, dict) or not files:
        raise HealthError("The runtime integrity manifest has no verified files.")
    for relative, expected in files.items():
        path = child_path(root, relative)
        if not path.is_file() or sha256(path).lower() != expected.lower():
            raise HealthError(f"Runtime integrity check failed: {relative}. Use Repair environment.")


def verify_project_code_set(rife_dir, profile):
    expected = {name for name in profile["projectFiles"] if name.endswith(".py")}
    expected.update("train_log/" + name for name in profile["modelFiles"] if name.endswith(".py"))
    actual = {path.relative_to(rife_dir).as_posix() for path in rife_dir.rglob("*.py")}
    unexpected = actual - expected
    if unexpected:
        raise HealthError(f"Unexpected Python code was found in the managed project: {sorted(unexpected)[0]}. Use Repair environment.")


def read_profile():
    path = Path(__file__).resolve().parent / "bootstrap" / "rife-profile.json"
    try:
        return json.loads(path.read_text(encoding="utf-8-sig"))
    except (OSError, ValueError) as error:
        raise HealthError("The bundled interpolation profile is missing or invalid. Reinstall cia render.") from error


def verify_runtime(rife_dir, model_dir, manifest_path, profile, full_integrity=True):
    if not rife_dir.is_dir() or not model_dir.is_dir():
        raise HealthError("The interpolation project or model directory is missing. Use Repair environment.")
    for relative in ("inference_video.py", "model/pytorch_msssim/__init__.py", "model/warplayer.py"):
        if not (rife_dir / relative).is_file():
            raise HealthError(f"The interpolation project is incomplete: {relative}. Use Repair environment.")
    for filename in ("flownet.pkl", "RIFE_HDv3.py", "IFNet_HDv3.py"):
        if not (model_dir / filename).is_file():
            raise HealthError(f"The interpolation model is incomplete: {filename}. Use Repair environment.")
    if manifest_path:
        try:
            manifest = json.loads(manifest_path.read_text(encoding="utf-8-sig"))
        except (OSError, ValueError) as error:
            raise HealthError("The managed runtime manifest is missing or invalid. Use Repair environment.") from error
        if manifest.get("schemaVersion") != 1 or manifest.get("profileId") != profile["profileId"]:
            raise HealthError("This managed runtime uses an obsolete profile. Use Repair environment.")
        root = manifest_path.parent.resolve()
        if rife_dir != child_path(root, "Practical-RIFE") or model_dir != child_path(root, "Practical-RIFE/train_log"):
            raise HealthError("The managed runtime paths do not match their generation.")
        if full_integrity:
            verify_files(root, manifest.get("files"))
        # The trusted bundled profile, rather than a writable per-user manifest,
        # determines the expected upstream project and model content.
        verify_files(rife_dir, profile["projectFiles"])
        verify_files(model_dir, profile["modelFiles"])
        verify_project_code_set(rife_dir, profile)
        expected_packages = profile["packages"]
        if Path(sys.base_prefix).resolve() != child_path(root, "python"):
            raise HealthError("The managed virtual environment points to another Python installation. Use Repair environment.")
    else:
        # Existing externally configured projects are never modified. The known
        # official model is integrity-checked where applicable; custom code is
        # explicitly user-selected executable code, not a sandboxed data format.
        model_hash = sha256(model_dir / "flownet.pkl")
        if model_hash.lower() == profile["modelFiles"]["flownet.pkl"].lower():
            verify_files(model_dir, profile["modelFiles"])
        expected_packages = None
    if struct.calcsize("P") != 8 or sys.version_info < (3, 10):
        raise HealthError("Interpolation requires a 64-bit Python 3.10 or newer runtime.", "unsupported")
    try:
        installed_torch = importlib.metadata.version("torch")
    except importlib.metadata.PackageNotFoundError as error:
        raise HealthError("PyTorch is missing. Use Repair environment.") from error
    if torch_version_tuple(installed_torch) < (2, 10, 0):
        raise HealthError("This PyTorch runtime is obsolete (2.10.0 or newer required). Use Repair environment.")
    if expected_packages:
        for name, expected in expected_packages.items():
            try:
                installed = importlib.metadata.version(name)
            except importlib.metadata.PackageNotFoundError as error:
                raise HealthError(f"Runtime dependency {name} is missing. Use Repair environment.") from error
            if installed != expected:
                raise HealthError(f"Runtime dependency {name} has changed ({installed}; expected {expected}). Use Repair environment.")
    return installed_torch


def forward_probe(rife_dir, model_dir, profile, device_index=0):
    # Dependencies must resolve from the isolated interpreter before the selected
    # model directory is added; an unrelated project torch.py must not shadow them.
    import numpy  # noqa: F401
    import torch
    import torchvision  # noqa: F401
    if not torch.cuda.is_available():
        raise HealthError("CUDA is unavailable. Install a supported NVIDIA driver and retry.", "unsupported")
    if device_index < 0 or device_index >= torch.cuda.device_count():
        raise HealthError("The selected NVIDIA GPU is no longer available. Use Repair environment.", "unsupported")
    torch.cuda.set_device(device_index)
    capability = torch.cuda.get_device_capability(device_index)
    if list(capability) not in profile["supportedComputeCapabilities"]:
        raise HealthError(f"This GPU architecture ({capability[0]}.{capability[1]}) is not supported by this interpolation profile.", "unsupported")
    device = torch.cuda.get_device_properties(device_index)
    if device.total_memory < 2 * 1024**3:
        raise HealthError("Interpolation requires at least 2 GiB of GPU memory.", "unsupported")
    # A device name or is_available() alone cannot verify that the wheel contains
    # working kernels for this GPU. Execute a kernel and synchronize before RIFE.
    with torch.inference_mode():
        tensor = torch.ones((16, 16), device="cuda")
        if (tensor @ tensor).sum().item() != 4096:
            raise HealthError("The CUDA kernel self-test returned an invalid result.")
    sys.path.insert(0, str(rife_dir))
    package = types.ModuleType("train_log")
    package.__path__ = [str(model_dir)]
    sys.modules["train_log"] = package
    torch.set_grad_enabled(False)
    with redirect_stdout(sys.stderr):
        model = importlib.import_module("train_log.RIFE_HDv3").Model()
        if getattr(model, "version", 0) < 3.9:
            raise HealthError("The selected model is not a supported Practical-RIFE 4.x model.")
        weights = torch.load(model_dir / "flownet.pkl", map_location="cpu", weights_only=True)
        if not isinstance(weights, dict) or not weights:
            raise HealthError("The model checkpoint does not contain a valid state dictionary.")
        normalized = {}
        network_keys = set(model.flownet.state_dict())
        for key, value in weights.items():
            if not isinstance(key, str):
                raise HealthError("The model checkpoint contains an invalid parameter name.")
            key = key.removeprefix("module.")
            if key in normalized:
                raise HealthError("The model checkpoint contains duplicate parameters.")
            if key in network_keys or not key.startswith(("teacher.", "caltime.")):
                normalized[key] = value
        model.flownet.load_state_dict(normalized, strict=True)
        model.eval()
        model.device()
        with torch.inference_mode():
            left = torch.zeros((1, 3, 128, 128), device="cuda")
            right = torch.ones_like(left) * 0.5
            result = model.inference(left, right, timestep=0.5, scale=1.0)
            torch.cuda.synchronize()
            if tuple(result.shape) != (1, 3, 128, 128) or not torch.isfinite(result).all().item():
                raise HealthError("The RIFE model forward self-test returned an invalid frame.")
    return {"name": device.name, "index": device_index, "computeCapability": f"{capability[0]}.{capability[1]}",
            "memoryBytes": device.total_memory, "cudaVersion": torch.version.cuda,
            "compiledArchitectures": torch.cuda.get_arch_list()}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--rife-dir", required=True, type=Path)
    parser.add_argument("--model-dir", required=True, type=Path)
    parser.add_argument("--manifest", type=Path)
    parser.add_argument("--device-index", type=int, default=0)
    parser.add_argument("--json", action="store_true")
    args = parser.parse_args()
    result = {"status": "broken", "ready": False, "detail": "", "profileId": None}
    try:
        profile = read_profile()
        result["profileId"] = profile["profileId"] if args.manifest else "external"
        result["torchVersion"] = verify_runtime(args.rife_dir.resolve(), args.model_dir.resolve(), args.manifest, profile)
        result["device"] = forward_probe(args.rife_dir.resolve(), args.model_dir.resolve(), profile, args.device_index)
        result.update(status="ready", ready=True, detail="Python, dependency integrity, CUDA kernels and RIFE model inference verified.")
    except Exception as error:
        result.update(status=getattr(error, "status", "broken"), detail=str(error).replace("\n", " ")[:2000])
    print(json.dumps(result, ensure_ascii=True, separators=(",", ":")), flush=True)
    return 0 if result["ready"] else 1


if __name__ == "__main__":
    raise SystemExit(main())
