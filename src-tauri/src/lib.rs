use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::env;
use std::fs::{self, OpenOptions};
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::Mutex;
use tauri::{Emitter, Manager};
use tokio::io::{AsyncRead, AsyncReadExt};
use tokio::process::Command;

#[cfg(target_os = "windows")]
const CREATE_NO_WINDOW: u32 = 0x08000000;

const CONFIG_SCHEMA_VERSION: u32 = 1;
const LEGACY_IDENTIFIER: &str = "com.ciarender.desktop";
const PROJECT_REPOSITORY_URL: &str = "https://github.com/cia213/cia-app";
const ABOUT_URLS: [&str; 9] = [
    PROJECT_REPOSITORY_URL,
    "https://github.com/hzwer/Practical-RIFE",
    "https://github.com/couleur-tweak-tips/smoothie-rs",
    "https://github.com/vapoursynth/vapoursynth",
    "https://github.com/FFmpeg/FFmpeg",
    "https://github.com/tauri-apps/tauri",
    "https://github.com/sveltejs/svelte",
    "https://github.com/IBM/plex",
    "https://github.com/n00mkrad/flowframes",
];

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase", default)]
struct RuntimeConfig {
    schema_version: u32,
    rife: RifeConfig,
    smoothie: SmoothieConfig,
    media_tools: MediaToolsConfig,
    ui: UiSettings,
}

impl Default for RuntimeConfig {
    fn default() -> Self {
        Self {
            schema_version: CONFIG_SCHEMA_VERSION,
            rife: RifeConfig::default(),
            smoothie: SmoothieConfig::default(),
            media_tools: MediaToolsConfig::default(),
            ui: UiSettings::default(),
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(rename_all = "camelCase", default)]
struct RifeConfig {
    python_executable: Option<String>,
    script: Option<String>,
    directory: Option<String>,
    model_file: Option<String>,
}

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(rename_all = "camelCase", default)]
struct SmoothieConfig {
    root: Option<String>,
    executable: Option<String>,
    recipe: Option<String>,
    lut_file: Option<String>,
}

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(rename_all = "camelCase", default)]
struct MediaToolsConfig {
    ffmpeg: Option<String>,
    ffprobe: Option<String>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase", default)]
struct UiSettings {
    migrated: bool,
    auto_render: bool,
    rife_settings: Value,
    smoothie_settings: Value,
}

impl Default for UiSettings {
    fn default() -> Self {
        Self {
            migrated: false,
            auto_render: false,
            rife_settings: json!({}),
            smoothie_settings: json!({}),
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
struct ComponentStatus {
    id: String,
    label: String,
    ready: bool,
    path: Option<String>,
    detail: String,
    expected: String,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
struct RuntimeSnapshot {
    config: RuntimeConfig,
    detected: RuntimeConfig,
    components: Vec<ComponentStatus>,
    rife_ready: bool,
    smoothie_ready: bool,
    media_tools_ready: bool,
    load_error: Option<String>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
struct VideoInfo {
    width: u32,
    height: u32,
    fps: f64,
    duration: f64,
    has_audio: bool,
}

struct MediaToolPaths {
    ffmpeg: PathBuf,
    ffprobe: PathBuf,
}

struct RifeRuntimePaths {
    python: PathBuf,
    script: PathBuf,
    directory: PathBuf,
    media: MediaToolPaths,
}

struct SmoothieRuntimePaths {
    root: PathBuf,
    executable: PathBuf,
    ffmpeg_directory: PathBuf,
}

struct OutputReservation {
    output: PathBuf,
    lock: PathBuf,
}

impl Drop for OutputReservation {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.lock);
    }
}

#[derive(Clone, Copy)]
struct RenderJob {
    process_id: u32,
    paused: bool,
    cancel_requested: bool,
}

#[derive(Default)]
struct JobRegistry {
    jobs: Mutex<HashMap<String, RenderJob>>,
}

fn register_job(registry: &tauri::State<JobRegistry>, job_id: &str, process_id: u32) -> Result<(), String> {
    if job_id.trim().is_empty() {
        return Err("A render job identifier is required".to_string());
    }
    let mut jobs = registry.jobs.lock().map_err(|_| "Render job registry is unavailable")?;
    if jobs.contains_key(job_id) {
        return Err("A render job with this identifier is already running".to_string());
    }
    jobs.insert(
        job_id.to_string(),
        RenderJob {
            process_id,
            paused: false,
            cancel_requested: false,
        },
    );
    Ok(())
}

fn running_job(registry: &tauri::State<JobRegistry>, job_id: &str) -> Result<RenderJob, String> {
    registry
        .jobs
        .lock()
        .map_err(|_| "Render job registry is unavailable")?
        .get(job_id)
        .copied()
        .ok_or_else(|| "The render job is no longer running".to_string())
}

fn finish_job(registry: &tauri::State<JobRegistry>, job_id: &str) -> Result<Option<RenderJob>, String> {
    Ok(registry
        .jobs
        .lock()
        .map_err(|_| "Render job registry is unavailable")?
        .remove(job_id))
}

#[cfg(target_os = "windows")]
mod process_control {
    use std::collections::VecDeque;
    use std::ffi::c_void;
    use std::mem::size_of;

    type Handle = *mut c_void;
    const INVALID_HANDLE_VALUE: Handle = -1isize as Handle;
    const TH32CS_SNAPPROCESS: u32 = 0x0000_0002;
    const PROCESS_SUSPEND_RESUME: u32 = 0x0000_0800;

    #[repr(C)]
    struct ProcessEntry32W {
        dw_size: u32,
        cnt_usage: u32,
        th32_process_id: u32,
        th32_default_heap_id: usize,
        th32_module_id: u32,
        cnt_threads: u32,
        th32_parent_process_id: u32,
        pc_pri_class_base: i32,
        dw_flags: u32,
        sz_exe_file: [u16; 260],
    }

    #[link(name = "kernel32")]
    extern "system" {
        fn CreateToolhelp32Snapshot(flags: u32, process_id: u32) -> Handle;
        fn Process32FirstW(snapshot: Handle, entry: *mut ProcessEntry32W) -> i32;
        fn Process32NextW(snapshot: Handle, entry: *mut ProcessEntry32W) -> i32;
        fn OpenProcess(access: u32, inherit_handle: i32, process_id: u32) -> Handle;
        fn CloseHandle(handle: Handle) -> i32;
    }

    #[link(name = "ntdll")]
    extern "system" {
        fn NtSuspendProcess(process: Handle) -> i32;
        fn NtResumeProcess(process: Handle) -> i32;
    }

    fn process_tree(root: u32) -> Result<Vec<u32>, String> {
        let snapshot = unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) };
        if snapshot == INVALID_HANDLE_VALUE {
            return Err("Unable to inspect the render process tree".to_string());
        }

        let result = (|| {
            let mut entry = ProcessEntry32W {
                dw_size: size_of::<ProcessEntry32W>() as u32,
                cnt_usage: 0,
                th32_process_id: 0,
                th32_default_heap_id: 0,
                th32_module_id: 0,
                cnt_threads: 0,
                th32_parent_process_id: 0,
                pc_pri_class_base: 0,
                dw_flags: 0,
                sz_exe_file: [0; 260],
            };
            let mut parents = Vec::new();
            if unsafe { Process32FirstW(snapshot, &mut entry) } != 0 {
                loop {
                    parents.push((entry.th32_process_id, entry.th32_parent_process_id));
                    entry.dw_size = size_of::<ProcessEntry32W>() as u32;
                    if unsafe { Process32NextW(snapshot, &mut entry) } == 0 {
                        break;
                    }
                }
            }

            let mut tree = vec![root];
            let mut queue = VecDeque::from([root]);
            while let Some(parent) = queue.pop_front() {
                for (process_id, parent_id) in &parents {
                    if *parent_id == parent && !tree.contains(process_id) {
                        tree.push(*process_id);
                        queue.push_back(*process_id);
                    }
                }
            }
            Ok(tree)
        })();
        unsafe { CloseHandle(snapshot) };
        result
    }

    pub fn set_process_tree_paused(root: u32, paused: bool) -> Result<(), String> {
        let mut tree = process_tree(root)?;
        if paused {
            tree.reverse();
        }
        let mut affected = 0usize;
        for process_id in tree {
            let handle = unsafe { OpenProcess(PROCESS_SUSPEND_RESUME, 0, process_id) };
            if handle.is_null() {
                continue;
            }
            let status = unsafe {
                if paused {
                    NtSuspendProcess(handle)
                } else {
                    NtResumeProcess(handle)
                }
            };
            unsafe { CloseHandle(handle) };
            if status >= 0 {
                affected += 1;
            }
        }
        if affected == 0 {
            return Err("The render process ended before it could be controlled".to_string());
        }
        Ok(())
    }
}

#[cfg(target_os = "windows")]
fn cancel_process_tree(process_id: u32) -> Result<(), String> {
    use std::os::windows::process::CommandExt;
    let status = std::process::Command::new("taskkill")
        .args(["/PID", &process_id.to_string(), "/T", "/F"])
        .creation_flags(CREATE_NO_WINDOW)
        .status()
        .map_err(|error| format!("Unable to cancel the render process: {error}"))?;
    if status.success() {
        Ok(())
    } else {
        Err("The render process ended before it could be cancelled".to_string())
    }
}

fn config_path(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    app.path()
        .app_config_dir()
        .map(|dir| dir.join("config.json"))
        .map_err(|error| format!("Unable to resolve the cia app config directory: {error}"))
}

fn migrate_legacy_config(app: &tauri::AppHandle) {
    let new_path = match config_path(app) {
        Ok(path) => path,
        Err(_) => return,
    };
    if new_path.exists() {
        return;
    }
    let legacy_path = match env::var_os("APPDATA") {
        Some(appdata) => PathBuf::from(appdata)
            .join(LEGACY_IDENTIFIER)
            .join("config.json"),
        None => return,
    };
    if !legacy_path.is_file() {
        return;
    }
    if let Some(parent) = new_path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    match fs::copy(&legacy_path, &new_path) {
        Ok(_) => println!(
            "[cia app] Migrated config from {} to {}",
            legacy_path.display(),
            new_path.display()
        ),
        Err(error) => eprintln!(
            "[cia app] Config migration failed: {} -> {}: {error}",
            legacy_path.display(),
            new_path.display()
        ),
    }
}

fn load_config(app: &tauri::AppHandle) -> Result<RuntimeConfig, String> {
    migrate_legacy_config(app);
    let path = config_path(app)?;
    if !path.exists() {
        return Ok(RuntimeConfig::default());
    }

    let raw = fs::read_to_string(&path)
        .map_err(|error| format!("Unable to read {}: {error}", path.display()))?;
    let config: RuntimeConfig =
        serde_json::from_str(&raw).map_err(|error| format!("Invalid config.json: {error}"))?;
    if config.schema_version != CONFIG_SCHEMA_VERSION {
        return Err(format!(
            "Unsupported config schema {} (expected {})",
            config.schema_version, CONFIG_SCHEMA_VERSION
        ));
    }
    Ok(config)
}

#[cfg(target_os = "windows")]
fn replace_file_atomically(source: &Path, destination: &Path) -> Result<(), String> {
    use std::iter::once;
    use std::os::windows::ffi::OsStrExt;

    #[link(name = "kernel32")]
    extern "system" {
        fn MoveFileExW(existing: *const u16, new: *const u16, flags: u32) -> i32;
    }

    const MOVEFILE_REPLACE_EXISTING: u32 = 0x0000_0001;
    let source_wide: Vec<u16> = source.as_os_str().encode_wide().chain(once(0)).collect();
    let destination_wide: Vec<u16> = destination
        .as_os_str()
        .encode_wide()
        .chain(once(0))
        .collect();
    let moved = unsafe {
        MoveFileExW(
            source_wide.as_ptr(),
            destination_wide.as_ptr(),
            MOVEFILE_REPLACE_EXISTING,
        )
    };
    if moved == 0 {
        return Err(format!(
            "Unable to atomically replace {}",
            destination.display()
        ));
    }
    Ok(())
}

#[cfg(not(target_os = "windows"))]
fn replace_file_atomically(source: &Path, destination: &Path) -> Result<(), String> {
    fs::rename(source, destination).map_err(|error| error.to_string())
}

fn write_config(app: &tauri::AppHandle, config: &RuntimeConfig) -> Result<(), String> {
    let path = config_path(app)?;
    let parent = path.parent().ok_or("Invalid cia app config path")?;
    fs::create_dir_all(parent)
        .map_err(|error| format!("Unable to create {}: {error}", parent.display()))?;

    let contents = serde_json::to_string_pretty(config)
        .map_err(|error| format!("Unable to serialize config.json: {error}"))?;
    let temporary = parent.join(format!(".config-{}.tmp", std::process::id()));
    fs::write(&temporary, contents)
        .map_err(|error| format!("Unable to write temporary config: {error}"))?;
    if let Err(error) = replace_file_atomically(&temporary, &path) {
        let _ = fs::remove_file(&temporary);
        return Err(error);
    }
    Ok(())
}

fn existing_file(value: &Option<String>) -> Option<PathBuf> {
    value
        .as_ref()
        .map(PathBuf::from)
        .filter(|path| path.is_file())
}

fn existing_directory(value: &Option<String>) -> Option<PathBuf> {
    value
        .as_ref()
        .map(PathBuf::from)
        .filter(|path| path.is_dir())
}

fn path_text(path: Option<PathBuf>) -> Option<String> {
    path.map(|value| value.to_string_lossy().to_string())
}

fn bundled_resource(app: &tauri::AppHandle, relative: &str) -> Option<PathBuf> {
    let resource_dir = app.path().resource_dir().ok()?;
    [
        resource_dir.join("resources").join(relative),
        resource_dir.join(relative),
    ]
    .into_iter()
    .find(|path| path.exists())
}

fn bundled_rife_script(app: &tauri::AppHandle) -> Option<PathBuf> {
    bundled_resource(app, "time_remap.py").filter(|path| path.is_file())
}

fn bundled_media_tools(app: &tauri::AppHandle) -> Option<MediaToolPaths> {
    let ffmpeg = bundled_resource(app, "runtime/ffmpeg/ffmpeg.exe")?;
    let ffprobe = bundled_resource(app, "runtime/ffmpeg/ffprobe.exe")?;
    (ffmpeg.is_file() && ffprobe.is_file()).then_some(MediaToolPaths { ffmpeg, ffprobe })
}

fn bundled_smoothie_runtime(
    app: &tauri::AppHandle,
    media: &MediaToolPaths,
) -> Option<SmoothieRuntimePaths> {
    let root = bundled_resource(app, "runtime/smoothie")?;
    let executable = root.join("bin").join("smoothie-rs.exe");
    let ffmpeg_directory = media.ffmpeg.parent()?.to_path_buf();
    (root.is_dir() && executable.is_file()).then_some(SmoothieRuntimePaths {
        root,
        executable,
        ffmpeg_directory,
    })
}

fn rife_install_root(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    app.path()
        .app_data_dir()
        .map(|directory| directory.join("runtimes").join("rife"))
        .map_err(|error| format!("Unable to resolve the cia app runtime directory: {error}"))
}

fn effective_rife_script(config: &RuntimeConfig, app: &tauri::AppHandle) -> Option<PathBuf> {
    existing_file(&config.rife.script).or_else(|| bundled_rife_script(app))
}

fn find_on_path(file_name: &str) -> Option<PathBuf> {
    let search_path = env::var_os("PATH")?;
    env::split_paths(&search_path)
        .map(|directory| directory.join(file_name))
        .find(|candidate| candidate.is_file())
}

fn auto_detect_config() -> RuntimeConfig {
    let mut config = RuntimeConfig::default();
    let home = env::var_os("USERPROFILE").map(PathBuf::from);

    if let Some(home) = home {
        let rife_base = home.join("time-remap-app");
        let python = rife_base.join("venv").join("Scripts").join("python.exe");
        let rife_directory = rife_base.join("Practical-RIFE");
        let model = rife_directory.join("train_log").join("flownet.pkl");
        if python.is_file() {
            config.rife.python_executable = path_text(Some(python));
        }
        if rife_directory.join("inference_video.py").is_file() {
            config.rife.directory = path_text(Some(rife_directory));
        }
        if model.is_file() {
            config.rife.model_file = path_text(Some(model));
        }

        let smoothie_root = home.join("Music").join("smoothie1");
        let smoothie_executable = smoothie_root.join("bin").join("smoothie-rs.exe");
        let recipe = smoothie_root.join("recipe.ini");
        if smoothie_root.is_dir() {
            config.smoothie.root = path_text(Some(smoothie_root));
        }
        if smoothie_executable.is_file() {
            config.smoothie.executable = path_text(Some(smoothie_executable));
        }
        if recipe.is_file() {
            config.smoothie.recipe = path_text(Some(recipe));
        }
    }

    config.media_tools.ffmpeg = path_text(find_on_path("ffmpeg.exe"));
    config.media_tools.ffprobe = path_text(find_on_path("ffprobe.exe"));
    config
}

fn normalize_config(mut config: RuntimeConfig) -> RuntimeConfig {
    config.schema_version = CONFIG_SCHEMA_VERSION;

    if config.rife.model_file.is_none() {
        if let Some(directory) = existing_directory(&config.rife.directory) {
            let model = directory.join("train_log").join("flownet.pkl");
            if model.is_file() {
                config.rife.model_file = path_text(Some(model));
            }
        }
    }

    if let Some(root) = existing_directory(&config.smoothie.root) {
        if config.smoothie.executable.is_none() {
            let executable = root.join("bin").join("smoothie-rs.exe");
            if executable.is_file() {
                config.smoothie.executable = path_text(Some(executable));
            }
        }
        if config.smoothie.recipe.is_none() {
            let recipe = root.join("recipe.ini");
            if recipe.is_file() {
                config.smoothie.recipe = path_text(Some(recipe));
            }
        }
    }
    config
}

fn component_status(
    id: &str,
    label: &str,
    path: Option<PathBuf>,
    expected: &str,
) -> ComponentStatus {
    let ready = path.is_some();
    ComponentStatus {
        id: id.to_string(),
        label: label.to_string(),
        path: path_text(path),
        ready,
        detail: if ready {
            "Configured and present".to_string()
        } else {
            "Missing or invalid path".to_string()
        },
        expected: expected.to_string(),
    }
}

fn snapshot_from_config(
    app: &tauri::AppHandle,
    config: RuntimeConfig,
    detected: RuntimeConfig,
    load_error: Option<String>,
) -> RuntimeSnapshot {
    let python = existing_file(&config.rife.python_executable);
    let script = effective_rife_script(&config, app);
    let rife_directory = existing_directory(&config.rife.directory)
        .filter(|directory| directory.join("inference_video.py").is_file());
    let model = existing_file(&config.rife.model_file);
    let bundled_media = bundled_media_tools(app);
    let ffmpeg = existing_file(&config.media_tools.ffmpeg)
        .or_else(|| bundled_media.as_ref().map(|media| media.ffmpeg.clone()));
    let ffprobe = existing_file(&config.media_tools.ffprobe)
        .or_else(|| bundled_media.as_ref().map(|media| media.ffprobe.clone()));
    let bundled_smoothie = bundled_media
        .as_ref()
        .and_then(|media| bundled_smoothie_runtime(app, media));
    let smoothie_root = existing_directory(&config.smoothie.root)
        .or_else(|| bundled_smoothie.as_ref().map(|runtime| runtime.root.clone()));
    let smoothie_executable = existing_file(&config.smoothie.executable)
        .or_else(|| bundled_smoothie.as_ref().map(|runtime| runtime.executable.clone()));
    let smoothie_recipe = existing_file(&config.smoothie.recipe).or_else(|| {
        bundled_smoothie
            .as_ref()
            .map(|runtime| runtime.root.join("recipe.ini"))
            .filter(|recipe| recipe.is_file())
    });

    let components = vec![
        component_status(
            "rife_python",
            "Python runtime",
            python.clone(),
            "Python 3.11+ executable",
        ),
        component_status(
            "rife_script",
            "cia app RIFE script",
            script.clone(),
            "Bundled script or explicit time_remap.py",
        ),
        component_status(
            "rife_directory",
            "Practical-RIFE",
            rife_directory.clone(),
            "Folder containing inference_video.py",
        ),
        component_status("rife_model", "RIFE model", model.clone(), "flownet.pkl"),
        component_status(
            "ffmpeg",
            "FFmpeg",
            ffmpeg.clone(),
            "Explicit ffmpeg executable",
        ),
        component_status(
            "ffprobe",
            "FFprobe",
            ffprobe.clone(),
            "Explicit ffprobe executable",
        ),
        component_status(
            "smoothie_root",
            "Smoothie root",
            smoothie_root.clone(),
            "smoothie-rs runtime folder",
        ),
        component_status(
            "smoothie_executable",
            "smoothie-rs",
            smoothie_executable.clone(),
            "smoothie-rs executable",
        ),
        component_status(
            "smoothie_recipe",
            "Smoothie recipe",
            smoothie_recipe,
            "recipe.ini",
        ),
    ];

    RuntimeSnapshot {
        config,
        detected,
        rife_ready: python.is_some()
            && script.is_some()
            && rife_directory.is_some()
            && model.is_some()
            && ffmpeg.is_some()
            && ffprobe.is_some(),
        smoothie_ready: smoothie_root.is_some() && smoothie_executable.is_some(),
        media_tools_ready: ffmpeg.is_some() && ffprobe.is_some(),
        components,
        load_error,
    }
}

fn runtime_snapshot(app: &tauri::AppHandle) -> RuntimeSnapshot {
    let detected = auto_detect_config();
    match load_config(app) {
        Ok(config) => snapshot_from_config(app, normalize_config(config), detected, None),
        Err(error) => snapshot_from_config(app, RuntimeConfig::default(), detected, Some(error)),
    }
}

fn required_file(value: &Option<String>, label: &str) -> Result<PathBuf, String> {
    existing_file(value).ok_or_else(|| format!("{label} is not configured. Open Runtime Setup."))
}

fn required_directory(value: &Option<String>, label: &str) -> Result<PathBuf, String> {
    existing_directory(value)
        .ok_or_else(|| format!("{label} is not configured. Open Runtime Setup."))
}

fn media_tools(config: &RuntimeConfig, app: &tauri::AppHandle) -> Result<MediaToolPaths, String> {
    match (
        existing_file(&config.media_tools.ffmpeg),
        existing_file(&config.media_tools.ffprobe),
    ) {
        (Some(ffmpeg), Some(ffprobe)) => Ok(MediaToolPaths { ffmpeg, ffprobe }),
        _ => bundled_media_tools(app).ok_or_else(|| {
            "Bundled FFmpeg tools are unavailable. Reinstall cia app or configure Runtime paths."
                .to_string()
        }),
    }
}

fn rife_runtime(
    config: &RuntimeConfig,
    app: &tauri::AppHandle,
) -> Result<RifeRuntimePaths, String> {
    let directory = required_directory(&config.rife.directory, "Practical-RIFE")?;
    if !directory.join("inference_video.py").is_file() {
        return Err("Practical-RIFE does not contain inference_video.py".to_string());
    }
    let model = required_file(&config.rife.model_file, "RIFE model")?;
    if model.file_name().and_then(|name| name.to_str()) != Some("flownet.pkl") {
        return Err("RIFE model must point to flownet.pkl".to_string());
    }
    let script = effective_rife_script(config, app)
        .ok_or("cia app RIFE script is unavailable. Reinstall the application or configure the script path.")?;

    Ok(RifeRuntimePaths {
        python: required_file(&config.rife.python_executable, "Python runtime")?,
        script,
        directory,
        media: media_tools(config, app)?,
    })
}

fn smoothie_runtime(
    config: &RuntimeConfig,
    app: &tauri::AppHandle,
) -> Result<SmoothieRuntimePaths, String> {
    let media = media_tools(config, app)?;
    match (
        existing_directory(&config.smoothie.root),
        existing_file(&config.smoothie.executable),
    ) {
        (Some(root), Some(executable)) => Ok(SmoothieRuntimePaths {
            root,
            executable,
            ffmpeg_directory: media
                .ffmpeg
                .parent()
                .ok_or("Invalid FFmpeg path")?
                .to_path_buf(),
        }),
        _ => bundled_smoothie_runtime(app, &media).ok_or_else(|| {
            "Bundled Smoothie is unavailable. Reinstall cia app or configure Runtime paths."
                .to_string()
        }),
    }
}

async fn pump<R>(reader: R, app: tauri::AppHandle)
where
    R: AsyncRead + Unpin,
{
    let mut reader = reader;
    let mut buf = [0u8; 4096];
    let mut pending = String::new();
    loop {
        match reader.read(&mut buf).await {
            Ok(0) => break,
            Ok(n) => {
                let chunk = String::from_utf8_lossy(&buf[..n]);
                pending.push_str(&chunk);
                let (consumed, lines) = {
                    let bytes = pending.as_bytes();
                    let mut lines = Vec::new();
                    let mut start = 0usize;
                    for (index, byte) in bytes.iter().enumerate() {
                        if *byte == b'\r' || *byte == b'\n' {
                            if let Ok(segment) = std::str::from_utf8(&bytes[start..index]) {
                                let trimmed = segment.trim();
                                if !trimmed.is_empty() {
                                    lines.push(trimmed.to_string());
                                }
                            }
                            start = index + 1;
                        }
                    }
                    (start, lines)
                };
                pending.drain(..consumed);
                for line in lines {
                    if let Some(progress) = line.strip_prefix("CIA_PROGRESS ") {
                        let mut step = 0u32;
                        let mut total = 0u32;
                        let mut label = String::new();
                        for part in progress.split_whitespace() {
                            if let Some(v) = part.strip_prefix("step=") {
                                step = v.parse().unwrap_or(0);
                            } else if let Some(v) = part.strip_prefix("total=") {
                                total = v.parse().unwrap_or(0);
                            } else if let Some(v) = part.strip_prefix("label=") {
                                label = v.replace('_', " ");
                            } else if !label.is_empty() {
                                label.push(' ');
                                label.push_str(part);
                            }
                        }
                        let _ = app.emit(
                            "install-progress",
                            json!({ "step": step, "total": total, "label": label }),
                        );
                    }
                    let _ = app.emit("live-log", &line);
                }
            }
            Err(_) => break,
        }
    }
    let trailing = pending.trim();
    if !trailing.is_empty() {
        let _ = app.emit("live-log", trailing);
    }
}

async fn pump_and_collect<R>(reader: R, app: tauri::AppHandle) -> Vec<String>
where
    R: AsyncRead + Unpin,
{
    let mut reader = reader;
    let mut buf = [0u8; 4096];
    let mut pending = String::new();
    let mut collected = Vec::new();
    loop {
        match reader.read(&mut buf).await {
            Ok(0) => break,
            Ok(n) => {
                let chunk = String::from_utf8_lossy(&buf[..n]);
                pending.push_str(&chunk);
                let (consumed, lines) = {
                    let bytes = pending.as_bytes();
                    let mut lines = Vec::new();
                    let mut start = 0usize;
                    for (index, byte) in bytes.iter().enumerate() {
                        if *byte == b'\r' || *byte == b'\n' {
                            if let Ok(segment) = std::str::from_utf8(&bytes[start..index]) {
                                let trimmed = segment.trim();
                                if !trimmed.is_empty() {
                                    lines.push(trimmed.to_string());
                                }
                            }
                            start = index + 1;
                        }
                    }
                    (start, lines)
                };
                pending.drain(..consumed);
                for line in &lines {
                    let _ = app.emit("live-log", line);
                }
                collected.extend(lines);
            }
            Err(_) => break,
        }
    }
    let trailing = pending.trim();
    if !trailing.is_empty() {
        let _ = app.emit("live-log", trailing);
        collected.push(trailing.to_string());
    }
    collected
}

async fn probe_video(video_path: &str, ffprobe: &Path) -> Result<VideoInfo, String> {
    let mut command = Command::new(ffprobe);
    command
        .args([
            "-v",
            "error",
            "-select_streams",
            "v:0",
            "-show_entries",
            "stream=width,height,r_frame_rate,duration",
            "-show_entries",
            "format=duration",
            "-of",
            "json",
        ])
        .arg(video_path);

    #[cfg(target_os = "windows")]
    command.creation_flags(CREATE_NO_WINDOW);

    let output = command
        .output()
        .await
        .map_err(|error| format!("FFprobe could not start: {error}"))?;
    if !output.status.success() {
        return Err(format!(
            "FFprobe error: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }

    let raw = String::from_utf8_lossy(&output.stdout);
    let json: Value = serde_json::from_str(&raw)
        .map_err(|error| format!("FFprobe returned invalid JSON: {error}"))?;
    let stream = &json["streams"][0];
    let width = stream["width"].as_u64().unwrap_or(0) as u32;
    let height = stream["height"].as_u64().unwrap_or(0) as u32;
    let fps_string = stream["r_frame_rate"].as_str().unwrap_or("30/1");
    let fps = if let Some((numerator, denominator)) = fps_string.split_once('/') {
        let numerator: f64 = numerator.parse().unwrap_or(30.0);
        let denominator: f64 = denominator.parse().unwrap_or(1.0);
        if denominator > 0.0 {
            numerator / denominator
        } else {
            30.0
        }
    } else {
        30.0
    };
    let duration = json["format"]["duration"]
        .as_str()
        .and_then(|value| value.parse().ok())
        .or_else(|| {
            stream["duration"]
                .as_str()
                .and_then(|value| value.parse().ok())
        })
        .unwrap_or(0.0);

    let mut audio_command = Command::new(ffprobe);
    audio_command
        .args([
            "-v",
            "error",
            "-select_streams",
            "a:0",
            "-show_entries",
            "stream=codec_type",
            "-of",
            "csv=p=0",
        ])
        .arg(video_path);
    #[cfg(target_os = "windows")]
    audio_command.creation_flags(CREATE_NO_WINDOW);
    let has_audio = audio_command
        .output()
        .await
        .map(|output| !String::from_utf8_lossy(&output.stdout).trim().is_empty())
        .unwrap_or(false);

    Ok(VideoInfo {
        width,
        height,
        fps,
        duration,
        has_audio,
    })
}

fn rife_output_path(
    video_path: &str,
    mode: &str,
    factor: f64,
    input_fps: f64,
) -> Result<PathBuf, String> {
    if !factor.is_finite() || factor <= 0.0 {
        return Err("Interpolation factor must be greater than zero".to_string());
    }
    let input = Path::new(video_path);
    let parent = input.parent().unwrap_or_else(|| Path::new(""));
    let stem = input
        .file_stem()
        .and_then(|value| value.to_str())
        .filter(|value| !value.is_empty())
        .ok_or("Unable to derive the interpolation output filename")?;
    let output_fps = match mode {
        "boost" => (input_fps * factor).round(),
        "slowmo" => input_fps.round(),
        _ => return Err(format!("Unsupported interpolation mode: {mode}")),
    };
    if output_fps <= 0.0 {
        return Err("Unable to derive a valid output framerate".to_string());
    }
    Ok(parent.join(format!("{stem}-{}fps.mp4", output_fps as u64)))
}

fn smoothie_output_path(video_path: &str, output_fps: u32) -> Result<PathBuf, String> {
    if output_fps == 0 {
        return Err("Smoothie output framerate must be greater than zero".to_string());
    }
    let input = Path::new(video_path);
    let parent = input.parent().unwrap_or_else(|| Path::new(""));
    let stem = input
        .file_stem()
        .and_then(|value| value.to_str())
        .filter(|value| !value.is_empty())
        .ok_or("Unable to derive the Smoothie output filename")?;
    Ok(parent.join(format!("{stem}_render{output_fps}fps.mp4")))
}

fn ensure_nonempty_file(path: &Path, label: &str) -> Result<(), String> {
    let metadata = fs::metadata(path)
        .map_err(|error| format!("{label} output is missing: {} ({error})", path.display()))?;
    if !metadata.is_file() || metadata.len() == 0 {
        return Err(format!("{label} output is invalid: {}", path.display()));
    }
    Ok(())
}

fn reserve_output_path(preferred: &Path) -> Result<OutputReservation, String> {
    let parent = preferred.parent().unwrap_or_else(|| Path::new(""));
    let stem = preferred
        .file_stem()
        .and_then(|value| value.to_str())
        .filter(|value| !value.is_empty())
        .ok_or("Unable to reserve the output filename")?;
    let extension = preferred.extension().and_then(|value| value.to_str());

    for index in 0..10_000u32 {
        let filename = match (index, extension) {
            (0, Some(extension)) => format!("{stem}.{extension}"),
            (0, None) => stem.to_string(),
            (_, Some(extension)) => format!("{stem} ({index}).{extension}"),
            (_, None) => format!("{stem} ({index})"),
        };
        let output = parent.join(filename);
        let lock_name = format!(
            ".{}.cia-render.lock",
            output
                .file_name()
                .and_then(|value| value.to_str())
                .ok_or("Unable to reserve the output filename")?
        );
        let lock = parent.join(lock_name);

        match OpenOptions::new().write(true).create_new(true).open(&lock) {
            Ok(_) => {
                if output.exists() {
                    let _ = fs::remove_file(&lock);
                    continue;
                }
                return Ok(OutputReservation { output, lock });
            }
            Err(error) if error.kind() == ErrorKind::AlreadyExists => continue,
            Err(error) => {
                return Err(format!(
                    "Unable to reserve an output name in {}: {error}",
                    parent.display()
                ))
            }
        }
    }

    Err("Unable to find a free output name after 9,999 existing files".to_string())
}

#[tauri::command]
fn get_runtime_snapshot(app: tauri::AppHandle) -> RuntimeSnapshot {
    runtime_snapshot(&app)
}

#[tauri::command]
fn get_app_version(app: tauri::AppHandle) -> String {
    app.package_info().version.to_string()
}

#[tauri::command]
fn save_runtime_config(
    app: tauri::AppHandle,
    config: RuntimeConfig,
) -> Result<RuntimeSnapshot, String> {
    let config = normalize_config(config);
    write_config(&app, &config)?;
    Ok(runtime_snapshot(&app))
}

#[tauri::command]
fn save_ui_preferences(
    app: tauri::AppHandle,
    auto_render: bool,
    rife_settings: Value,
    smoothie_settings: Value,
) -> Result<RuntimeSnapshot, String> {
    let mut config = load_config(&app).unwrap_or_default();
    config.ui = UiSettings {
        migrated: true,
        auto_render,
        rife_settings,
        smoothie_settings,
    };
    write_config(&app, &normalize_config(config))?;
    Ok(runtime_snapshot(&app))
}

#[tauri::command]
async fn pick_runtime_path(kind: String) -> Result<Option<String>, String> {
    let selected = tokio::task::spawn_blocking(move || match kind.as_str() {
        "rife_directory" | "smoothie_root" => rfd::FileDialog::new().pick_folder(),
        "rife_python"
        | "rife_script"
        | "rife_model"
        | "smoothie_executable"
        | "smoothie_recipe"
        | "ffmpeg"
        | "ffprobe" => rfd::FileDialog::new().pick_file(),
        _ => None,
    })
    .await
    .map_err(|error| error.to_string())?;
    Ok(selected.map(|path| path.to_string_lossy().to_string()))
}

#[tauri::command]
async fn analyze_video(app: tauri::AppHandle, video_path: String) -> Result<VideoInfo, String> {
    let config = load_config(&app)?;
    let media = media_tools(&config, &app)?;
    probe_video(&video_path, &media.ffprobe).await
}

#[tauri::command]
async fn install_rife_environment(app: tauri::AppHandle) -> Result<RuntimeSnapshot, String> {
    let configured = normalize_config(load_config(&app).unwrap_or_default());
    if rife_runtime(&configured, &app).is_ok() {
        let _ = app.emit(
            "live-log",
            "[cia app] A configured RIFE environment is already ready.",
        );
        return Ok(runtime_snapshot(&app));
    }

    let detected = auto_detect_config();
    let mut adopted = configured.clone();
    adopted.rife = detected.rife;
    let adopted = normalize_config(adopted);
    if rife_runtime(&adopted, &app).is_ok() {
        write_config(&app, &adopted)?;
        let _ = app.emit(
            "live-log",
            "[cia app] A complete local RIFE runtime was detected and is now in use.",
        );
        return Ok(runtime_snapshot(&app));
    }

    let _ = app.emit(
        "live-log",
        "[cia app] No complete local RIFE runtime was found. Installing the optional environment...",
    );
    let bootstrap = bundled_resource(&app, "bootstrap/bootstrap-rife.ps1")
        .filter(|path| path.is_file())
        .ok_or("The bundled RIFE installer script is missing. Reinstall cia app.")?;
    let python_installer = bundled_resource(&app, "bootstrap/python-3.11.9-amd64.exe")
        .filter(|path| path.is_file())
        .ok_or("The bundled Python installer is missing. Reinstall cia app.")?;
    let root = rife_install_root(&app)?;
    fs::create_dir_all(&root)
        .map_err(|error| format!("Unable to create {}: {error}", root.display()))?;

    let mut command = Command::new("powershell.exe");
    command
        .args([
            "-NoProfile",
            "-ExecutionPolicy",
            "Bypass",
            "-File",
        ])
        .arg(&bootstrap)
        .arg("-RuntimeRoot")
        .arg(&root)
        .arg("-PythonInstaller")
        .arg(&python_installer)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    #[cfg(target_os = "windows")]
    command.creation_flags(CREATE_NO_WINDOW);

    let mut child = command
        .spawn()
        .map_err(|error| format!("Failed to start the RIFE environment installer: {error}"))?;
    let stdout = child
        .stdout
        .take()
        .ok_or("RIFE installer stdout was unavailable")?;
    let stderr = child
        .stderr
        .take()
        .ok_or("RIFE installer stderr was unavailable")?;
    let output_app = app.clone();
    let error_app = app.clone();
    let output_task = tokio::spawn(async move { pump(stdout, output_app).await });
    let error_task = tokio::spawn(async move { pump_and_collect(stderr, error_app).await });
    let status = child.wait().await.map_err(|error| error.to_string())?;
    let _ = output_task.await;
    let stderr_lines = error_task.await.unwrap_or_default();
    if !status.success() {
        let detail = stderr_lines
            .iter()
            .rev()
            .find(|line| line.contains("ERROR:"))
            .or_else(|| stderr_lines.last())
            .cloned()
            .unwrap_or_default();
        return if detail.is_empty() {
            Err(format!(
                "RIFE environment installation failed ({status}). Review COPY LOGS for the exact step."
            ))
        } else {
            Err(format!("RIFE environment installation failed: {detail}"))
        };
    }

    let mut config = load_config(&app).unwrap_or_default();
    config.rife = RifeConfig {
        python_executable: path_text(Some(root.join("venv").join("Scripts").join("python.exe"))),
        script: None,
        directory: path_text(Some(root.join("Practical-RIFE"))),
        model_file: path_text(Some(
            root.join("Practical-RIFE").join("train_log").join("flownet.pkl"),
        )),
    };
    let config = normalize_config(config);
    rife_runtime(&config, &app)?;
    write_config(&app, &config)?;
    Ok(runtime_snapshot(&app))
}

#[tauri::command]
fn pause_render(job_id: String, registry: tauri::State<JobRegistry>) -> Result<(), String> {
    let job = running_job(&registry, &job_id)?;
    if job.paused {
        return Ok(());
    }
    #[cfg(target_os = "windows")]
    process_control::set_process_tree_paused(job.process_id, true)?;
    #[cfg(not(target_os = "windows"))]
    return Err("Pause is currently supported on Windows only".to_string());

    let mut jobs = registry.jobs.lock().map_err(|_| "Render job registry is unavailable")?;
    if let Some(entry) = jobs.get_mut(&job_id) {
        entry.paused = true;
    }
    Ok(())
}

#[tauri::command]
fn resume_render(job_id: String, registry: tauri::State<JobRegistry>) -> Result<(), String> {
    let job = running_job(&registry, &job_id)?;
    if !job.paused {
        return Ok(());
    }
    #[cfg(target_os = "windows")]
    process_control::set_process_tree_paused(job.process_id, false)?;
    #[cfg(not(target_os = "windows"))]
    return Err("Resume is currently supported on Windows only".to_string());

    let mut jobs = registry.jobs.lock().map_err(|_| "Render job registry is unavailable")?;
    if let Some(entry) = jobs.get_mut(&job_id) {
        entry.paused = false;
    }
    Ok(())
}

#[tauri::command]
fn cancel_render(job_id: String, registry: tauri::State<JobRegistry>) -> Result<(), String> {
    let job = running_job(&registry, &job_id)?;
    {
        let mut jobs = registry.jobs.lock().map_err(|_| "Render job registry is unavailable")?;
        if let Some(entry) = jobs.get_mut(&job_id) {
            entry.cancel_requested = true;
        }
    }
    #[cfg(target_os = "windows")]
    return cancel_process_tree(job.process_id);
    #[cfg(not(target_os = "windows"))]
    Err("Cancel is currently supported on Windows only".to_string())
}

#[tauri::command]
async fn run_time_remap(
    app: tauri::AppHandle,
    registry: tauri::State<'_, JobRegistry>,
    job_id: String,
    video_path: String,
    mode: String,
    factor: f64,
    crf: u32,
    preset: String,
    scene_threshold: f64,
    blend_cuts: u32,
) -> Result<String, String> {
    let config = load_config(&app)?;
    let runtime = rife_runtime(&config, &app)?;
    let info = probe_video(&video_path, &runtime.media.ffprobe).await?;
    let reservation = reserve_output_path(&rife_output_path(&video_path, &mode, factor, info.fps)?)?;
    let out_path = reservation.output.clone();

    let mut command = Command::new(&runtime.python);
    command
        .arg(&runtime.script)
        .arg("--video")
        .arg(&video_path)
        .arg("--mode")
        .arg(&mode)
        .arg("--factor")
        .arg(factor.to_string())
        .arg("--crf")
        .arg(crf.to_string())
        .arg("--preset")
        .arg(&preset)
        .arg("--scene_threshold")
        .arg(scene_threshold.to_string())
        .arg("--blend-cuts")
        .arg(blend_cuts.to_string())
        .arg("--output")
        .arg(&out_path)
        .arg("--ffmpeg")
        .arg(&runtime.media.ffmpeg)
        .arg("--ffprobe")
        .arg(&runtime.media.ffprobe)
        .arg("--rife-dir")
        .arg(&runtime.directory)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    #[cfg(target_os = "windows")]
    command.creation_flags(CREATE_NO_WINDOW);

    let mut child = command
        .spawn()
        .map_err(|error| format!("Failed to start Python runtime: {error}"))?;
    let process_id = child
        .id()
        .ok_or("The RIFE process did not expose a process identifier")?;
    register_job(&registry, &job_id, process_id)?;
    let stdout = child.stdout.take().ok_or("Python stdout was unavailable")?;
    let stderr = child.stderr.take().ok_or("Python stderr was unavailable")?;
    let output_app = app.clone();
    let error_app = app.clone();
    let output_task = tokio::spawn(async move { pump(stdout, output_app).await });
    let error_task = tokio::spawn(async move { pump(stderr, error_app).await });
    let status = child.wait().await.map_err(|error| error.to_string())?;
    let _ = output_task.await;
    let _ = error_task.await;
    let cancelled = finish_job(&registry, &job_id)?
        .map(|job| job.cancel_requested)
        .unwrap_or(false);

    if cancelled {
        if out_path.exists() {
            let _ = fs::remove_file(&out_path);
        }
        return Err("CIA_RENDER_CANCELLED".to_string());
    }

    if status.success() {
        ensure_nonempty_file(&out_path, "RIFE")?;
        Ok(out_path.to_string_lossy().to_string())
    } else {
        Err(format!("RIFE process failed ({status})"))
    }
}

#[tauri::command]
async fn run_smoothie(
    app: tauri::AppHandle,
    registry: tauri::State<'_, JobRegistry>,
    job_id: String,
    video_path: String,
    output_fps: u32,
    overrides: Vec<String>,
) -> Result<String, String> {
    let config = load_config(&app)?;
    let runtime = smoothie_runtime(&config, &app)?;
    let reservation = reserve_output_path(&smoothie_output_path(&video_path, output_fps)?)?;
    let out_path = reservation.output.clone();
    let out_path_text = out_path.to_string_lossy().to_string();

    let mut command = Command::new(&runtime.executable);
    let inherited_path = env::var_os("PATH").unwrap_or_default();
    let scoped_path = env::join_paths(
        std::iter::once(runtime.ffmpeg_directory.clone()).chain(env::split_paths(&inherited_path)),
    )
    .map_err(|error| format!("Unable to prepare bundled media-tool path: {error}"))?;
    command
        .current_dir(&runtime.root)
        .env("PATH", scoped_path)
        .arg("-i")
        .arg(&video_path)
        .arg("-o")
        .arg(&out_path_text)
        .arg("--progress");
    if !overrides.is_empty() {
        command.arg("--override");
        for override_value in &overrides {
            command.arg(override_value);
        }
    }
    command.stdout(Stdio::piped()).stderr(Stdio::piped());
    #[cfg(target_os = "windows")]
    command.creation_flags(CREATE_NO_WINDOW);

    let mut child = command
        .spawn()
        .map_err(|error| format!("Failed to start smoothie-rs: {error}"))?;
    let process_id = child
        .id()
        .ok_or("The Smoothie process did not expose a process identifier")?;
    register_job(&registry, &job_id, process_id)?;
    let stdout = child
        .stdout
        .take()
        .ok_or("Smoothie stdout was unavailable")?;
    let stderr = child
        .stderr
        .take()
        .ok_or("Smoothie stderr was unavailable")?;
    let output_app = app.clone();
    let error_app = app.clone();
    let output_task = tokio::spawn(async move { pump(stdout, output_app).await });
    let error_task = tokio::spawn(async move { pump(stderr, error_app).await });
    let status = child.wait().await.map_err(|error| error.to_string())?;
    let _ = output_task.await;
    let _ = error_task.await;
    let cancelled = finish_job(&registry, &job_id)?
        .map(|job| job.cancel_requested)
        .unwrap_or(false);

    if cancelled {
        if out_path.exists() {
            let _ = fs::remove_file(&out_path);
        }
        return Err("CIA_RENDER_CANCELLED".to_string());
    }

    if status.success() {
        ensure_nonempty_file(&out_path, "Smoothie")?;
        Ok(out_path_text)
    } else {
        Err(format!("smoothie-rs process failed ({status})"))
    }
}

#[tauri::command]
async fn open_file_dialog() -> Result<Option<String>, String> {
    let file = tokio::task::spawn_blocking(|| {
        rfd::FileDialog::new()
            .add_filter("Video", &["mp4", "mkv", "mov", "avi", "webm"])
            .pick_file()
    })
    .await
    .map_err(|error| error.to_string())?;
    Ok(file.map(|path| path.to_string_lossy().to_string()))
}

#[tauri::command]
fn open_target_file(path: String) -> Result<(), String> {
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        std::process::Command::new("cmd")
            .args(["/C", "start", "", &path])
            .creation_flags(CREATE_NO_WINDOW)
            .spawn()
            .map_err(|error| format!("Failed to open file: {error}"))?;
    }
    Ok(())
}

#[tauri::command]
fn open_target_folder(path: String) -> Result<(), String> {
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        std::process::Command::new("explorer")
            .arg(format!("/select,{}", path))
            .creation_flags(CREATE_NO_WINDOW)
            .spawn()
            .map_err(|error| format!("Failed to reveal file: {error}"))?;
    }
    Ok(())
}

#[tauri::command]
fn open_about_link(url: String) -> Result<(), String> {
    if !ABOUT_URLS.contains(&url.as_str()) {
        return Err("This About link is not supported".to_string());
    }
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        std::process::Command::new("cmd")
            .args(["/C", "start", "", &url])
            .creation_flags(CREATE_NO_WINDOW)
            .spawn()
            .map_err(|error| format!("Failed to open browser: {error}"))?;
    }
    Ok(())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let mut builder = tauri::Builder::default();

    #[cfg(desktop)]
    {
        builder = builder
            .plugin(tauri_plugin_updater::Builder::new().build())
            .plugin(tauri_plugin_process::init());
    }

    builder
        .setup(|app| {
            #[cfg(desktop)]
            {
                if let (Some(window), Some(icon)) = (app.get_webview_window("main"), app.default_window_icon()) {
                    let _ = window.set_icon(icon.clone());
                }
            }
            Ok(())
        })
        .manage(JobRegistry::default())
        .invoke_handler(tauri::generate_handler![
            get_runtime_snapshot,
            get_app_version,
            save_runtime_config,
            save_ui_preferences,
            pick_runtime_path,
            analyze_video,
            install_rife_environment,
            pause_render,
            resume_render,
            cancel_render,
            run_time_remap,
            run_smoothie,
            open_file_dialog,
            open_target_file,
            open_target_folder,
            open_about_link
        ])
        .run(tauri::generate_context!())
        .expect("error while running cia app");
}

#[cfg(test)]
mod tests {
    use super::{reserve_output_path, rife_output_path, smoothie_output_path};

    #[test]
    fn interpolation_name_uses_only_the_actual_output_fps() {
        let output = rife_output_path(r"C:\media\clip.mp4", "boost", 12.0, 30.0).unwrap();
        assert_eq!(
            output,
            std::path::PathBuf::from(r"C:\media\clip-360fps.mp4")
        );
    }

    #[test]
    fn smoothie_name_uses_its_input_stem_and_selected_fps() {
        let output = smoothie_output_path(r"C:\media\clip-360fps.mp4", 30).unwrap();
        assert_eq!(
            output,
            std::path::PathBuf::from(r"C:\media\clip-360fps_render30fps.mp4")
        );
    }

    #[test]
    fn an_existing_output_gets_a_numbered_name_without_leaving_a_lock() {
        let directory = std::env::temp_dir().join(format!(
            "cia-render-output-reservation-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&directory).unwrap();
        let preferred = directory.join("cutshirlery_render30fps.mp4");
        let first_numbered = directory.join("cutshirlery_render30fps (1).mp4");
        std::fs::write(&preferred, b"already rendered").unwrap();
        std::fs::write(&first_numbered, b"already rendered again").unwrap();

        let reservation = reserve_output_path(&preferred).unwrap();
        assert_eq!(
            reservation.output,
            directory.join("cutshirlery_render30fps (2).mp4")
        );
        assert!(reservation.lock.is_file());
        let lock = reservation.lock.clone();
        drop(reservation);
        assert!(!lock.exists());
        std::fs::remove_dir_all(directory).unwrap();
    }

    #[cfg(target_os = "windows")]
    fn sleeping_process(seconds: u32) -> std::process::Child {
        use std::os::windows::process::CommandExt;

        std::process::Command::new("powershell.exe")
            .args([
                "-NoProfile",
                "-Command",
                &format!("Start-Sleep -Seconds {seconds}"),
            ])
            .creation_flags(super::CREATE_NO_WINDOW)
            .spawn()
            .expect("start controlled test process")
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn pause_and_resume_controls_a_live_process() {
        let mut child = sleeping_process(2);
        std::thread::sleep(std::time::Duration::from_millis(120));
        super::process_control::set_process_tree_paused(child.id(), true)
            .expect("pause live process tree");
        std::thread::sleep(std::time::Duration::from_millis(2200));
        assert!(
            child.try_wait().expect("inspect paused process").is_none(),
            "the paused process should not complete while suspended"
        );
        super::process_control::set_process_tree_paused(child.id(), false)
            .expect("resume live process tree");
        assert!(child.wait().expect("wait resumed process").success());
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn cancel_terminates_a_live_process_tree() {
        let mut child = sleeping_process(10);
        std::thread::sleep(std::time::Duration::from_millis(120));
        super::cancel_process_tree(child.id()).expect("cancel live process tree");
        assert!(
            !child.wait().expect("wait cancelled process").success(),
            "a cancelled process must not report success"
        );
    }
}
