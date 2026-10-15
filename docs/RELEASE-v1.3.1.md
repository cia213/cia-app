# cia render v1.3.1

This small interface update keeps environment maintenance out of normal use and
makes genuine failures easier to report.

- **Quiet when ready.** The healthy interpolation page no longer shows
  **ENVIRONMENT READY**, **RECHECK** or **REPAIR ENVIRONMENT**. Background
  verification remains active; first-time setup and recovery for actual problems
  remain available.
- **Error-only popup.** An operation failure opens a popup with the error and
  reporting actions. Ordinary use and deliberate cancellation do not open a bug
  popup, and saved diagnostics remain available after dismissal.
- **Optional developer report.** Copy a report with local paths and common
  credentials masked, or open a prefilled GitHub issue draft. Review and submit
  the draft on GitHub to send it to the developer. Long drafts are shortened to
  fit browser URL limits; **COPY REPORT** keeps the complete report.

There is no automatic telemetry or background upload of diagnostics. The update
does not change interpolation, rendering, or Rubber Band speed processing.
