"""Prepare the pinned runtime payload and provenance used by a Windows release.

Python 3.11 or later is required. Downloads are reused only after hash validation;
the generated runtime directory is rebuilt from the pinned archives each time.
"""
import argparse
import hashlib
import json
from pathlib import Path
import shutil
import subprocess
import textwrap
import urllib.request
import zipfile

UI = Path(__file__).resolve().parents[1]
VERSION = json.loads((UI/'src-tauri/tauri.conf.json').read_text(encoding='utf-8'))['version']
TAG = 'Nightly_2025.11.30_14-36'
COMMIT = '3da73de9df0c991801c4f8474b320445fb25a208'
SOURCE_RESOURCES = (
    'time_remap.py', 'rife_worker.py', 'runtime_health.py',
    'bootstrap/bootstrap-rife.ps1', 'bootstrap/bootstrap-python.ps1',
    'bootstrap/rife-profile.json', 'bootstrap/rife-requirements.lock',
)
PYTHON_PACKAGE = {
    'url': 'https://api.nuget.org/v3-flatcontainer/python/3.11.9/python.3.11.9.nupkg',
    'sha256': '9283876d58c017e0e846f95b490da3bca0fc0a6ee1134b2870677cfb7eec3c67',
}
FFMPEG_URL = 'https://github.com/GyanD/codexffmpeg/releases/download/8.1.1/ffmpeg-8.1.1-essentials_build.zip'
FFMPEG_SHA256 = '6f58ce889f59c311410f7d2b18895b33c03456463486f3b1ebc93d97a0f54541'

def sha(path):
    with path.open('rb') as stream:
        return hashlib.file_digest(stream, 'sha256').hexdigest()

def acquire(url, destination, expected):
    if destination.is_file() and sha(destination) == expected:
        return destination
    destination.parent.mkdir(parents=True, exist_ok=True)
    partial = destination.with_suffix(destination.suffix + '.part')
    request = urllib.request.Request(url, headers={'User-Agent': 'cia-render-release/1.3'})
    with urllib.request.urlopen(request, timeout=60) as response, partial.open('wb') as output:
        shutil.copyfileobj(response, output, 1024 * 1024)
    if sha(partial) != expected:
        partial.unlink()
        raise RuntimeError(f'Release artifact hash mismatch: {destination.name}')
    partial.replace(destination)
    return destination

def prepare_release(kit):
    target_root = (UI/'src-tauri/target').resolve()
    KIT = kit.resolve()
    if not KIT.is_relative_to(target_root) or KIT == target_root:
        raise ValueError('The release kit must be a child directory of src-tauri/target.')
    RUNTIME = KIT/'runtime'
    ARCHIVE = KIT/'smoothie-rs-nightly.zip'
    acquire(f'https://github.com/couleur-tweak-tips/smoothie-rs/releases/download/{TAG}/smoothie-rs-nightly.zip',
            ARCHIVE, 'a9e49e8638c0c1db90a86d4ddb4e5eae0681e773c5bd9177caab8eb5586c4a07')
    assert sha(ARCHIVE) == 'a9e49e8638c0c1db90a86d4ddb4e5eae0681e773c5bd9177caab8eb5586c4a07'
    assert RUNTIME.resolve().is_relative_to((UI/'src-tauri/target').resolve()) and RUNTIME.name == 'runtime'
    if RUNTIME.exists(): shutil.rmtree(RUNTIME)
    RUNTIME.mkdir(parents=True, exist_ok=True)
    with zipfile.ZipFile(ARCHIVE) as archive:
        for item in archive.infolist():
            if item.is_dir(): continue
            relative = Path(item.filename).relative_to('smoothie-rs')
            if '__pycache__' in relative.parts or relative.suffix.lower() in {'.pyc','.log','.cube','.mp4','.mkv','.pkl','.pt','.onnx'} or relative.name == 'last_args.txt': continue
            target = (RUNTIME/'smoothie'/relative).resolve()
            assert target.is_relative_to((RUNTIME/'smoothie').resolve())
            target.parent.mkdir(parents=True, exist_ok=True)
            target.write_bytes(archive.read(item))
            tested = UI/'src-tauri/resources/runtime/smoothie'/relative
            if relative.suffix.lower() in {'.exe','.dll','.pyd','.vpy'} and tested.exists():
                assert sha(target) == sha(tested), relative

    # Keep Smoothie's #{type: ...} metadata: defaults parsing requires it.
    original_defaults = (RUNTIME/'smoothie/defaults.ini').read_text(encoding='utf-8-sig')
    settings = {
     'interpolation': {'enabled':'no'}, 'frame blending': {'enabled':'yes','fps':'30','intensity':'1.0'},
     'flowblur': {'enabled':'no'}, 'pre-interp': {'enabled':'no'},
     'color grading': {'enabled':'no','brightness':'1.0','saturation':'1.0','contrast':'1.0','hue':'0'},
     'lut': {'enabled':'no','path':'','opacity':'1.0'}, 'timescale': {'in':'1.0','out':'1.0'},
     'miscellaneous': {'global output folder':'','source indexing':'no'},
     'output': {'enc args':'-c:v libx264 -preset medium -crf 18 -pix_fmt yuv420p -c:a aac -b:a 320k -ar 48000 -movflags +faststart'}
    }
    section = ''
    lines = []
    replaced = set()
    for line in original_defaults.splitlines():
     stripped = line.strip()
     if stripped.startswith('[') and stripped.endswith(']'):
      section = stripped[1:-1].strip().lower()
     elif stripped and not stripped.startswith(('#', ';', '/', ':')) and ':' in stripped:
      key = stripped.split(':', 1)[0].strip().lower()
      if key in settings.get(section, {}):
       line = f'{key}: {settings[section][key]}'
       replaced.add((section, key))
     lines.append(line)
    assert replaced == {(section, key) for section, values in settings.items() for key in values}, 'Missing default setting'
    text = '\n'.join(lines) + '\n'
    assert text.count('#{') == original_defaults.count('#{'), 'Smoothie metadata changed'
    for name in ('recipe.ini','defaults.ini'):
     (RUNTIME/'smoothie'/name).write_text(text,encoding='utf-8')
    ffmpeg_archive = acquire(FFMPEG_URL, KIT/'ffmpeg-8.1.1-essentials.zip', FFMPEG_SHA256)
    ffmpeg_root = RUNTIME/'ffmpeg'
    ffmpeg_root.mkdir()
    with zipfile.ZipFile(ffmpeg_archive) as archive:
        for name, expected in {'ffmpeg.exe':'228d7a8556258de907fdb55f36850078ebc7680b84ec30d84ea02e99bec1d1eb',
                               'ffprobe.exe':'0fde260f5abd35c9cafd96f594cc76365a780c1b73a90e35b6a3409ea1db1bf0'}.items():
            member = next(item for item in archive.namelist() if item.endswith('/bin/'+name))
            target = ffmpeg_root/name
            target.write_bytes(archive.read(member))
            assert sha(target) == expected, name
        notice = next(name for name in archive.namelist() if name.endswith('/LICENSE'))
        (ffmpeg_root/'COPYING.GPLv3.txt').write_bytes(archive.read(notice))

    licence_artifacts = {
        'smoothie/COPYING.GPLv3.txt': {
            'url': f'https://raw.githubusercontent.com/couleur-tweak-tips/smoothie-rs/{COMMIT}/LICENSE',
            'sha256': '605e9047a563c5c8396ffb18232aa4304ec56586aee537c45064c6fb425e44ad',
        },
        'smoothie/VAPOURSYNTH-COPYING.LESSER': {
            'url': 'https://raw.githubusercontent.com/vapoursynth/vapoursynth/R70/COPYING.LESSER',
            'sha256': 'b634ab5640e258563c536e658cad87080553df6f34f62269a21d554844e58bfe',
        },
    }
    for relative, artifact in licence_artifacts.items():
        cached = acquire(artifact['url'], KIT/'licences'/Path(relative).name, artifact['sha256'])
        shutil.copyfile(cached, RUNTIME/relative)
    profile_path = UI/'src-tauri/resources/bootstrap/rife-profile.json'
    profile = json.loads(profile_path.read_text(encoding='utf-8-sig'))
    lock_path = UI/'src-tauri/resources/bootstrap/rife-requirements.lock'
    source_notes = textwrap.dedent(f'''
    # Runtime source availability

    Smoothie portable {TAG}, source commit {COMMIT}:
    https://github.com/couleur-tweak-tips/smoothie-rs/tree/{COMMIT}
    https://github.com/couleur-tweak-tips/smoothie-rs/archive/{COMMIT}.tar.gz

    VapourSynth R70:
    https://github.com/vapoursynth/vapoursynth/tree/R70
    https://github.com/vapoursynth/vapoursynth/archive/refs/tags/R70.tar.gz
    The portable archive's scripts, DLLs and licence notices are retained.
    Recipe/default settings are replaced with neutral app settings; caches and user
    sidecars are excluded.

    FFmpeg / FFprobe 8.1.1 essentials, Gyan.dev GPLv3 build:
    https://www.gyan.dev/ffmpeg/builds/
    Pinned binary archive: {FFMPEG_URL}
    FFmpeg upstream source for version 8.1.1:
    https://ffmpeg.org/releases/ffmpeg-8.1.1.tar.xz
    The release also supplies this FFmpeg source archive. It is the upstream FFmpeg
    component source, not the source of every statically linked third-party library.
    The complete build configuration and exact executable hashes are recorded in
    RELEASE-PAYLOAD.json. Dependency sources and build information are linked from:
    https://www.gyan.dev/ffmpeg/builds/#libraries
    https://github.com/GyanD/codexffmpeg
    Preserved licence texts and these links do not replace applicable distribution
    obligations for linked libraries.

    Optional RIFE environment (downloaded after explicit installation or repair):
    Profile: {profile['profileId']}
    Practical-RIFE project archive: {profile['projectArchive']['url']}
    Model archive: {profile['modelArchive']['url']}
    Python package: {PYTHON_PACKAGE['url']}
    The installer contains the pinned profile, requirements lock and bootstrap; it
    does not contain RIFE model weights, Python, Torch or the downloaded environment.
    Their precise download URLs and SHA-256 hashes are recorded in RELEASE-PAYLOAD.json.
    The managed environment preserves upstream package licence files and Practical-RIFE's
    MIT licence. Model-weight redistribution is not implied by this installer.
    ''').lstrip()
    (RUNTIME/'SOURCE-AVAILABILITY.md').write_text(source_notes,encoding='utf-8')
    (ffmpeg_root/'SOURCE-AVAILABILITY.md').write_text(source_notes,encoding='utf-8')
    ffmpeg_build = subprocess.check_output([str(RUNTIME/'ffmpeg/ffmpeg.exe'), '-version'], text=True)
    assert '8.1.1-essentials_build-www.gyan.dev' in ffmpeg_build
    files = {str(path.relative_to(RUNTIME)).replace('\\','/'): {'bytes':path.stat().st_size,'sha256':sha(path)} for path in sorted(RUNTIME.rglob('*')) if path.is_file()}
    assert not any(Path(name).suffix.lower() in {'.cube','.mp4','.mkv','.pkl','.pt','.onnx'} or Path(name).name in {'last_args.txt','config.json'} for name in files)
    for name in ('smoothie/recipe.ini','smoothie/defaults.ini'):
     text=(RUNTIME/name).read_text(); assert 'C:/Users/' not in text and 'C:\\Users\\' not in text
    source_files = {relative: {'bytes': (UI/'src-tauri/resources'/relative).stat().st_size,
                               'sha256': sha(UI/'src-tauri/resources'/relative)}
                    for relative in SOURCE_RESOURCES}
    source_files['THIRD_PARTY_NOTICES.md'] = {'bytes': (UI/'THIRD_PARTY_NOTICES.md').stat().st_size,
                                              'sha256': sha(UI/'THIRD_PARTY_NOTICES.md')}
    manifest = {
        'version': VERSION,
        'smoothie_release': TAG,
        'smoothie_source_commit': COMMIT,
        'archive_sha256': sha(ARCHIVE),
        'smoothie_archive_url': f'https://github.com/couleur-tweak-tips/smoothie-rs/releases/download/{TAG}/smoothie-rs-nightly.zip',
        'ffmpeg_version': '8.1.1-essentials_build-www.gyan.dev',
        'ffmpeg_build': ffmpeg_build,
        'ffmpeg_archive_url': FFMPEG_URL,
        'ffmpeg_archive_sha256': FFMPEG_SHA256,
        'vapoursynth_version': 'R70',
        'licence_artifacts': licence_artifacts,
        'python_package_url': PYTHON_PACKAGE['url'],
        'python_package_sha256': PYTHON_PACKAGE['sha256'],
        'rife_optional_environment': {
            'bundled': False,
            'profile_sha256': sha(profile_path),
            'requirements_lock_sha256': sha(lock_path),
            'python_package': PYTHON_PACKAGE,
            'profile': profile,
        },
        'source_files': source_files,
        'files': files,
    }
    (KIT/'RELEASE-PAYLOAD.json').write_text(json.dumps(manifest,indent=2)+'\n',encoding='utf-8')
    relative_kit = KIT.relative_to(UI/'src-tauri').as_posix()
    resources = {f'{relative_kit}/runtime':'resources/runtime','../THIRD_PARTY_NOTICES.md':'resources/THIRD_PARTY_NOTICES.md', f'{relative_kit}/RELEASE-PAYLOAD.json':'resources/RELEASE-PAYLOAD.json'}
    for relative in SOURCE_RESOURCES:
     resources['resources/'+relative]='resources/'+relative
    (KIT/'build.json').write_text(json.dumps({'build':{'beforeBuildCommand':'node node_modules/vite/bin/vite.js build'},'bundle':{'resources':resources}},indent=2),encoding='utf-8')
    print(f'Prepared {len(files)} pinned runtime files for {VERSION}; optional RIFE profile {profile["profileId"]}.')


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--kit', type=Path, default=UI/'src-tauri/target'/f'release-kit-v{VERSION}',
                        help='Release kit below src-tauri/target (defaults to the current version).')
    args = parser.parse_args()
    prepare_release(args.kit)


if __name__ == '__main__':
    main()
