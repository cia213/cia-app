# RIFE installation fix

## Fixed defect

The v1.2.5 script ran the official Python Windows installer silently with a
private target directory. If the same Python version was already registered,
the installer could enter maintenance mode and exit without creating that
directory. The script then failed because `python/python.exe` was missing.
This behaviour is [documented by CPython](https://github.com/python/cpython/issues/86358).

PowerShell displayed the error over multiple lines. The Rust backend sometimes
retained only the final line containing the end of the path. The UI then added
a second failure prefix, producing a repeated and truncated diagnostic.

## New behaviour

The bootstrap uses the [official Python NuGet package](https://docs.python.org/3.11/using/windows.html#the-nuget-org-packages),
version 3.11.9 x64, which contains the standard library, `venv`, and `ensurepip`.
The archive's SHA-256 is verified before extraction into the user's RIFE data
directory. No MSI installation runs: other Python installations, their registry
entries, file associations, and the user `PATH` remain unchanged.

A valid private Python installation is reused. An incomplete copy is replaced
only after verifying the package and the new executable; the previous copy is
restored if replacement fails. DLL and standard-library checks prevent a
partial installation from being mistaken for a ready environment.

Script errors use a structured `CIA_INSTALL_ERROR` line. Rust preserves the full
message; the UI displays it once and keeps it on the interpolation page until
the next attempt. The log remains available to copy. Application text and
installation steps remain in English. Command identifiers, configuration keys,
progress markers, software names, and original licence texts are preserved.

## Verification

- Real Windows test with spaces and accented characters in paths: private
  Python, its virtual environment, pip, and SSL work alongside an already
  registered Python 3.11. Reuse avoids another download.
- Removal of a DLL to simulate an incomplete installation: the copy is repaired
  and the existing virtual environment still works.
- A package with the wrong hash is rejected before extraction; deletion outside
  the RIFE directory is refused; the Python registry and user `PATH` are
  identical before and after the tests.
- Complete RIFE installation in a fresh test directory: all seven stages pass,
  model 4.26 is verified, and PyTorch `2.5.1+cu121` is available on the RTX 3050.
- Real interpolation with that environment: 12 frames at 24 FPS produce 24 frames
  at 48 FPS in 320×192; video and audio each last 0.50 s.
- Rust and UI tests cover the complete structured diagnostic despite extra
  PowerShell lines, absence of a duplicated prefix, and a successful retry
  clearing the persistent error. Build and static analysis were verified.
- The v1.2.6 installer was extracted and both bootstrap scripts matched the
  tracked source byte for byte. Its packaged setup completed all seven stages
  from an unrelated working directory, with spaces and accents in runtime paths.
- The packaged setup repaired a deliberately broken virtual environment, then
  its installed RIFE runtime completed a real 24 → 48 FPS interpolation render
  with synchronized audio. The registered Python and user PATH were unchanged.
- The English UI was checked in a browser with the Tauri boundary mocked. NSIS
  installation and full GUI navigation on a fresh Windows user profile were not
  tested; the packaged scripts were executed directly from the extracted installer.

The fix ships in [v1.2.6](https://github.com/cia213/cia-app/releases/tag/v1.2.6).
The older v1.2.5 installer is unchanged. Users seeing this failure should update
to v1.2.6 and retry INSTALL ENVIRONMENT.
