"""Verify an extracted Windows installer against its release kit and source tree.

Python 3.11 or later is required. This checks payload integrity and the Tauri
NSIS executable marker; the updater signature is verified separately.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path, PurePosixPath
import re

REPO = Path(__file__).resolve().parents[1]
SOURCE_RESOURCES = (
    'time_remap.py', 'rife_worker.py', 'runtime_health.py',
    'bootstrap/bootstrap-rife.ps1', 'bootstrap/bootstrap-python.ps1',
    'bootstrap/rife-profile.json', 'bootstrap/rife-requirements.lock',
)
FORBIDDEN_SUFFIXES = {
    '.mp4', '.mkv', '.mov', '.avi', '.webm', '.wav', '.flac', '.mp3',
    '.cube', '.pkl', '.pt', '.onnx', '.safetensors', '.ckpt', '.pyc', '.pyo', '.log', '.key', '.pfx', '.p12', '.ppk',
}
FORBIDDEN_NAMES = {'last_args.txt', 'config.json', 'runtime-config.json', '.env'}
TEXT_SUFFIXES = {'.py', '.ps1', '.json', '.ini', '.md', '.txt', '.cfg', '.pth', '.lock', '.toml', '.pem'}
UNKNOWN_MARKER = b'__TAURI_BUNDLE_TYPE_VAR_UNK'
NSIS_MARKER = b'__TAURI_BUNDLE_TYPE_VAR_NSS'
ALLOWED_PTH = 'smoothie/bin/Lib/site-packages/distutils-precedence.pth'
ALLOWED_CA_BUNDLE = 'smoothie/bin/Lib/site-packages/pip/_vendor/certifi/cacert.pem'


def require(condition, message):
    if not condition:
        raise ValueError(message)


def sha(path):
    with path.open('rb') as stream:
        return hashlib.file_digest(stream, 'sha256').hexdigest()


def read_json(path):
    return json.loads(path.read_text(encoding='utf-8-sig'))


def safe_relative(name):
    require(isinstance(name, str) and '\\' not in name, 'Manifest paths must use forward slashes.')
    relative = PurePosixPath(name)
    require(not relative.is_absolute() and '..' not in relative.parts and ':' not in name
            and name == relative.as_posix() and name not in {'', '.'},
            f'Unsafe manifest path: {name}')
    return relative


def actual_files(directory):
    files = {}
    casefolded = set()
    for path in directory.rglob('*'):
        require(not path.is_symlink(), f'Symlink in payload: {path.name}')
        if path.is_file():
            name = path.relative_to(directory).as_posix()
            require(name.casefold() not in casefolded,
                    f'Case-colliding payload path: {name}')
            casefolded.add(name.casefold())
            files[name] = path
    return files


def verify_file(path, expected, label):
    require(path.is_file(), f'Missing payload file: {label}')
    require(path.stat().st_size == expected['bytes'], f'Payload size mismatch: {label}')
    require(sha(path) == expected['sha256'], f'Payload digest mismatch: {label}')


def verify_clean_payload(resources, runtime_files):
    # Upstream libraries contain generic example paths. Reject the actual build
    # user's profile and repository paths, rather than those upstream examples.
    private_paths = {str(REPO.resolve()), os.environ.get('USERPROFILE', '')}
    private_paths = {value.replace('\\', '/').casefold() for value in private_paths if value}
    for path in resources.rglob('*'):
        if not path.is_file():
            continue
        name = path.relative_to(resources).as_posix()
        require(path.suffix.casefold() not in FORBIDDEN_SUFFIXES, f'Unwanted media/model/private file: {name}')
        require(path.name.casefold() not in FORBIDDEN_NAMES, f'Personal settings file: {name}')
        require('__pycache__' not in path.parts, f'Python cache in payload: {name}')
        if path.suffix.casefold() == '.pth':
            require(name == 'runtime/' + ALLOWED_PTH, f'Unexpected .pth file: {name}')
            require(ALLOWED_PTH in runtime_files, 'The allowed .pth file is absent from the pinned manifest.')
        if path.suffix.casefold() == '.pem':
            require(name == 'runtime/' + ALLOWED_CA_BUNDLE, f'Unexpected certificate/key file: {name}')
            require(ALLOWED_CA_BUNDLE in runtime_files, 'The public CA bundle is absent from the pinned manifest.')
        if path.suffix.casefold() in TEXT_SUFFIXES:
            content = path.read_text(encoding='utf-8-sig', errors='replace').replace('\\', '/').casefold()
            require(not any(value in content for value in private_paths), f'Private build path in payload: {name}')
            require(re.search(r'gh[pousr]_[a-z0-9]{20,}|github_pat_[a-z0-9_]{20,}', content) is None,
                    f'Credential-like text in payload: {name}')
            require(re.search(r'-----begin (?:[a-z0-9 ]* )?private key-----', content) is None,
                    f'Private key in payload: {name}')


def verify_release(kit, unpacked, compiled, repo=REPO):
    kit, unpacked, compiled, repo = (path.resolve() for path in (kit, unpacked, compiled, repo))
    require(kit.is_dir() and unpacked.is_dir() and compiled.is_file(), 'Kit, unpacked directory and compiled executable must exist.')
    manifests = list(unpacked.rglob('RELEASE-PAYLOAD.json'))
    require(len(manifests) == 1, 'The extracted installer must contain exactly one RELEASE-PAYLOAD.json.')
    resources = manifests[0].parent
    manifest = read_json(manifests[0])
    require(manifest == read_json(kit/'RELEASE-PAYLOAD.json'), 'Installer manifest differs from the release kit.')
    version = read_json(repo/'src-tauri/tauri.conf.json')['version']
    require(manifest['version'] == version, 'Installer manifest version differs from the source version.')

    runtime = resources/'runtime'
    runtime_files = actual_files(runtime)
    expected_files = manifest['files']
    for name in expected_files:
        safe_relative(name)
    require(set(runtime_files) == set(expected_files),
            f'Runtime inventory mismatch: extra={sorted(set(runtime_files)-set(expected_files))}, '
            f'missing={sorted(set(expected_files)-set(runtime_files))}')
    require(set(actual_files(kit/'runtime')) == set(expected_files), 'Release kit runtime inventory differs from its manifest.')
    for name, expected in expected_files.items():
        verify_file(runtime_files[name], expected, name)
        verify_file(kit/'runtime'/name, expected, 'kit/' + name)

    source_files = manifest['source_files']
    require(set(source_files) == set(SOURCE_RESOURCES) | {'THIRD_PARTY_NOTICES.md'},
            'The source resource inventory must contain all seven scripts/profile/lock and notices.')
    for name, expected in source_files.items():
        safe_relative(name)
        source = repo/name if name == 'THIRD_PARTY_NOTICES.md' else repo/'src-tauri/resources'/name
        verify_file(source, expected, 'source/' + name)
        verify_file(resources/name, expected, name)

    resource_files = actual_files(resources)
    expected_resources = {'runtime/' + name for name in expected_files} | set(source_files) | {'RELEASE-PAYLOAD.json'}
    require(set(resource_files) == expected_resources,
            f'Unexpected resource inventory: extra={sorted(set(resource_files)-expected_resources)}, '
            f'missing={sorted(expected_resources-set(resource_files))}')

    profile_path = resources/'bootstrap/rife-profile.json'
    profile = read_json(profile_path)
    optional = manifest['rife_optional_environment']
    require(optional['bundled'] is False, 'Downloaded RIFE environment must not be bundled.')
    require(optional['profile'] == profile and optional['profile_sha256'] == sha(profile_path),
            'Recorded RIFE profile provenance differs from the bundled profile.')
    require(optional['requirements_lock_sha256'] == sha(resources/'bootstrap/rife-requirements.lock'),
            'Recorded RIFE requirements provenance differs from the bundled lock.')
    require(optional['python_package']['url'] == manifest['python_package_url']
            and optional['python_package']['sha256'] == manifest['python_package_sha256'],
            'Python package provenance is inconsistent.')

    for name in ('recipe.ini', 'defaults.ini'):
        content = (runtime/'smoothie'/name).read_text(encoding='utf-8-sig')
        require('#{type:' in content and 'enabled: no' in content, f'Missing Smoothie defaults metadata: {name}')
        require(re.search(r'[a-z]:[/\\]Users[/\\]', content, re.IGNORECASE) is None,
                f'Personal path in Smoothie settings: {name}')
    verify_clean_payload(resources, runtime_files)

    executables = [path for path in unpacked.rglob(compiled.name) if not path.is_relative_to(resources)]
    require(len(executables) == 1, 'The extracted installer must contain exactly one application executable.')
    binary = compiled.read_bytes()
    require(binary.count(UNKNOWN_MARKER) + binary.count(NSIS_MARKER) == 1, 'Compiled executable must contain exactly one supported Tauri bundle marker.')
    # Recent Tauri CLIs patch the compiled file itself. Older CLIs patch a copy.
    # Both must match exactly, with this sole well-defined marker conversion.
    expected = binary.replace(UNKNOWN_MARKER, NSIS_MARKER, 1)
    require(executables[0].read_bytes() == expected,
            'Packaged executable differs beyond the expected Tauri NSIS marker patch.')
    return {'version': version, 'runtimeFiles': len(runtime_files), 'sourceResources': len(source_files),
            'rifeProfile': profile['profileId'], 'executableSha256': sha(executables[0])}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--kit', required=True, type=Path, help='Prepared release kit directory.')
    parser.add_argument('--unpacked', required=True, type=Path, help='Directory containing the extracted NSIS installer.')
    parser.add_argument('--compiled', required=True, type=Path, help='Original Cargo release executable, before bundling.')
    args = parser.parse_args()
    result = verify_release(args.kit, args.unpacked, args.compiled)
    print(json.dumps(result, indent=2))
    print('Exact installer payload and expected Tauri NSIS executable patch verified.')


if __name__ == '__main__':
    main()
