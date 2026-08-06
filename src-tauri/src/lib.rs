use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use tauri::{Emitter, Manager};
use tokio::io::{AsyncRead, AsyncReadExt};
use tokio::process::Command;

#[cfg(target_os = "windows")]
const CREATE_NO_WINDOW: u32 = 0x08000000;

const CONFIG_SCHEMA_VERSION: u32 = 1;
const ABOUT_URLS: [&str; 8] = [
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
}

fn config_path(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    app.path()
        .app_config_dir()
        .map(|dir| dir.join("config.json"))
        .map_err(|error| format!("Unable to resolve the CIA RENDER config directory: {error}"))
}

fn load_config(app: &tauri::AppHandle) -> Result<RuntimeConfig, String> {
    let path = config_path(app)?;
    if !path.exists() {
        return Ok(RuntimeConfig::default());
    }

    let raw = fs::read_to_string(&path)
        .map_err(|error| format!("Unable to read {}: {error}", path.display()))?;
    let config: RuntimeConfig = serde_json::from_str(&raw)
        .map_err(|error| format!("Invalid config.json: {error}"))?;
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
    let destination_wide: Vec<u16> = destination.as_os_str().encode_wide().chain(once(0)).collect();
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
    let parent = path.parent().ok_or("Invalid CIA RENDER config path")?;
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

fn bundled_rife_script(app: &tauri::AppHandle) -> Option<PathBuf> {
    let resource_dir = app.path().resource_dir().ok()?;
    [
        resource_dir.join("resources").join("time_remap.py"),
        resource_dir.join("time_remap.py"),
    ]
    .into_iter()
    .find(|path| path.is_file())
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
    let ffmpeg = existing_file(&config.media_tools.ffmpeg);
    let ffprobe = existing_file(&config.media_tools.ffprobe);
    let smoothie_root = existing_directory(&config.smoothie.root);
    let smoothie_executable = existing_file(&config.smoothie.executable);
    let smoothie_recipe = existing_file(&config.smoothie.recipe);

    let components = vec![
        component_status("rife_python", "Python runtime", python.clone(), "Python 3.11+ executable"),
        component_status("rife_script", "CIA RENDER RIFE script", script.clone(), "Bundled script or explicit time_remap.py"),
        component_status("rife_directory", "Practical-RIFE", rife_directory.clone(), "Folder containing inference_video.py"),
        component_status("rife_model", "RIFE model", model.clone(), "flownet.pkl"),
        component_status("ffmpeg", "FFmpeg", ffmpeg.clone(), "Explicit ffmpeg executable"),
        component_status("ffprobe", "FFprobe", ffprobe.clone(), "Explicit ffprobe executable"),
        component_status("smoothie_root", "Smoothie root", smoothie_root.clone(), "smoothie-rs runtime folder"),
        component_status("smoothie_executable", "smoothie-rs", smoothie_executable.clone(), "smoothie-rs executable"),
        component_status("smoothie_recipe", "Smoothie recipe", smoothie_recipe, "recipe.ini"),
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
    existing_directory(value).ok_or_else(|| format!("{label} is not configured. Open Runtime Setup."))
}

fn media_tools(config: &RuntimeConfig) -> Result<MediaToolPaths, String> {
    Ok(MediaToolPaths {
        ffmpeg: required_file(&config.media_tools.ffmpeg, "FFmpeg")?,
        ffprobe: required_file(&config.media_tools.ffprobe, "FFprobe")?,
    })
}

fn rife_runtime(config: &RuntimeConfig, app: &tauri::AppHandle) -> Result<RifeRuntimePaths, String> {
    let directory = required_directory(&config.rife.directory, "Practical-RIFE")?;
    if !directory.join("inference_video.py").is_file() {
        return Err("Practical-RIFE does not contain inference_video.py".to_string());
    }
    let model = required_file(&config.rife.model_file, "RIFE model")?;
    if model.file_name().and_then(|name| name.to_str()) != Some("flownet.pkl") {
        return Err("RIFE model must point to flownet.pkl".to_string());
    }
    let script = effective_rife_script(config, app)
        .ok_or("CIA RENDER RIFE script is unavailable. Reinstall the application or configure the script path.")?;

    Ok(RifeRuntimePaths {
        python: required_file(&config.rife.python_executable, "Python runtime")?,
        script,
        directory,
        media: media_tools(config)?,
    })
}

fn smoothie_runtime(config: &RuntimeConfig) -> Result<SmoothieRuntimePaths, String> {
    Ok(SmoothieRuntimePaths {
        root: required_directory(&config.smoothie.root, "Smoothie root")?,
        executable: required_file(&config.smoothie.executable, "smoothie-rs")?,
    })
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
        if denominator > 0.0 { numerator / denominator } else { 30.0 }
    } else {
        30.0
    };
    let duration = json["format"]["duration"]
        .as_str()
        .and_then(|value| value.parse().ok())
        .or_else(|| stream["duration"].as_str().and_then(|value| value.parse().ok()))
        .unwrap_or(0.0);

    let mut audio_command = Command::new(ffprobe);
    audio_command
        .args([
            "-v", "error", "-select_streams", "a:0", "-show_entries", "stream=codec_type", "-of", "csv=p=0",
        ])
        .arg(video_path);
    #[cfg(target_os = "windows")]
    audio_command.creation_flags(CREATE_NO_WINDOW);
    let has_audio = audio_command
        .output()
        .await
        .map(|output| !String::from_utf8_lossy(&output.stdout).trim().is_empty())
        .unwrap_or(false);

    Ok(VideoInfo { width, height, fps, duration, has_audio })
}

fn rife_output_path(video_path: &str, mode: &str, factor: f64, input_fps: f64) -> Result<PathBuf, String> {
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

fn ensure_destination_available(path: &Path) -> Result<(), String> {
    if path.exists() {
        return Err(format!(
            "Refusing to overwrite an existing output: {}",
            path.display()
        ));
    }
    Ok(())
}

#[tauri::command]
fn get_runtime_snapshot(app: tauri::AppHandle) -> RuntimeSnapshot {
    runtime_snapshot(&app)
}

#[tauri::command]
fn save_runtime_config(app: tauri::AppHandle, config: RuntimeConfig) -> Result<RuntimeSnapshot, String> {
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
        "rife_python" | "rife_script" | "rife_model" | "smoothie_executable" | "smoothie_recipe" | "ffmpeg" | "ffprobe" => {
            rfd::FileDialog::new().pick_file()
        }
        _ => None,
    })
    .await
    .map_err(|error| error.to_string())?;
    Ok(selected.map(|path| path.to_string_lossy().to_string()))
}

#[tauri::command]
async fn analyze_video(app: tauri::AppHandle, video_path: String) -> Result<VideoInfo, String> {
    let config = load_config(&app)?;
    let media = media_tools(&config)?;
    probe_video(&video_path, &media.ffprobe).await
}

#[tauri::command]
async fn run_time_remap(
    app: tauri::AppHandle,
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
    let out_path = rife_output_path(&video_path, &mode, factor, info.fps)?;
    ensure_destination_available(&out_path)?;

    let mut command = Command::new(&runtime.python);
    command
        .arg(&runtime.script)
        .arg("--video").arg(&video_path)
        .arg("--mode").arg(&mode)
        .arg("--factor").arg(factor.to_string())
        .arg("--crf").arg(crf.to_string())
        .arg("--preset").arg(&preset)
        .arg("--scene_threshold").arg(scene_threshold.to_string())
        .arg("--blend-cuts").arg(blend_cuts.to_string())
        .arg("--output").arg(&out_path)
        .arg("--ffmpeg").arg(&runtime.media.ffmpeg)
        .arg("--ffprobe").arg(&runtime.media.ffprobe)
        .arg("--rife-dir").arg(&runtime.directory)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    #[cfg(target_os = "windows")]
    command.creation_flags(CREATE_NO_WINDOW);

    let mut child = command
        .spawn()
        .map_err(|error| format!("Failed to start Python runtime: {error}"))?;
    let stdout = child.stdout.take().ok_or("Python stdout was unavailable")?;
    let stderr = child.stderr.take().ok_or("Python stderr was unavailable")?;
    let output_app = app.clone();
    let error_app = app.clone();
    let output_task = tokio::spawn(async move { pump(stdout, output_app).await });
    let error_task = tokio::spawn(async move { pump(stderr, error_app).await });
    let status = child.wait().await.map_err(|error| error.to_string())?;
    let _ = output_task.await;
    let _ = error_task.await;

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
    video_path: String,
    output_fps: u32,
    overrides: Vec<String>,
) -> Result<String, String> {
    let config = load_config(&app)?;
    let runtime = smoothie_runtime(&config)?;
    let out_path = smoothie_output_path(&video_path, output_fps)?;
    ensure_destination_available(&out_path)?;
    let out_path_text = out_path.to_string_lossy().to_string();

    let mut command = Command::new(&runtime.executable);
    command
        .current_dir(&runtime.root)
        .arg("-i").arg(&video_path)
        .arg("-o").arg(&out_path_text)
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
    let stdout = child.stdout.take().ok_or("Smoothie stdout was unavailable")?;
    let stderr = child.stderr.take().ok_or("Smoothie stderr was unavailable")?;
    let output_app = app.clone();
    let error_app = app.clone();
    let output_task = tokio::spawn(async move { pump(stdout, output_app).await });
    let error_task = tokio::spawn(async move { pump(stderr, error_app).await });
    let status = child.wait().await.map_err(|error| error.to_string())?;
    let _ = output_task.await;
    let _ = error_task.await;

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
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![
            get_runtime_snapshot,
            save_runtime_config,
            save_ui_preferences,
            pick_runtime_path,
            analyze_video,
            run_time_remap,
            run_smoothie,
            open_file_dialog,
            open_target_file,
            open_target_folder,
            open_about_link
        ])
        .run(tauri::generate_context!())
        .expect("error while running CIA RENDER");
}

#[cfg(test)]
mod tests {
    use super::{rife_output_path, smoothie_output_path};

    #[test]
    fn interpolation_name_uses_only_the_actual_output_fps() {
        let output = rife_output_path(r"C:\media\clip.mp4", "boost", 12.0, 30.0).unwrap();
        assert_eq!(output, std::path::PathBuf::from(r"C:\media\clip-360fps.mp4"));
    }

    #[test]
    fn smoothie_name_uses_its_input_stem_and_selected_fps() {
        let output = smoothie_output_path(r"C:\media\clip-360fps.mp4", 30).unwrap();
        assert_eq!(
            output,
            std::path::PathBuf::from(r"C:\media\clip-360fps_render30fps.mp4")
        );
    }
}
