import importlib.util
import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch


SCRIPT = Path(__file__).resolve().parents[1] / "src-tauri/resources/runtime_health.py"
SPEC = importlib.util.spec_from_file_location("runtime_health", SCRIPT)
health = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(health)


class RuntimeHealthTests(unittest.TestCase):
    def test_rejects_corrupt_present_component(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            component = root / "flownet.pkl"
            component.write_bytes(b"original weights")
            expected = health.sha256(component)
            component.write_bytes(b"corrupted weight")
            with self.assertRaisesRegex(health.HealthError, "integrity check failed"):
                health.verify_files(root, {"flownet.pkl": expected})

    def test_manifest_paths_cannot_escape_generation(self):
        with tempfile.TemporaryDirectory() as directory:
            for relative in ("../outside", "nested/../../outside"):
                with self.assertRaises(health.HealthError):
                    health.child_path(Path(directory), relative)

    def test_outdated_torch_fails_before_model_execution(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            project = root / "Practical-RIFE"
            model = project / "train_log"
            for relative in ("inference_video.py", "model/pytorch_msssim/__init__.py", "model/warplayer.py",
                             "train_log/flownet.pkl", "train_log/RIFE_HDv3.py", "train_log/IFNet_HDv3.py"):
                target = project / relative
                target.parent.mkdir(parents=True, exist_ok=True)
                target.write_bytes(b"fixture")
            profile = {"modelFiles": {"flownet.pkl": "0" * 64}}
            with patch.object(health.importlib.metadata, "version", return_value="2.5.1+cu121"):
                with self.assertRaisesRegex(health.HealthError, "obsolete"):
                    health.verify_runtime(project.resolve(), model.resolve(), None, profile)

    def test_writable_manifest_cannot_redefine_trusted_model_hash(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory).resolve()
            project = root / "Practical-RIFE"
            model = project / "train_log"
            for relative in ("inference_video.py", "model/pytorch_msssim/__init__.py", "model/warplayer.py",
                             "train_log/flownet.pkl", "train_log/RIFE_HDv3.py", "train_log/IFNet_HDv3.py"):
                target = project / relative
                target.parent.mkdir(parents=True, exist_ok=True)
                target.write_bytes(b"fixture")
            trusted = health.sha256(model / "flownet.pkl")
            (model / "flownet.pkl").write_bytes(b"changed model")
            manifest = root / "runtime-manifest.json"
            manifest.write_text(json.dumps({"schemaVersion": 1, "profileId": "test-profile",
                                           "files": {"Practical-RIFE/train_log/flownet.pkl": health.sha256(model / "flownet.pkl")}}))
            profile = {"profileId": "test-profile", "projectFiles": {"inference_video.py": health.sha256(project / "inference_video.py")},
                       "modelFiles": {"flownet.pkl": trusted}, "packages": {}}
            with self.assertRaisesRegex(health.HealthError, "flownet.pkl"):
                health.verify_runtime(project, model, manifest, profile)

    def test_profile_has_complete_pinned_wheel_hashes(self):
        profile = health.read_profile()
        lock = (SCRIPT.parent / "bootstrap/rife-requirements.lock").read_text()
        self.assertGreaterEqual(health.torch_version_tuple(profile["packages"]["torch"]), (2, 10, 0))
        self.assertEqual(len(profile["packages"]), len(profile["wheels"]))
        for artifact in profile["wheels"]:
            self.assertEqual(len(artifact["sha256"]), 64)
            self.assertIn(artifact["sha256"], lock)
            self.assertTrue(artifact["url"].startswith(("https://files.pythonhosted.org/", "https://download-r2.pytorch.org/")))

    def test_prerelease_torch_is_not_mistaken_for_patched_stable(self):
        for version in ("2.10.0rc1", "2.10.0.dev20250101", "not-a-version"):
            with self.assertRaises(health.HealthError):
                health.torch_version_tuple(version)

    def test_new_unrecorded_project_code_is_rejected(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / "model").mkdir()
            (root / "model/__init__.py").write_text("# unexpected import hook")
            with self.assertRaisesRegex(health.HealthError, "Unexpected Python code"):
                health.verify_project_code_set(root, {"projectFiles": {"model/loss.py": "trusted"}, "modelFiles": {}})


if __name__ == "__main__":
    unittest.main()
