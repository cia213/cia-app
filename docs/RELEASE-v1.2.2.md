# cia render v1.2.2

- **RIFE processes**: live threaded stdout/stderr forwarding, unbuffered Python output, and explicit process working directory.
- **Windows path normalization**: removes extended `\\?\` prefixes from tool paths and `PATH` entries to prevent Win32 loading errors and skvideo lookup failures.
- **Complete diagnostics**: Tauri collects error logs to show Python tracebacks and failure details instead of only an exit code.
- **Automatic updates**: signed installer and updated `latest.json` manifest.
