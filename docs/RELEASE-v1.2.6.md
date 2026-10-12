# cia render v1.2.6

This build fixes **INSTALL ENVIRONMENT** failures on the interpolation page,
including installations that stopped with a path ending in `rife/python/python.exe`.

- Python 3.11.9 x64 is now extracted from the official package into the private RIFE directory after SHA-256 verification. An existing system Python no longer redirects setup into maintenance mode.
- A missing or incomplete private Python runtime is repaired; a valid runtime is reused. Existing system Python installations and the user PATH are left intact.
- Setup reports the complete failure cause once and keeps it visible on the interpolation page. **COPY LOGS** provides the full installation log for retry and troubleshooting.
- Both bootstrap scripts are included in the Windows installer. Application text remains in English.

RIFE remains optional and requires a CUDA-capable NVIDIA GPU and an Internet
connection to download its environment and model. RENDER continues to use the
bundled Smoothie and media tools. Render speed, Rubber Band audio processing,
and the last-frame hold correction remain unchanged.

[Installation fix and verification](https://github.com/cia213/cia-app/blob/main/docs/CORRECTION-INSTALLATION-RIFE.md).
