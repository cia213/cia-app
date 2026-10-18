# cia render v1.3.2

- **A usable workspace at launch.** The quick render-tool check no longer waits
  for the complete RIFE/CUDA test. Navigation, clip selection, Details and classic
  rendering stay available while interpolation is checked in the background.
- **Visible, non-blocking progress.** A bottom-right notification shows the real
  check stage, verified file counts, setup steps and downloaded bytes. Unknown
  durations use an indeterminate bar, without an invented overall percentage.
- **Automatic setup and repair.** Missing or broken interpolation components
  start the verified installer automatically. Invalid saved render paths fall
  back to bundled tools, and invalid configuration is backed up before recovery.
  The main workspace shows component updates and available application updates.
- **Independent operations.** One classic render can run alongside component
  installation, with separate process ownership, cancellation and diagnostics.
  Interpolation waits for a verified environment; application updates wait for
  active work to finish. Settings edited during a check are preserved.
- **Failures remain non-modal.** Unsupported hardware or failed repairs appear
  in the notification. Error details and a GitHub report are available on request.
  Automatic repair makes one attempt per launch; Retry starts another attempt.

The pinned interpolation profile, render processing and pitch-preserving Rubber
Band speed control are unchanged. Component installation still checks GPU,
driver and disk requirements before large downloads. Missing packaged render
files require reinstalling the application; RIFE setup cannot replace them.
