"""Exercise a packaged cia render app through native WebView2 and real Tauri IPC.

Requires tauri-driver and Microsoft Edge WebDriver matching the WebView2 runtime.
No Selenium/Python packages are needed. This script does not install NSIS, reset
native configuration, download RIFE, edit media, or claim a clean Windows profile.
Pass generated media in an isolated writable folder for optional render tests.

References:
https://v2.tauri.app/develop/tests/webdriver/manual-setup/
https://learn.microsoft.com/en-us/microsoft-edge/webview2/how-to/webdriver
https://www.w3.org/TR/webdriver2/
"""

from __future__ import annotations

import argparse
import base64
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import socket
import subprocess
import sys
import time
import urllib.error
import urllib.request
import zipfile


ELEMENT = "element-6066-11e4-a52e-4f735466cecf"
ROOT = Path(__file__).resolve().parents[2]
VERSION = re.compile(r"^\d+\.\d+\.\d+\.\d+$")


def sha256(filename: Path) -> str:
    digest = hashlib.sha256()
    with filename.open("rb") as source:
        for chunk in iter(lambda: source.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def free_port() -> int:
    with socket.socket() as connection:
        connection.bind(("127.0.0.1", 0))
        return int(connection.getsockname()[1])


def installed_webview_version() -> str:
    import winreg

    key = r"SOFTWARE\Microsoft\EdgeUpdate\Clients\{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}"
    for hive in (winreg.HKEY_LOCAL_MACHINE, winreg.HKEY_CURRENT_USER):
        for view in (winreg.KEY_WOW64_32KEY, winreg.KEY_WOW64_64KEY):
            try:
                with winreg.OpenKey(hive, key, 0, winreg.KEY_READ | view) as handle:
                    value = str(winreg.QueryValueEx(handle, "pv")[0]).strip()
                if VERSION.fullmatch(value):
                    return value
            except OSError:
                continue
    raise RuntimeError("WebView2 version was not found. Supply --webview-version for a fixed runtime.")


def get_url(url: str, maximum: int) -> bytes:
    request = urllib.request.Request(url, headers={"User-Agent": "cia-render-native-qualification/1"})
    with urllib.request.urlopen(request, timeout=45) as response:
        data = response.read(maximum + 1)
    if len(data) > maximum:
        raise RuntimeError("Microsoft driver response exceeded the expected size limit")
    return data


def download_driver(version: str, destination: Path) -> tuple[Path, str]:
    """Use the detected exact runtime version, with a checked release-API fallback."""
    selected = version
    try:
        data = get_url(f"https://msedgedriver.microsoft.com/{selected}/edgedriver_win64.zip", 100 * 1024 * 1024)
    except urllib.error.HTTPError as error:
        if error.code != 404:
            raise
        release = get_url(f"https://msedgedriver.microsoft.com/LATEST_RELEASE_{version.split('.')[0]}_WINDOWS", 1024)
        selected = release.decode("utf-16" if release.startswith((b"\xff\xfe", b"\xfe\xff")) else "utf-8-sig").strip()
        if not VERSION.fullmatch(selected) or selected.split(".")[:3] != version.split(".")[:3]:
            raise RuntimeError("Microsoft release API did not return a matching WebView2 driver build") from error
        data = get_url(f"https://msedgedriver.microsoft.com/{selected}/edgedriver_win64.zip", 100 * 1024 * 1024)
    folder = destination / "drivers" / selected
    folder.mkdir(parents=True, exist_ok=True)
    archive = folder / "edgedriver_win64.zip"
    archive.write_bytes(data)
    filename = folder / "msedgedriver.exe"
    with zipfile.ZipFile(archive) as package:
        entry = package.getinfo("msedgedriver.exe")
        if entry.file_size > 100 * 1024 * 1024:
            raise RuntimeError("Unexpected Edge WebDriver executable size")
        # Extract only the named executable, never arbitrary archive paths.
        filename.write_bytes(package.read(entry))
    return filename, selected


class WebDriver:
    def __init__(self, origin: str, timeout: float):
        self.origin = origin
        self.timeout = timeout
        self.session: str | None = None

    def request(self, method: str, route: str, payload=None):
        body = None if payload is None else json.dumps(payload).encode("utf-8")
        request = urllib.request.Request(self.origin + route, data=body, method=method, headers={"Content-Type": "application/json"})
        try:
            with urllib.request.urlopen(request, timeout=self.timeout) as response:
                result = json.load(response)
        except urllib.error.HTTPError as error:
            detail = error.read().decode("utf-8", "replace")
            raise RuntimeError(f"WebDriver {method} {route}: {detail[:2000]}") from error
        value = result.get("value")
        if isinstance(value, dict) and value.get("error"):
            raise RuntimeError(f"WebDriver {value['error']}: {value.get('message', '')[:2000]}")
        return value

    def command(self, route: str, payload=None, method="POST"):
        return self.request(method, f"/session/{self.session}{route}", payload)

    def execute(self, script: str, *args):
        return self.command("/execute/sync", {"script": script, "args": list(args)})

    def invoke(self, command: str, args=None):
        result = self.command("/execute/async", {
            "script": "const done=arguments[arguments.length-1]; window.__TAURI_INTERNALS__.invoke(arguments[0],arguments[1]||{}).then(value=>done({ok:true,value})).catch(error=>done({ok:false,error:String(error)}));",
            "args": [command, args or {}],
        })
        if not result or not result.get("ok"):
            raise RuntimeError(f"Native {command} failed: {(result or {}).get('error', 'no reply')}")
        return result.get("value")

    def click_text(self, text: str):
        # All requested action names are literals owned by this test.
        xpath = f"//button[normalize-space()={json.dumps(text)}]"
        found = self.command("/element", {"using": "xpath", "value": xpath})
        self.command(f"/element/{found[ELEMENT]}/click", {})

    def wait(self, condition, description: str, timeout=None):
        end = time.monotonic() + (timeout or self.timeout)
        while time.monotonic() < end:
            if condition():
                return
            time.sleep(0.2)
        raise TimeoutError(f"Timed out waiting for {description}")

    def screenshot(self, filename: Path):
        filename.write_bytes(base64.b64decode(self.command("/screenshot", method="GET"), validate=True))


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--application", required=True, type=Path, help="Installed/extracted candidate application EXE, not the NSIS installer")
    parser.add_argument("--expected-version", default=json.loads((ROOT / "src-tauri/tauri.conf.json").read_text(encoding="utf-8"))["version"])
    parser.add_argument("--expected-speed", type=float, help="Require this saved speed; use 1.07 when qualifying an actual fresh profile")
    parser.add_argument("--tauri-driver", type=Path, default=Path(shutil.which("tauri-driver") or Path.home() / ".cargo/bin/tauri-driver.exe"))
    parser.add_argument("--edge-driver", type=Path)
    parser.add_argument("--download-edge-driver", action="store_true", help="Download a matching tool from Microsoft's service into the evidence directory")
    parser.add_argument("--webview-version", help="Explicit fixed-runtime version; otherwise query Windows WebView2 registration")
    parser.add_argument("--video", type=Path, help="Optional generated writable SDR fixture for a real UI render")
    parser.add_argument("--interpolate", action="store_true", help="Also interpolate the fixture using an already installed healthy environment")
    parser.add_argument("--attach", action="store_true", help="Launch the host explicitly and attach WebDriver to its local debugging port")
    parser.add_argument("--timeout", type=float, default=180)
    parser.add_argument("--output", type=Path, default=ROOT / "src-tauri/target/native-ui-evidence")
    args = parser.parse_args()
    if os.name != "nt":
        parser.error("Native WebView2 qualification must run on Windows")
    application = args.application.resolve(strict=True)
    tool = args.tauri_driver.resolve(strict=True)
    destination = args.output.resolve()
    destination.mkdir(parents=True, exist_ok=True)
    version = args.webview_version or installed_webview_version()
    if not VERSION.fullmatch(version):
        parser.error("--webview-version must be MAJOR.MINOR.BUILD.PATCH")
    if args.edge_driver:
        native_driver = args.edge_driver.resolve(strict=True)
        driver_version = subprocess.check_output([str(native_driver), "--version"], text=True, timeout=15).strip()
    elif args.download_edge_driver:
        native_driver, driver_version = download_driver(version, destination)
    else:
        parser.error("Supply --edge-driver or opt in to --download-edge-driver")
    parsed_driver_version = re.search(r"\d+\.\d+\.\d+\.\d+", driver_version)
    if not parsed_driver_version or parsed_driver_version.group().split(".")[:3] != version.split(".")[:3]:
        raise RuntimeError("Edge WebDriver does not match the detected WebView2 build")
    video = args.video.resolve(strict=True) if args.video else None
    if args.interpolate and not video:
        parser.error("--interpolate requires --video")
    port, native_port = free_port(), free_port()
    while native_port == port:
        native_port = free_port()
    evidence = {
        "application_sha256": sha256(application), "expected_version": args.expected_version,
        "native_ipc_mocked": False, "native_ui": True,
        "profile_boundary": "Current Windows user/known folders; isolated WebView user-data folder. Not a clean Windows VM or new Windows user.",
        "webview2_version": version, "edge_driver_version": driver_version,
        "edge_driver_sha256": sha256(native_driver), "checks": {},
    }
    driver = WebDriver(f"http://127.0.0.1:{port}", args.timeout)
    process = None
    application_process = None
    log = (destination / "tauri-driver.log").open("wb")
    try:
        environment = dict(os.environ)
        # The host and WebDriver must use the same folder: driver options alone
        # do not override the WebView2 environment created by a native host.
        environment["WEBVIEW2_USER_DATA_FOLDER"] = str(destination / "webview-data")
        debug_port = free_port()
        environment["TAURI_WEBVIEW_AUTOMATION"] = "true"
        if args.attach:
            environment["CIA_RENDER_AUTOMATION_PORT"] = str(debug_port)
        environment["WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS"] = f"--remote-debugging-port={debug_port if args.attach else 0} --remote-debugging-address=127.0.0.1"
        process = subprocess.Popen([str(tool), "--port", str(port), "--native-port", str(native_port), "--native-driver", str(native_driver)], stdout=log, stderr=subprocess.STDOUT, env=environment, creationflags=subprocess.CREATE_NO_WINDOW)
        def ready():
            if process.poll() is not None:
                raise RuntimeError("tauri-driver exited; inspect tauri-driver.log")
            try:
                return bool(driver.request("GET", "/status"))
            except (OSError, RuntimeError):
                return False
        driver.wait(ready, "WebDriver readiness", 30)
        if args.attach:
            application_process = subprocess.Popen([str(application)], stdout=log, stderr=subprocess.STDOUT, env=environment, creationflags=subprocess.CREATE_NO_WINDOW)
            def debug_ready():
                if application_process.poll() is not None:
                    raise RuntimeError("Native application exited before WebView2 attachment")
                try:
                    with urllib.request.urlopen(f"http://127.0.0.1:{debug_port}/json/version", timeout=2) as response:
                        return bool(json.load(response).get("webSocketDebuggerUrl"))
                except OSError:
                    return False
            driver.wait(debug_ready, "native WebView2 debugging endpoint", 30)
            capabilities = {"browserName": "webview2", "ms:edgeOptions": {"debuggerAddress": f"127.0.0.1:{debug_port}"}}
        else:
            capabilities = {"browserName": "wry", "tauri:options": {"application": str(application), "webviewOptions": {"userDataFolder": str(destination / "webview-data")}}}
        evidence["attachment_mode"] = "explicit host launch and local WebDriver attach" if args.attach else "WebDriver host launch"
        session = driver.request("POST", "/session", {"capabilities": {"alwaysMatch": capabilities}})
        driver.session = session["sessionId"]
        driver.command("/timeouts", {"script": int(args.timeout * 1000), "implicit": 0, "pageLoad": int(args.timeout * 1000)})
        driver.wait(lambda: driver.execute("return !!document.querySelector('.tab-bar') && !!window.__TAURI_INTERNALS__;"), "compiled UI and native IPC")
        driver.execute("window.ciaQualificationCsp=[]; document.addEventListener('securitypolicyviolation',e=>window.ciaQualificationCsp.push({directive:e.violatedDirective,blocked:e.blockedURI}));")
        actual_version = driver.invoke("get_app_version")
        assert actual_version == args.expected_version, actual_version
        evidence["checks"]["native_version"] = actual_version
        snapshot = driver.invoke("get_runtime_snapshot")
        assert snapshot["smoothieReady"] and snapshot["mediaToolsReady"] and not snapshot.get("loadError"), "Bundled Render tools/configuration are not ready"
        evidence["checks"]["bundled_render_ready"] = True
        evidence["checks"]["rife_health_status"] = snapshot.get("rifeHealth", {}).get("status")
        duplicate = subprocess.Popen([str(application)], stdout=log, stderr=subprocess.STDOUT, env=environment, creationflags=subprocess.CREATE_NO_WINDOW)
        try:
            assert duplicate.wait(timeout=15) == 0, "The second application instance did not exit successfully"
        except subprocess.TimeoutExpired:
            duplicate.terminate()
            duplicate.wait(timeout=5)
            raise RuntimeError("The second application instance remained active")
        assert driver.invoke("get_app_version") == args.expected_version
        evidence["checks"]["second_instance_exits_and_existing_instance_survives"] = True
        driver.click_text("ABOUT")
        driver.wait(lambda: driver.execute("return !!document.querySelector('.about-page');"), "About page")
        assert driver.execute("return document.querySelectorAll('.about-link-card').length;") == 8
        assert driver.execute("return [...document.querySelectorAll('button')].every(e=>e.textContent.trim()!=='ADVANCED RUNTIME PATHS');")
        evidence["checks"]["about_credits_and_removed_button"] = True
        driver.screenshot(destination / "about-native.png")
        driver.click_text("RENDER")
        driver.wait(lambda: driver.execute("return !!document.querySelector('.drop-zone');"), "Render source picker")
        if video:
            driver.invoke("plugin:event|emit", {"event": "tauri://drag-drop", "payload": {"paths": [str(video)]}})
            driver.wait(lambda: driver.execute("return [...document.querySelectorAll('button')].some(e=>e.textContent.trim()==='START RENDER');"), "real source analysis/preview")
            driver.click_text("DETAILS")
            speed = float(driver.execute("return document.querySelector('#mod-sm-speed').value;"))
            assert 0.5 <= speed <= 2.0, "Saved playback speed is invalid"
            if args.expected_speed is not None:
                assert speed == args.expected_speed, "Saved playback speed differs from the requested qualification value"
            evidence["checks"]["saved_render_speed"] = speed
            assert driver.execute("return !!document.querySelector('.lut-picker');")
            driver.command("/execute/sync", {"script": "document.querySelector('[aria-label=\"Close render settings\"]').click();", "args": []})
            driver.click_text("START RENDER")
            def render_finished():
                failure = driver.execute("return document.querySelector('.failure-card[role=alert]')?.textContent || ''; ")
                if failure:
                    raise RuntimeError(f"Native UI render failed: {failure[:2000]}")
                return driver.execute("return !!document.querySelector('.completion-output-name');")
            driver.wait(render_finished, "real bundled Smoothie render")
            evidence["checks"]["real_ui_render"] = True
            evidence["checks"]["source_selection_boundary"] = "Real Tauri drag-drop event emitted by harness; native OS file picker/drag gesture not tested."
            driver.screenshot(destination / "render-native.png")
            if args.interpolate:
                assert snapshot.get("rifeReady"), "Interpolation requires a healthy preinstalled environment"
                driver.click_text("INTERPOLATION")
                driver.wait(lambda: driver.execute("return !!document.querySelector('.drop-zone');"), "Interpolation source picker")
                assert driver.execute("return !document.querySelector('.runtime-toolbar') && !document.querySelector('[role=dialog]') && [...document.querySelectorAll('button')].every(e=>!['RECHECK','REPAIR ENVIRONMENT'].includes(e.textContent.trim()));")
                evidence["checks"]["healthy_interpolation_has_no_maintenance_controls_or_popup"] = True
                driver.screenshot(destination / "interpolation-ready-native.png")
                driver.invoke("plugin:event|emit", {"event": "tauri://drag-drop", "payload": {"paths": [str(video)]}})
                driver.wait(lambda: driver.execute("return [...document.querySelectorAll('button')].some(e=>e.textContent.trim()==='START INTERPOLATION');"), "interpolation analysis")
                driver.click_text("START INTERPOLATION")
                driver.wait(render_finished, "real RIFE interpolation")
                evidence["checks"]["real_ui_interpolation"] = True
                driver.screenshot(destination / "interpolation-native.png")
                driver.click_text("NEW RENDER")
                invalid_video = destination / "invalid diagnostic fixture.mp4"
                invalid_video.write_bytes(b"Not a video: native error popup qualification.")
                driver.invoke("plugin:event|emit", {"event": "tauri://drag-drop", "payload": {"paths": [str(invalid_video)]}})
                driver.wait(lambda: driver.execute("return document.querySelector('[role=dialog]')?.getAttribute('aria-labelledby')==='failure-title';"), "native error popup")
                report = driver.execute("return document.querySelector('.bug-report-preview pre').textContent;")
                assert "Error:" in report and "[local path]" in report
                assert str(destination) not in report and "invalid diagnostic fixture.mp4" not in report
                evidence["checks"]["real_analysis_failure_opens_redacted_report_popup"] = True
                evidence["checks"]["bug_report_not_submitted"] = True
                driver.screenshot(destination / "error-popup-native.png")
                driver.execute("document.querySelector('[aria-label=\"Close error report\"]').click();")
        assert not driver.execute("return window.ciaQualificationCsp;")
        evidence["checks"]["csp_violations_during_navigation_and_operations"] = 0
        evidence["status"] = "passed"
        print("Native UI qualification passed; evidence recorded locally.")
        return 0
    except Exception as error:
        evidence["status"] = "failed"
        evidence["error"] = str(error)
        if driver.session:
            try:
                driver.screenshot(destination / "failure-native.png")
            except Exception:
                pass
        print(f"Native UI qualification failed: {type(error).__name__}. Inspect local evidence.", file=sys.stderr)
        return 1
    finally:
        if driver.session:
            try:
                driver.command("", method="DELETE")
            except Exception:
                pass
        if process:
            process.terminate()
            try:
                process.wait(timeout=10)
            except subprocess.TimeoutExpired:
                process.kill()
                process.wait(timeout=5)
        if application_process and application_process.poll() is None:
            application_process.terminate()
            try:
                application_process.wait(timeout=10)
            except subprocess.TimeoutExpired:
                application_process.kill()
                application_process.wait(timeout=5)
        log.close()
        (destination / "NATIVE-UI-VALIDATION.json").write_text(json.dumps(evidence, indent=2) + "\n", encoding="utf-8")


if __name__ == "__main__":
    raise SystemExit(main())
