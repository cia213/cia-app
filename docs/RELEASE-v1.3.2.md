# cia render v1.3.2

- **Manual component checks.** The full RIFE/CUDA diagnostic no longer runs at
  launch. Use **CHECK COMPONENTS** beside **CHECK FOR UPDATES** in About. Existing
  configured interpolation remains usable without running a diagnostic every
  session; each interpolation job still validates its runtime before processing.
- **Visible, non-blocking progress.** The manual check opens a bottom-right
  notification with the real stage, verified file counts, setup steps and
  downloaded bytes. Navigation, clip selection, Details and classic rendering
  stay available. Unknown durations use an indeterminate bar.
- **Automatic setup and repair after checking.** Missing or broken interpolation
  components start the verified installer without another confirmation. Invalid
  saved render paths fall back to bundled tools; invalid configuration is backed
  up before recovery. The workspace shows component and application updates.
- **Independent operations.** One classic render can run alongside component
  installation, with separate process ownership, cancellation and diagnostics.
  Interpolation waits during diagnostics or installation; application updates
  wait for active work. Settings edited during a check are preserved.
- **Failures remain non-modal.** Unsupported hardware or failed repairs appear
  in the notification. Error details and a GitHub report are available on request.
  Repair makes one automatic attempt per manual check; Retry starts another.

The pinned interpolation profile, render processing and pitch-preserving Rubber
Band speed control are unchanged. Component installation still checks GPU,
driver and disk requirements before large downloads. Missing packaged render
files require reinstalling the application; RIFE setup cannot replace them.
