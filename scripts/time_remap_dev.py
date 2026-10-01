"""Development entry point shared with the local time-remap-app directory."""
from pathlib import Path
import runpy

parent = Path(__file__).resolve().parent.parent
candidates = [parent / 'src-tauri/resources/time_remap.py',
              parent / 'time-remap-ui/src-tauri/resources/time_remap.py']
source = next((candidate for candidate in candidates if candidate.is_file()), None)
if source is None:
    raise SystemExit('Cannot find the cia render orchestration source')
globals().update(runpy.run_path(str(source), run_name='__main__' if __name__ == '__main__' else 'cia_remap'))
