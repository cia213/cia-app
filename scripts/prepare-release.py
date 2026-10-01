"""Prepare a clean release payload from the pinned, tested Smoothie archive."""
import hashlib
import json
from pathlib import Path
import shutil
import subprocess
import urllib.request
import zipfile

UI = Path(__file__).resolve().parents[1]
VERSION = json.loads((UI/'src-tauri/tauri.conf.json').read_text())['version']
KIT = UI/'src-tauri/target'/f'release-kit-v{VERSION}'
RUNTIME = KIT/'runtime'
ARCHIVE = KIT/'smoothie-rs-nightly.zip'
PYTHON_INSTALLER = UI/'src-tauri/resources/bootstrap/python-3.11.9-amd64.exe'
TAG = 'Nightly_2025.11.30_14-36'
COMMIT = '3da73de9df0c991801c4f8474b320445fb25a208'

def sha(path): return hashlib.sha256(path.read_bytes()).hexdigest()
assert sha(PYTHON_INSTALLER) == '5ee42c4eee1e6b4464bb23722f90b45303f79442df63083f05322f1785f5fdde'
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
shutil.copytree(UI/'src-tauri/resources/runtime/ffmpeg',RUNTIME/'ffmpeg',dirs_exist_ok=True)

urls = {
 'smoothie/COPYING.GPLv3.txt': f'https://raw.githubusercontent.com/couleur-tweak-tips/smoothie-rs/{COMMIT}/LICENSE',
 'smoothie/VAPOURSYNTH-COPYING.LESSER': 'https://raw.githubusercontent.com/vapoursynth/vapoursynth/R70/COPYING.LESSER',
}
for relative,url in urls.items():
 with urllib.request.urlopen(url) as response: (RUNTIME/relative).write_bytes(response.read())
source_notes = f'''# Runtime source availability\n\nSmoothie portable {TAG}, source commit {COMMIT}:\nhttps://github.com/couleur-tweak-tips/smoothie-rs/tree/{COMMIT}\nhttps://github.com/couleur-tweak-tips/smoothie-rs/archive/{COMMIT}.tar.gz\n\nVapourSynth R70: https://github.com/vapoursynth/vapoursynth/tree/R70\nThe portable archive's scripts, DLLs and licence notices are retained.\nOnly recipe/default settings are replaced with neutral app settings; caches and user sidecars are excluded.\n\nFFmpeg / FFprobe 8.1.1 essentials, Gyan.dev GPLv3 build:\nhttps://www.gyan.dev/ffmpeg/builds/\nMatching upstream source: https://ffmpeg.org/releases/ffmpeg-8.1.1.tar.xz\nBuild configuration and exact executable hashes are recorded in RELEASE-PAYLOAD.json.\nThird-party libraries retain their own notices and source repositories; see Gyan's library/build documentation.\n'''
(RUNTIME/'SOURCE-AVAILABILITY.md').write_text(source_notes,encoding='utf-8')
ffmpeg_build = subprocess.check_output([str(RUNTIME/'ffmpeg/ffmpeg.exe'), '-version'], text=True)
assert '8.1.1-essentials_build-www.gyan.dev' in ffmpeg_build
files = {str(path.relative_to(RUNTIME)).replace('\\','/'): {'bytes':path.stat().st_size,'sha256':sha(path)} for path in sorted(RUNTIME.rglob('*')) if path.is_file()}
assert not any(Path(name).suffix.lower() in {'.cube','.mp4','.mkv','.pkl','.pt','.onnx'} or Path(name).name in {'last_args.txt','config.json'} for name in files)
for name in ('smoothie/recipe.ini','smoothie/defaults.ini'):
 text=(RUNTIME/name).read_text(); assert 'C:/Users/' not in text and 'C:\\Users\\' not in text
manifest = {'version':VERSION,'smoothie_release':TAG,'smoothie_source_commit':COMMIT,'archive_sha256':sha(ARCHIVE),'ffmpeg_version':'8.1.1-essentials_build-www.gyan.dev','ffmpeg_build':ffmpeg_build,'vapoursynth_version':'R70','python_installer_sha256':sha(PYTHON_INSTALLER),'files':files}
(KIT/'RELEASE-PAYLOAD.json').write_text(json.dumps(manifest,indent=2)+'\n',encoding='utf-8')
resources = {f'target/release-kit-v{VERSION}/runtime':'resources/runtime', 'resources/time_remap.py':'resources/time_remap.py','resources/rife_worker.py':'resources/rife_worker.py','resources/bootstrap/bootstrap-rife.ps1':'resources/bootstrap/bootstrap-rife.ps1','resources/bootstrap/python-3.11.9-amd64.exe':'resources/bootstrap/python-3.11.9-amd64.exe','../THIRD_PARTY_NOTICES.md':'resources/THIRD_PARTY_NOTICES.md', f'target/release-kit-v{VERSION}/RELEASE-PAYLOAD.json':'resources/RELEASE-PAYLOAD.json'}
(KIT/'build.json').write_text(json.dumps({'build':{'beforeBuildCommand':'node node_modules/vite/bin/vite.js build'},'bundle':{'resources':resources}},indent=2),encoding='utf-8')
print(f'Prepared {len(files)} clean runtime files; no binary changes versus tested installation.')
