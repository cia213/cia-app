use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use tauri::Emitter;
use tokio::io::{AsyncRead, AsyncReadExt};
use tokio::process::Command;

#[cfg(target_os = "windows")]
const CREATE_NO_WINDOW: u32 = 0x08000000;

#[derive(Serialize, Deserialize, Debug, Clone)]
struct VideoInfo {
    width: u32,
    height: u32,
    fps: f64,
    duration: f64,
    has_audio: bool,
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
                let (consumed, segs) = {
                    let bytes = pending.as_bytes();
                    let mut v: Vec<String> = Vec::new();
                    let mut s = 0usize;
                    for i in 0..bytes.len() {
                        if bytes[i] == b'\r' || bytes[i] == b'\n' {
                            if let Ok(seg) = std::str::from_utf8(&bytes[s..i]) {
                                let t = seg.trim();
                                if !t.is_empty() {
                                    v.push(t.to_string());
                                }
                            }
                            s = i + 1;
                        }
                    }
                    (s, v)
                };
                pending.drain(..consumed);
                for seg in segs {
                    let _ = app.emit("live-log", &seg);
                }
            }
            Err(_) => break,
        }
    }
    let t = pending.trim();
    if !t.is_empty() {
        let _ = app.emit("live-log", t);
    }
}

async fn probe_video(video_path: &str) -> Result<VideoInfo, String> {
    let mut cmd = Command::new("ffprobe");
    cmd.args([
        "-v", "error",
        "-select_streams", "v:0",
        "-show_entries", "stream=width,height,r_frame_rate,duration",
        "-show_entries", "format=duration",
        "-of", "json",
    ])
    .arg(video_path);

    #[cfg(target_os = "windows")]
    cmd.creation_flags(CREATE_NO_WINDOW);

    let output = cmd
        .output()
        .await
        .map_err(|e| format!("ffprobe failed ({e}). Ensure ffmpeg/ffprobe are in system PATH."))?;

    if !output.status.success() {
        return Err(format!("ffprobe error: {}", String::from_utf8_lossy(&output.stderr)));
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    let json: serde_json::Value =
        serde_json::from_str(&stdout).map_err(|e| format!("JSON parse: {e} (raw: {stdout})"))?;

    let stream = &json["streams"][0];
    let width = stream["width"].as_u64().unwrap_or(0) as u32;
    let height = stream["height"].as_u64().unwrap_or(0) as u32;
    let fps_str = stream["r_frame_rate"].as_str().unwrap_or("30/1");
    let fps = if let Some((n, d)) = fps_str.split_once('/') {
        let nn: f64 = n.parse().unwrap_or(30.0);
        let dd: f64 = d.parse().unwrap_or(1.0);
        if dd > 0.0 { nn / dd } else { 30.0 }
    } else {
        30.0
    };
    let duration = json["format"]["duration"]
        .as_str()
        .and_then(|s| s.parse().ok())
        .or_else(|| stream["duration"].as_str().and_then(|s| s.parse().ok()))
        .unwrap_or(0.0);

    let mut audio_cmd = Command::new("ffprobe");
    audio_cmd
        .args(["-v", "error", "-select_streams", "a:0", "-show_entries", "stream=codec_type", "-of", "csv=p=0"])
        .arg(video_path);

    #[cfg(target_os = "windows")]
    audio_cmd.creation_flags(CREATE_NO_WINDOW);

    let audio = audio_cmd.output().await;
    let has_audio = audio
        .map(|o| !String::from_utf8_lossy(&o.stdout).trim().is_empty())
        .unwrap_or(false);

    Ok(VideoInfo { width, height, fps, duration, has_audio })
}

#[tauri::command]
async fn analyze_video(video_path: String) -> Result<VideoInfo, String> {
    probe_video(&video_path).await
}

fn format_factor(factor: f64) -> String {
    if (factor - factor.round()).abs() < f64::EPSILON {
        format!("{:.0}", factor)
    } else {
        factor.to_string()
    }
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
        .ok_or("Unable to derive the RIFE output filename")?;
    let output_fps = match mode {
        "boost" => (input_fps * factor).round(),
        "slowmo" => input_fps.round(),
        _ => return Err(format!("Unsupported interpolation mode: {mode}")),
    };

    if output_fps <= 0.0 {
        return Err("Unable to derive a valid output framerate".to_string());
    }

    Ok(parent.join(format!(
        "{stem}-{}x-RIFE-4.26-{}fps.mp4",
        format_factor(factor),
        output_fps as u64
    )))
}

fn ensure_nonempty_file(path: &Path, label: &str) -> Result<(), String> {
    let metadata = std::fs::metadata(path)
        .map_err(|error| format!("{label} output is missing: {} ({error})", path.display()))?;
    if !metadata.is_file() || metadata.len() == 0 {
        return Err(format!("{label} output is invalid: {}", path.display()));
    }
    Ok(())
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
    let python_exe = r"C:\Users\cia\time-remap-app\venv\Scripts\python.exe";
    let script_path = r"C:\Users\cia\time-remap-app\time_remap.py";
    let info = probe_video(&video_path).await?;
    let out_path = rife_output_path(&video_path, &mode, factor, info.fps)?;

    if !Path::new(python_exe).is_file() || !Path::new(script_path).is_file() {
        return Err("The local RIFE runtime is unavailable".to_string());
    }

    let mut cmd = Command::new(python_exe);
    cmd.arg(script_path)
        .arg("--video").arg(&video_path)
        .arg("--mode").arg(&mode)
        .arg("--factor").arg(factor.to_string())
        .arg("--crf").arg(crf.to_string())
        .arg("--preset").arg(&preset)
        .arg("--scene_threshold").arg(scene_threshold.to_string())
        .arg("--blend-cuts").arg(blend_cuts.to_string())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    #[cfg(target_os = "windows")]
    cmd.creation_flags(CREATE_NO_WINDOW);

    let mut child = cmd
        .spawn()
        .map_err(|e| format!("Failed to start Python ({python_exe}): {e}"))?;

    let stdout = child.stdout.take().ok_or("no stdout pipe")?;
    let stderr = child.stderr.take().ok_or("no stderr pipe")?;
    let app_out = app.clone();
    let app_err = app.clone();
    let t_out = tokio::spawn(async move { pump(stdout, app_out).await });
    let t_err = tokio::spawn(async move { pump(stderr, app_err).await });

    let status = child.wait().await.map_err(|e| e.to_string())?;
    let _ = t_out.await;
    let _ = t_err.await;

    if status.success() {
        ensure_nonempty_file(&out_path, "RIFE")?;
        Ok(out_path.to_string_lossy().to_string())
    } else {
        Err(format!("time_remap.py process failed ({status})"))
    }
}

#[tauri::command]
async fn run_smoothie(app: tauri::AppHandle, video_path: String, overrides: Vec<String>) -> Result<String, String> {
    let smoothie_dir = r"C:\Users\cia\Music\smoothie1";
    let smoothie_exe = r"C:\Users\cia\Music\smoothie1\bin\smoothie-rs.exe";

    if !Path::new(smoothie_exe).is_file() {
        return Err("The local smoothie-rs runtime is unavailable".to_string());
    }

    let out_path = {
        let p = std::path::Path::new(&video_path);
        let dir = p.parent().unwrap_or(std::path::Path::new(""));
        let stem = p.file_stem().and_then(|s| s.to_str()).unwrap_or("output");
        dir.join(format!("{}_smoothie.mp4", stem))
    };
    let out_path_str = out_path.to_string_lossy().to_string();

    let mut cmd = Command::new(smoothie_exe);
    cmd.current_dir(smoothie_dir)
        .arg("-i").arg(&video_path)
        .arg("-o").arg(&out_path_str)
        .arg("--progress");

    if !overrides.is_empty() {
        cmd.arg("--override");
        for ov in &overrides {
            cmd.arg(ov);
        }
    }

    cmd.stdout(Stdio::piped())
        .stderr(Stdio::piped());

    #[cfg(target_os = "windows")]
    cmd.creation_flags(CREATE_NO_WINDOW);

    let mut child = cmd
        .spawn()
        .map_err(|e| format!("Failed to start smoothie-rs ({smoothie_exe}): {e}"))?;

    let stdout = child.stdout.take().ok_or("no stdout pipe")?;
    let stderr = child.stderr.take().ok_or("no stderr pipe")?;
    let app_out = app.clone();
    let app_err = app.clone();
    let t_out = tokio::spawn(async move { pump(stdout, app_out).await });
    let t_err = tokio::spawn(async move { pump(stderr, app_err).await });

    let status = child.wait().await.map_err(|e| e.to_string())?;
    let _ = t_out.await;
    let _ = t_err.await;

    if status.success() {
        ensure_nonempty_file(&out_path, "Smoothie")?;
        Ok(out_path_str)
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
    .map_err(|e| e.to_string())?;
    Ok(file.map(|p| p.to_string_lossy().to_string()))
}

#[tauri::command]
fn open_target_file(path: String) -> Result<(), String> {
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        let _ = std::process::Command::new("cmd")
            .args(["/C", "start", "", &path])
            .creation_flags(CREATE_NO_WINDOW)
            .spawn();
    }
    Ok(())
}

#[tauri::command]
fn open_target_folder(path: String) -> Result<(), String> {
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        let _ = std::process::Command::new("explorer")
            .arg(format!("/select,{}", path))
            .creation_flags(CREATE_NO_WINDOW)
            .spawn();
    }
    Ok(())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![
            analyze_video,
            run_time_remap,
            run_smoothie,
            open_file_dialog,
            open_target_file,
            open_target_folder
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
