"""Release verifier regressions using small synthetic installer fixtures."""
import importlib.util
import json
from pathlib import Path
import shutil
import tempfile
import unittest


REPO = Path(__file__).resolve().parents[1]
SPEC = importlib.util.spec_from_file_location('release_verifier', REPO/'scripts/verify-release.py')
VERIFIER = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(VERIFIER)


class ReleasePayloadTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)
        self.kit = self.root/'kit'
        self.unpacked = self.root/'unpacked'
        self.resources = self.unpacked/'resources'
        self.repo = self.root/'source'
        self.compiled = self.root/'compiled/cia-render.exe'
        self.compiled.parent.mkdir()
        self.compiled.write_bytes(b'fixture-executable\0' + VERIFIER.UNKNOWN_MARKER + b'\0fixture-end')
        self.resources.mkdir(parents=True)
        (self.unpacked/self.compiled.name).write_bytes(
            self.compiled.read_bytes().replace(VERIFIER.UNKNOWN_MARKER, VERIFIER.NSIS_MARKER))
        (self.repo/'src-tauri').mkdir(parents=True)
        shutil.copyfile(REPO/'src-tauri/tauri.conf.json', self.repo/'src-tauri/tauri.conf.json')
        for name in VERIFIER.SOURCE_RESOURCES:
            source = REPO/'src-tauri/resources'/name
            destination = self.repo/'src-tauri/resources'/name
            destination.parent.mkdir(parents=True, exist_ok=True)
            shutil.copyfile(source, destination)
            (self.resources/name).parent.mkdir(parents=True, exist_ok=True)
            shutil.copyfile(source, self.resources/name)
        for directory in (self.repo, self.resources):
            shutil.copyfile(REPO/'THIRD_PARTY_NOTICES.md', directory/'THIRD_PARTY_NOTICES.md')
        for name in ('smoothie/recipe.ini', 'smoothie/defaults.ini'):
            path = self.resources/'runtime'/name
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_text('[interpolation]\nenabled: no #{type: bool}\n', encoding='utf-8')
        self.refresh_manifest()

    def refresh_manifest(self):
        runtime = self.resources/'runtime'
        if (self.kit/'runtime').exists():
            shutil.rmtree(self.kit/'runtime')
        shutil.copytree(runtime, self.kit/'runtime')
        profile_path = self.resources/'bootstrap/rife-profile.json'
        profile = VERIFIER.read_json(profile_path)
        files = {path.relative_to(runtime).as_posix(): {
            'bytes': path.stat().st_size, 'sha256': VERIFIER.sha(path)}
            for path in runtime.rglob('*') if path.is_file()}
        source_files = {name: {'bytes': (self.resources/name).stat().st_size,
                               'sha256': VERIFIER.sha(self.resources/name)}
                        for name in (*VERIFIER.SOURCE_RESOURCES, 'THIRD_PARTY_NOTICES.md')}
        self.manifest = {
            'version': VERIFIER.read_json(self.repo/'src-tauri/tauri.conf.json')['version'],
            'python_package_url': 'https://example.invalid/python.nupkg',
            'python_package_sha256': '0'*64,
            'rife_optional_environment': {
                'bundled': False, 'profile': profile,
                'profile_sha256': VERIFIER.sha(profile_path),
                'requirements_lock_sha256': VERIFIER.sha(self.resources/'bootstrap/rife-requirements.lock'),
                'python_package': {'url': 'https://example.invalid/python.nupkg', 'sha256': '0'*64},
            },
            'source_files': source_files, 'files': files,
        }
        self.write_manifest()

    def write_manifest(self):
        for directory in (self.kit, self.resources):
            (directory/'RELEASE-PAYLOAD.json').write_text(json.dumps(self.manifest), encoding='utf-8')

    def verify(self):
        return VERIFIER.verify_release(self.kit, self.unpacked, self.compiled, self.repo)

    def test_exact_payload_and_only_expected_executable_patch_pass(self):
        result = self.verify()
        self.assertEqual(result['runtimeFiles'], 2)
        self.assertEqual(result['sourceResources'], 8)

    def test_extra_or_missing_runtime_file_is_rejected(self):
        extra = self.resources/'runtime/extra.txt'
        extra.write_text('extra', encoding='utf-8')
        with self.assertRaisesRegex(ValueError, 'Runtime inventory mismatch'):
            self.verify()
        extra.unlink()
        (self.resources/'runtime/smoothie/recipe.ini').unlink()
        with self.assertRaisesRegex(ValueError, 'Runtime inventory mismatch'):
            self.verify()

    def test_same_size_runtime_corruption_is_rejected(self):
        path = self.resources/'runtime/smoothie/recipe.ini'
        path.write_bytes(path.read_bytes().replace(b'no', b'on', 1))
        with self.assertRaisesRegex(ValueError, 'Payload digest mismatch'):
            self.verify()

    def test_changed_source_after_preparation_is_rejected(self):
        with (self.repo/'src-tauri/resources/runtime_health.py').open('ab') as stream:
            stream.write(b'\n# changed after preparation\n')
        with self.assertRaisesRegex(ValueError, 'Payload size mismatch: source/runtime_health.py'):
            self.verify()

    def test_extra_installer_resource_is_rejected(self):
        (self.resources/'unexpected.txt').write_text('extra', encoding='utf-8')
        with self.assertRaisesRegex(ValueError, 'Unexpected resource inventory'):
            self.verify()

    def test_executable_change_beyond_bundle_marker_is_rejected(self):
        with (self.unpacked/self.compiled.name).open('ab') as stream:
            stream.write(b'tampered')
        with self.assertRaisesRegex(ValueError, 'differs beyond the expected Tauri NSIS marker'):
            self.verify()

    def test_matched_manifest_cannot_allow_models_or_arbitrary_pth(self):
        for filename in ('model.pth', 'model.pkl'):
            path = self.resources/'runtime'/filename
            path.write_bytes(b'not a permitted runtime artifact')
            self.refresh_manifest()
            with self.assertRaisesRegex(ValueError, 'Unexpected .pth|Unwanted media/model/private file'):
                self.verify()
            path.unlink()
        self.refresh_manifest()
        allowed = self.resources/'runtime'/VERIFIER.ALLOWED_PTH
        allowed.parent.mkdir(parents=True, exist_ok=True)
        allowed.write_text("import _distutils_hack\n", encoding='utf-8')
        self.refresh_manifest()
        self.assertEqual(self.verify()['runtimeFiles'], 3)

    def test_inconsistent_rife_provenance_is_rejected(self):
        self.manifest['rife_optional_environment']['profile']['profileId'] = 'unexpected-profile'
        self.write_manifest()
        with self.assertRaisesRegex(ValueError, 'Recorded RIFE profile provenance'):
            self.verify()


if __name__ == '__main__':
    unittest.main()
