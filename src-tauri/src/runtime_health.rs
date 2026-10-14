use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::io::Read;
use std::path::{Component, Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, LazyLock, Mutex};
use std::time::{Duration, Instant};

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub(crate) struct RifeHealth {
    pub status: String,
    pub detail: String,
    #[serde(default, alias = "profile_id")]
    pub profile_id: Option<String>,
    #[serde(default)]
    pub device: Option<Value>,
}

impl RifeHealth {
    pub fn unavailable(status: &str, detail: impl Into<String>) -> Self {
        Self {
            status: status.into(),
            detail: detail.into(),
            profile_id: None,
            device: None,
        }
    }
    pub fn ready(&self) -> bool {
        self.status == "ready"
    }
}

type HealthCache = HashMap<String, (Instant, RifeHealth)>;
static CACHE: LazyLock<Mutex<HealthCache>> = LazyLock::new(|| Mutex::new(HashMap::new()));
static CLOSING: AtomicBool = AtomicBool::new(false);
#[cfg(target_os = "windows")]
static OWNERS: LazyLock<Mutex<HashMap<u32, Arc<super::process_control::ProcessOwner>>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

pub(crate) fn shutdown() {
    CLOSING.store(true, Ordering::Relaxed);
    #[cfg(target_os = "windows")]
    {
        let owners = OWNERS
            .lock()
            .map(|owners| owners.values().cloned().collect::<Vec<_>>())
            .unwrap_or_default();
        for owner in owners {
            owner.terminate();
        }
    }
}

pub(crate) fn invalidate() {
    if let Ok(mut cache) = CACHE.lock() {
        cache.clear();
    }
}

pub(crate) fn pointer_path(root: &Path, relative: &str) -> Result<PathBuf, String> {
    let path = Path::new(relative);
    if path.is_absolute()
        || path
            .components()
            .any(|part| !matches!(part, Component::Normal(_)))
    {
        return Err("The managed runtime pointer contains an unsafe path".into());
    }
    let resolved = root.join(path);
    if !resolved.starts_with(root) {
        return Err("The managed runtime path leaves its root".into());
    }
    Ok(resolved)
}

pub(crate) fn fingerprint(paths: &[&Path]) -> String {
    paths
        .iter()
        .map(|path| {
            let metadata = std::fs::metadata(path).ok();
            format!(
                "{}:{:?}:{:?}",
                path.display(),
                metadata.as_ref().map(|m| m.len()),
                metadata.and_then(|m| m.modified().ok())
            )
        })
        .collect::<Vec<_>>()
        .join("|")
}

pub(crate) fn selected_gpu(rife: &Path) -> Option<String> {
    let manifest = rife.parent()?.join("runtime-manifest.json");
    let bytes = std::fs::read(manifest).ok()?;
    let value: Value =
        serde_json::from_slice(bytes.strip_prefix(&[0xef, 0xbb, 0xbf]).unwrap_or(&bytes)).ok()?;
    let index = value["gpuIndex"]
        .as_str()
        .map(str::to_owned)
        .or_else(|| value["gpuIndex"].as_u64().map(|v| v.to_string()))?;
    (!index.is_empty() && index.len() <= 3 && index.chars().all(|c| c.is_ascii_digit()))
        .then_some(index)
}

fn read_bounded(mut pipe: impl Read) -> String {
    let mut bytes = Vec::new();
    let mut chunk = [0u8; 4096];
    while let Ok(count) = pipe.read(&mut chunk) {
        if count == 0 {
            break;
        }
        bytes.extend_from_slice(&chunk[..count]);
        if bytes.len() > 64 * 1024 {
            bytes.drain(..bytes.len() - 64 * 1024);
        }
    }
    String::from_utf8_lossy(&bytes).into_owned()
}

pub(crate) fn probe(
    python: &Path,
    script: &Path,
    rife: &Path,
    model: &Path,
    force: bool,
) -> RifeHealth {
    if CLOSING.load(Ordering::Relaxed) {
        return RifeHealth::unavailable(
            "broken",
            "Runtime verification cancelled while the application is closing.",
        );
    }
    let manifest = rife.parent().unwrap_or(rife).join("runtime-manifest.json");
    let weights = model.join("flownet.pkl");
    let model_code = model.join("RIFE_HDv3.py");
    let network_code = model.join("IFNet_HDv3.py");
    let mut key = fingerprint(&[
        python,
        script,
        rife,
        &weights,
        &model_code,
        &network_code,
        &manifest,
    ]);
    // Reuse the full verification only while every installed file's metadata
    // remains unchanged. A removed or modified dependency invalidates readiness
    // without hashing several GiB before each short interpolation job.
    if let Ok(bytes) = std::fs::read(&manifest) {
        if let Ok(value) = serde_json::from_slice::<Value>(
            bytes.strip_prefix(&[0xef, 0xbb, 0xbf]).unwrap_or(&bytes),
        ) {
            if let Some(files) = value["files"].as_object() {
                let root = manifest.parent().unwrap_or(rife);
                let mut hash = std::collections::hash_map::DefaultHasher::new();
                for relative in files.keys() {
                    relative.hash(&mut hash);
                    let metadata = std::fs::metadata(root.join(relative)).ok();
                    metadata.as_ref().map(|m| m.len()).hash(&mut hash);
                    metadata.and_then(|m| m.modified().ok()).hash(&mut hash);
                }
                key.push_str(&format!(":{}", hash.finish()));
            }
        }
    }
    if !force {
        if let Ok(cache) = CACHE.lock() {
            if let Some((time, health)) = cache.get(&key) {
                if time.elapsed() < Duration::from_secs(300) {
                    return health.clone();
                }
            }
        }
    }
    let mut command = Command::new(python);
    command
        .args(["-I", "-X", "utf8", "-u"])
        .arg(script)
        .arg("--rife-dir")
        .arg(rife)
        .arg("--model-dir")
        .arg(model)
        .arg("--json")
        .env_remove("PYTHONHOME")
        .env_remove("PYTHONPATH")
        .env_remove("PYTHONIOENCODING")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    if manifest.is_file() {
        command.arg("--manifest").arg(&manifest);
    }
    if let Some(index) = selected_gpu(rife) {
        command.env("CUDA_VISIBLE_DEVICES", index);
    }
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(super::CREATE_NO_WINDOW);
    }
    #[cfg(target_os = "windows")]
    let spawned = super::process_control::spawn_std_owned(&mut command)
        .map(|(child, owner)| (child, Some(Arc::new(owner))));
    #[cfg(not(target_os = "windows"))]
    let spawned = command
        .spawn()
        .map(|child| (child, None::<()>))
        .map_err(|error| error.to_string());
    let health = match spawned {
        Err(error) => RifeHealth::unavailable(
            "broken",
            format!("The RIFE interpreter cannot start: {error}. Select Repair environment."),
        ),
        Ok((mut child, owner)) => {
            #[cfg(target_os = "windows")]
            if let Some(owner) = owner.as_ref() {
                if let Ok(mut owners) = OWNERS.lock() {
                    owners.insert(child.id(), owner.clone());
                }
                if CLOSING.load(Ordering::Relaxed) {
                    owner.terminate();
                }
            }
            let pid = child.id();
            let stdout = child
                .stdout
                .take()
                .map(|p| std::thread::spawn(move || read_bounded(p)));
            let stderr = child
                .stderr
                .take()
                .map(|p| std::thread::spawn(move || read_bounded(p)));
            let start = Instant::now();
            let outcome = loop {
                match child.try_wait() {
                    Ok(Some(status)) => break Ok(status),
                    Ok(None)
                        if start.elapsed() < Duration::from_secs(90)
                            && !CLOSING.load(Ordering::Relaxed) =>
                    {
                        std::thread::sleep(Duration::from_millis(50))
                    }
                    Ok(None) => {
                        let _ = child.kill();
                        let _ = child.wait();
                        break Err("Runtime verification timed out. Select Repair environment."
                            .to_string());
                    }
                    Err(error) => {
                        let _ = child.kill();
                        let _ = child.wait();
                        break Err(format!("Runtime verification failed: {error}"));
                    }
                }
            };
            #[cfg(target_os = "windows")]
            if let Some(owner) = owner.as_ref() {
                owner.terminate();
            }
            let output = stdout
                .and_then(|thread| thread.join().ok())
                .unwrap_or_default();
            let errors = stderr
                .and_then(|thread| thread.join().ok())
                .unwrap_or_default();
            #[cfg(target_os = "windows")]
            if let Ok(mut owners) = OWNERS.lock() {
                owners.remove(&pid);
            }
            drop(owner);
            match outcome {
                Err(error) => RifeHealth::unavailable("broken", error),
                Ok(status) => {
                    let parsed = output
                        .lines()
                        .rev()
                        .find_map(|line| serde_json::from_str::<RifeHealth>(line).ok());
                    match parsed {
                        Some(health) if status.success() || !health.ready() => health,
                        _ => RifeHealth::unavailable(
                            "broken",
                            format!(
                                "RIFE verification failed ({status}): {}",
                                errors.lines().last().unwrap_or(
                                    "No valid health result. Select Repair environment."
                                )
                            ),
                        ),
                    }
                }
            }
        }
    };
    if let Ok(mut cache) = CACHE.lock() {
        if cache.len() >= 8 {
            cache.clear();
        }
        cache.insert(key, (Instant::now(), health.clone()));
    }
    health
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn managed_pointer_rejects_absolute_parent_and_prefix_paths() {
        let root = Path::new("C:/runtime");
        assert!(pointer_path(root, "generations/healthy/venv/Scripts/python.exe").is_ok());
        for path in [
            "../outside",
            "generations/../../outside",
            "C:/outside",
            "\\\\server\\share\\x",
        ] {
            assert!(pointer_path(root, path).is_err(), "{path}");
        }
    }
    #[test]
    fn health_requires_explicit_success() {
        assert!(!RifeHealth::unavailable("broken", "bad import").ready());
        assert!(!RifeHealth::unavailable("unsupported", "old device").ready());
    }
}
