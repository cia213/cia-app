# cia render v1.2.1

- **FFmpeg access in subprocesses**: automatically adds the bundled FFmpeg/FFprobe directory to child Python processes' `PATH`.
- **skvideo and Practical-RIFE compatibility**: fixes video-decoding `AssertionError` on machines without a global FFmpeg installation.
- **Automatic updates**: signed package for in-app updates.
