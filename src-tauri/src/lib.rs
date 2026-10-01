use base64::engine::general_purpose::STANDARD as BASE64_STANDARD;
use base64::Engine;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::{HashMap, VecDeque};
use std::env;
use std::fs::{self, OpenOptions};
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use tauri::{Emitter, Manager};
use tokio::io::{AsyncRead, AsyncReadExt};
use tokio::process::Command;

#[cfg(target_os = "windows")]
const CREATE_NO_WINDOW: u32 = 0x08000000;

const CONFIG_SCHEMA_VERSION: u32 = 1;
static CONFIG_WRITE_LOCK: Mutex<()> = Mutex::new(());
static TEMPORARY_ID: AtomicU64 = AtomicU64::new(0);
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
    frame_count: Option<u64>,
    color_transfer: Option<String>,
    color_space: Option<String>,
    color_primaries: Option<String>,
    sample_aspect_ratio: String,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct VideoPreviewSet {
    cover: String,
    frames: Vec<String>,
}

struct MediaToolPaths {
    ffmpeg: PathBuf,
    ffprobe: PathBuf,
}

struct RifeRuntimePaths {
    python: PathBuf,
    script: PathBuf,
    directory: PathBuf,
    model_directory: PathBuf,
    media: MediaToolPaths,
}

struct SmoothieRuntimePaths {
    root: PathBuf,
    executable: PathBuf,
    ffmpeg_directory: PathBuf,
    recipe: PathBuf,
    lut_file: Option<PathBuf>,
    ffprobe: PathBuf,
}

struct OutputReservation {
    output: PathBuf,
    lock: PathBuf,
    work_directory: Option<PathBuf>,
    process_owner: Option<Arc<process_control::ProcessOwner>>,
}

impl Drop for OutputReservation {
    fn drop(&mut self) {
        if let Some(owner) = &self.process_owner {
            owner.terminate();
        }
        if let Some(work_directory) = &self.work_directory {
            let _ = fs::remove_dir_all(work_directory);
        }
        let _ = fs::remove_file(&self.lock);
    }
}

impl OutputReservation {
    fn prepare_work_directory(&mut self) -> Result<PathBuf, String> {
        let parent = self
            .output
            .parent()
            .ok_or("Invalid render output directory")?;
        for _ in 0..100 {
            let directory = parent.join(format!(
                ".cia-render-{}-{}-{}",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_nanos(),
                TEMPORARY_ID.fetch_add(1, Ordering::Relaxed)
            ));
            match fs::create_dir(&directory) {
                Ok(()) => {
                    self.work_directory = Some(directory.clone());
                    return Ok(directory);
                }
                Err(error) if error.kind() == ErrorKind::AlreadyExists => continue,
                Err(error) => return Err(format!("Unable to prepare render workspace: {error}")),
            }
        }
        Err("Unable to reserve a unique render workspace".to_string())
    }

    fn publish(&self, working_output: &Path) -> Result<String, String> {
        publish_file_without_overwrite(working_output, &self.output)?;
        Ok(self.output.to_string_lossy().to_string())
    }
}

#[cfg(target_os = "windows")]
fn publish_file_without_overwrite(source: &Path, destination: &Path) -> Result<(), String> {
    use std::os::windows::ffi::OsStrExt;
    #[link(name = "kernel32")]
    extern "system" {
        fn MoveFileExW(existing: *const u16, new: *const u16, flags: u32) -> i32;
    }
    let source_wide: Vec<u16> = source
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();
    let destination_wide: Vec<u16> = destination
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();
    if unsafe { MoveFileExW(source_wide.as_ptr(), destination_wide.as_ptr(), 0) } == 0 {
        return Err(format!(
            "Unable to publish render without overwriting {}: {}",
            destination.display(),
            std::io::Error::last_os_error()
        ));
    }
    Ok(())
}

#[cfg(not(target_os = "windows"))]
fn publish_file_without_overwrite(source: &Path, destination: &Path) -> Result<(), String> {
    fs::hard_link(source, destination).map_err(|error| {
        format!(
            "Unable to publish render without overwriting {}: {error}",
            destination.display()
        )
    })?;
    let _ = fs::remove_file(source);
    Ok(())
}

#[derive(Clone)]
struct RenderJob {
    process_id: Option<u32>,
    owner: Option<Arc<process_control::ProcessOwner>>,
    paused: bool,
    cancel_requested: bool,
}

#[derive(Default)]
struct JobRegistry {
    jobs: Mutex<HashMap<String, RenderJob>>,
    previews: Mutex<HashMap<String, PreviewJob>>,
    preview_cache: Mutex<VecDeque<(String, VideoPreviewSet)>>,
}

#[derive(Default)]
struct PreviewJob {
    cancelled: bool,
    owner: Option<Arc<process_control::ProcessOwner>>,
}

struct PreviewGuard<'a> {
    registry: &'a JobRegistry,
    request_id: String,
}

impl<'a> PreviewGuard<'a> {
    fn reserve(registry: &'a JobRegistry, request_id: Option<String>) -> Result<Self, String> {
        let render_jobs = registry
            .jobs
            .lock()
            .map_err(|_| "Render job registry is unavailable")?;
        if !render_jobs.is_empty() {
            return Err("Video previews are deferred while a render is running".to_string());
        }
        let request_id = request_id
            .unwrap_or_else(|| format!("preview-{}", TEMPORARY_ID.fetch_add(1, Ordering::Relaxed)));
        if request_id.trim().is_empty() {
            return Err("A preview request identifier is required".to_string());
        }
        let mut previews = registry
            .previews
            .lock()
            .map_err(|_| "Preview registry is unavailable")?;
        if let Some(previous) = previews.remove(&request_id) {
            if previous.cancelled {
                return Err("CIA_PREVIEW_CANCELLED".to_string());
            }
            if let Some(owner) = previous.owner {
                owner.terminate();
            }
            return Err("A preview request with this identifier is already running".to_string());
        }
        previews.insert(request_id.clone(), PreviewJob::default());
        Ok(Self {
            registry,
            request_id,
        })
    }

    fn spawn(&self, command: &mut Command) -> Result<tokio::process::Child, String> {
        let render_jobs = self
            .registry
            .jobs
            .lock()
            .map_err(|_| "Render job registry is unavailable")?;
        if !render_jobs.is_empty() {
            return Err("Video previews are deferred while a render is running".to_string());
        }
        let mut previews = self
            .registry
            .previews
            .lock()
            .map_err(|_| "Preview registry is unavailable")?;
        let preview = previews
            .get_mut(&self.request_id)
            .ok_or("CIA_PREVIEW_CANCELLED")?;
        if preview.cancelled {
            return Err("CIA_PREVIEW_CANCELLED".to_string());
        }
        let (child, owner) = process_control::spawn_owned(command)?;
        preview.owner = Some(Arc::new(owner));
        Ok(child)
    }

    fn check_cancelled(&self) -> Result<(), String> {
        let previews = self
            .registry
            .previews
            .lock()
            .map_err(|_| "Preview registry is unavailable")?;
        if previews
            .get(&self.request_id)
            .is_none_or(|preview| preview.cancelled)
        {
            return Err("CIA_PREVIEW_CANCELLED".to_string());
        }
        Ok(())
    }
}

impl Drop for PreviewGuard<'_> {
    fn drop(&mut self) {
        if let Ok(mut previews) = self.registry.previews.lock() {
            if let Some(preview) = previews.remove(&self.request_id) {
                if let Some(owner) = preview.owner {
                    owner.terminate();
                }
            }
        }
    }
}

#[tauri::command]
fn cancel_video_previews(
    request_id: String,
    registry: tauri::State<JobRegistry>,
) -> Result<(), String> {
    let mut previews = registry
        .previews
        .lock()
        .map_err(|_| "Preview registry is unavailable")?;
    // A bounded tombstone also handles cancellation that arrives before invocation.
    if previews.len() >= 128 && !previews.contains_key(&request_id) {
        let stale = previews
            .iter()
            .find(|(_, preview)| preview.owner.is_none())
            .map(|(id, _)| id.clone());
        if let Some(stale) = stale {
            previews.remove(&stale);
        }
    }
    let preview = previews.entry(request_id).or_default();
    preview.cancelled = true;
    if let Some(owner) = &preview.owner {
        owner.terminate();
    }
    Ok(())
}

struct JobGuard<'a> {
    registry: &'a JobRegistry,
    job_id: String,
}

impl<'a> JobGuard<'a> {
    fn reserve(registry: &'a JobRegistry, job_id: &str) -> Result<Self, String> {
        if job_id.trim().is_empty() {
            return Err("A render job identifier is required".to_string());
        }
        let mut jobs = registry
            .jobs
            .lock()
            .map_err(|_| "Render job registry is unavailable")?;
        if !jobs.is_empty() {
            return Err("Another render job is already running".to_string());
        }
        jobs.insert(
            job_id.to_string(),
            RenderJob {
                process_id: None,
                owner: None,
                paused: false,
                cancel_requested: false,
            },
        );
        drop(jobs);
        registry.cancel_all_previews();
        Ok(Self {
            registry,
            job_id: job_id.to_string(),
        })
    }

    fn check_cancelled(&self) -> Result<(), String> {
        if running_job(self.registry, &self.job_id)?.cancel_requested {
            Err("CIA_RENDER_CANCELLED".to_string())
        } else {
            Ok(())
        }
    }

    fn spawn(&self, command: &mut Command) -> Result<tokio::process::Child, String> {
        // Keep reservation and cancellation atomic with launching the process.
        let mut jobs = self
            .registry
            .jobs
            .lock()
            .map_err(|_| "Render job registry is unavailable")?;
        let job = jobs
            .get_mut(&self.job_id)
            .ok_or("The render job is no longer running")?;
        if job.cancel_requested {
            return Err("CIA_RENDER_CANCELLED".to_string());
        }
        let (child, owner) = process_control::spawn_owned(command)?;
        job.process_id = child.id();
        job.owner = Some(Arc::new(owner));
        Ok(child)
    }

    fn terminate_descendants(&self) {
        if let Ok(job) = running_job(self.registry, &self.job_id) {
            if let Some(owner) = job.owner {
                owner.terminate();
            }
        }
    }

    async fn wait_for_descendants(&self) -> Result<(), String> {
        if let Some(owner) = running_job(self.registry, &self.job_id)?.owner {
            tokio::task::spawn_blocking(move || owner.wait_for_exit())
                .await
                .map_err(|error| error.to_string())??;
        }
        self.check_cancelled()
    }
}

impl Drop for JobGuard<'_> {
    fn drop(&mut self) {
        if let Ok(mut jobs) = self.registry.jobs.lock() {
            if let Some(job) = jobs.remove(&self.job_id) {
                if let Some(owner) = job.owner {
                    owner.terminate();
                }
            }
        }
    }
}

fn running_job(registry: &JobRegistry, job_id: &str) -> Result<RenderJob, String> {
    registry
        .jobs
        .lock()
        .map_err(|_| "Render job registry is unavailable")?
        .get(job_id)
        .cloned()
        .ok_or_else(|| "The render job is no longer running".to_string())
}

impl JobRegistry {
    fn cancel_all_previews(&self) {
        if let Ok(mut previews) = self.previews.lock() {
            for preview in previews.values_mut() {
                preview.cancelled = true;
                if let Some(owner) = &preview.owner {
                    owner.terminate();
                }
            }
        }
    }

    fn shutdown(&self) {
        if let Ok(mut jobs) = self.jobs.lock() {
            for job in jobs.values_mut() {
                job.cancel_requested = true;
                if let Some(owner) = &job.owner {
                    owner.terminate();
                }
            }
        }
        self.cancel_all_previews();
    }
}

#[cfg(target_os = "windows")]
mod process_control {
    use super::{Command, CREATE_NO_WINDOW};
    use std::collections::VecDeque;
    use std::ffi::c_void;
    use std::mem::size_of;

    type Handle = *mut c_void;
    const INVALID_HANDLE_VALUE: Handle = -1isize as Handle;
    const TH32CS_SNAPPROCESS: u32 = 0x0000_0002;
    const PROCESS_SUSPEND_RESUME: u32 = 0x0000_0800;
    const PROCESS_ALL_NEEDED: u32 = 0x0000_0100 | 0x0000_0001 | PROCESS_SUSPEND_RESUME;
    const CREATE_SUSPENDED: u32 = 0x0000_0004;

    #[repr(C)]
    #[derive(Default)]
    struct BasicLimitInformation {
        per_process_user_time_limit: i64,
        per_job_user_time_limit: i64,
        limit_flags: u32,
        minimum_working_set_size: usize,
        maximum_working_set_size: usize,
        active_process_limit: u32,
        affinity: usize,
        priority_class: u32,
        scheduling_class: u32,
    }

    #[repr(C)]
    #[derive(Default)]
    struct IoCounters {
        read_ops: u64,
        write_ops: u64,
        other_ops: u64,
        read_bytes: u64,
        write_bytes: u64,
        other_bytes: u64,
    }

    #[repr(C)]
    #[derive(Default)]
    struct ExtendedLimitInformation {
        basic: BasicLimitInformation,
        io: IoCounters,
        process_memory_limit: usize,
        job_memory_limit: usize,
        peak_process_memory: usize,
        peak_job_memory: usize,
    }

    // The integer owns a kernel handle and is never used after its Drop.
    pub struct ProcessOwner {
        handle: usize,
    }

    #[repr(C)]
    #[derive(Default)]
    struct AccountingInformation {
        user_time: i64,
        kernel_time: i64,
        period_user_time: i64,
        period_kernel_time: i64,
        page_faults: u32,
        total_processes: u32,
        active_processes: u32,
        terminated_processes: u32,
    }

    impl ProcessOwner {
        fn active_processes(&self) -> Result<u32, String> {
            let mut accounting = AccountingInformation::default();
            if unsafe {
                QueryInformationJobObject(
                    self.handle as Handle,
                    1,
                    (&mut accounting as *mut AccountingInformation).cast(),
                    size_of::<AccountingInformation>() as u32,
                    std::ptr::null_mut(),
                )
            } == 0
            {
                return Err(format!(
                    "Unable to inspect render process owner: {}",
                    std::io::Error::last_os_error()
                ));
            }
            Ok(accounting.active_processes)
        }

        pub fn wait_for_exit(&self) -> Result<(), String> {
            while self.active_processes()? != 0 {
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
            Ok(())
        }

        pub fn terminate(&self) {
            unsafe {
                TerminateJobObject(self.handle as Handle, 1);
            }
            // Termination is asynchronous. Wait before deleting files held by descendants.
            for _ in 0..200 {
                if self.active_processes().unwrap_or(0) == 0 {
                    break;
                }
                std::thread::sleep(std::time::Duration::from_millis(5));
            }
        }
    }

    impl Drop for ProcessOwner {
        fn drop(&mut self) {
            self.terminate();
            unsafe {
                CloseHandle(self.handle as Handle);
            }
        }
    }

    pub fn spawn_owned(
        command: &mut Command,
    ) -> Result<(tokio::process::Child, ProcessOwner), String> {
        let handle = unsafe { CreateJobObjectW(std::ptr::null_mut(), std::ptr::null()) };
        if handle.is_null() {
            return Err(format!(
                "Unable to create render process owner: {}",
                std::io::Error::last_os_error()
            ));
        }
        let owner = ProcessOwner {
            handle: handle as usize,
        };
        let mut limits = ExtendedLimitInformation::default();
        limits.basic.limit_flags = 0x0000_2000; // JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE
        if unsafe {
            SetInformationJobObject(
                handle,
                9,
                (&limits as *const ExtendedLimitInformation).cast(),
                size_of::<ExtendedLimitInformation>() as u32,
            )
        } == 0
        {
            return Err(format!(
                "Unable to configure render process owner: {}",
                std::io::Error::last_os_error()
            ));
        }
        command
            .creation_flags(CREATE_NO_WINDOW | CREATE_SUSPENDED)
            .kill_on_drop(true);
        let mut child = command
            .spawn()
            .map_err(|error| format!("Failed to start render process: {error}"))?;
        let pid = child
            .id()
            .ok_or("The process did not expose a process identifier")?;
        let process = unsafe { OpenProcess(PROCESS_ALL_NEEDED, 0, pid) };
        if process.is_null() {
            let _ = child.start_kill();
            return Err(format!(
                "Unable to own render process: {}",
                std::io::Error::last_os_error()
            ));
        }
        let assigned = unsafe { AssignProcessToJobObject(handle, process) };
        let resumed = if assigned != 0 {
            unsafe { NtResumeProcess(process) }
        } else {
            -1
        };
        let error = std::io::Error::last_os_error();
        unsafe {
            CloseHandle(process);
        }
        if assigned == 0 || resumed < 0 {
            owner.terminate();
            let _ = child.start_kill();
            return Err(format!(
                "Unable to attach or start render process tree: {error}"
            ));
        }
        Ok((child, owner))
    }

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
        fn CreateJobObjectW(attributes: *mut c_void, name: *const u16) -> Handle;
        fn SetInformationJobObject(
            job: Handle,
            class: i32,
            information: *const c_void,
            length: u32,
        ) -> i32;
        fn AssignProcessToJobObject(job: Handle, process: Handle) -> i32;
        fn TerminateJobObject(job: Handle, exit_code: u32) -> i32;
        fn QueryInformationJobObject(
            job: Handle,
            class: i32,
            information: *mut c_void,
            length: u32,
            returned_length: *mut u32,
        ) -> i32;
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

        let result = {
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
        };
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

#[cfg(not(target_os = "windows"))]
mod process_control {
    use super::Command;
    pub struct ProcessOwner;
    impl ProcessOwner {
        pub fn terminate(&self) {}
        pub fn wait_for_exit(&self) -> Result<(), String> {
            Ok(())
        }
    }
    pub fn spawn_owned(
        command: &mut Command,
    ) -> Result<(tokio::process::Child, ProcessOwner), String> {
        command.kill_on_drop(true);
        command
            .spawn()
            .map(|child| (child, ProcessOwner))
            .map_err(|error| error.to_string())
    }
}

fn config_path(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    app.path()
        .app_config_dir()
        .map(|dir| dir.join("config.json"))
        .map_err(|error| format!("Unable to resolve the cia render config directory: {error}"))
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
            "[cia render] Migrated config from {} to {}",
            legacy_path.display(),
            new_path.display()
        ),
        Err(error) => eprintln!(
            "[cia render] Config migration failed: {} -> {}: {error}",
            legacy_path.display(),
            new_path.display()
        ),
    }
}

fn load_config(app: &tauri::AppHandle) -> Result<RuntimeConfig, String> {
    let _guard = CONFIG_WRITE_LOCK
        .lock()
        .map_err(|_| "Configuration lock is unavailable")?;
    load_config_unlocked(app)
}

fn load_config_unlocked(app: &tauri::AppHandle) -> Result<RuntimeConfig, String> {
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
    Ok(normalize_config(config))
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
    let _guard = CONFIG_WRITE_LOCK
        .lock()
        .map_err(|_| "Configuration lock is unavailable")?;
    write_config_unlocked(app, config)
}

fn update_config(
    app: &tauri::AppHandle,
    update: impl FnOnce(&mut RuntimeConfig),
) -> Result<(), String> {
    let _guard = CONFIG_WRITE_LOCK
        .lock()
        .map_err(|_| "Configuration lock is unavailable")?;
    let mut config = load_config_unlocked(app)?;
    update(&mut config);
    write_config_unlocked(app, &normalize_config(config))
}

fn write_config_unlocked(app: &tauri::AppHandle, config: &RuntimeConfig) -> Result<(), String> {
    let path = config_path(app)?;
    let parent = path.parent().ok_or("Invalid cia render config path")?;
    fs::create_dir_all(parent)
        .map_err(|error| format!("Unable to create {}: {error}", parent.display()))?;

    let contents = serde_json::to_string_pretty(config)
        .map_err(|error| format!("Unable to serialize config.json: {error}"))?;
    let temporary = parent.join(format!(
        ".config-{}-{}.tmp",
        std::process::id(),
        TEMPORARY_ID.fetch_add(1, Ordering::Relaxed)
    ));
    use std::io::Write;
    let write_result = (|| {
        let mut file = OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&temporary)?;
        file.write_all(contents.as_bytes())?;
        file.sync_all()
    })();
    if let Err(error) = write_result {
        let _ = fs::remove_file(&temporary);
        return Err(format!("Unable to write temporary config: {error}"));
    }
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

fn strip_unc_path<P: AsRef<Path>>(path: P) -> PathBuf {
    let text = path.as_ref().to_string_lossy();
    if let Some(stripped) = text.strip_prefix(r"\\?\UNC\") {
        PathBuf::from(format!(r"\\{stripped}"))
    } else if let Some(stripped) = text.strip_prefix(r"\\?\") {
        PathBuf::from(stripped)
    } else {
        path.as_ref().to_path_buf()
    }
}

fn bundled_resource(app: &tauri::AppHandle, relative: &str) -> Option<PathBuf> {
    let resource_dir = strip_unc_path(app.path().resource_dir().ok()?);
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
        recipe: root.join("recipe.ini"),
        root,
        executable,
        ffmpeg_directory,
        lut_file: None,
        ffprobe: media.ffprobe.clone(),
    })
}

fn rife_install_root(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    app.path()
        .app_data_dir()
        .map(|directory| directory.join("runtimes").join("rife"))
        .map_err(|error| format!("Unable to resolve the cia render runtime directory: {error}"))
}

fn effective_rife_script(config: &RuntimeConfig, app: &tauri::AppHandle) -> Option<PathBuf> {
    if config.rife.script.is_some() {
        existing_file(&config.rife.script)
    } else {
        bundled_rife_script(app)
    }
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
    let rife_ready = rife_runtime(&config, app).is_ok();
    let smoothie_ready = smoothie_runtime(&config, app).is_ok();
    let python = existing_file(&config.rife.python_executable);
    let script = effective_rife_script(&config, app);
    let rife_directory = existing_directory(&config.rife.directory)
        .filter(|directory| directory.join("inference_video.py").is_file());
    let model = existing_file(&config.rife.model_file).filter(|model| {
        model.file_name().and_then(|name| name.to_str()) == Some("flownet.pkl")
            && model.parent().is_some_and(|directory| {
                directory.join("RIFE_HDv3.py").is_file()
                    && directory.join("IFNet_HDv3.py").is_file()
            })
    });
    let bundled_media = bundled_media_tools(app);
    let ffmpeg = if config.media_tools.ffmpeg.is_some() {
        existing_file(&config.media_tools.ffmpeg)
    } else {
        bundled_media.as_ref().map(|media| media.ffmpeg.clone())
    };
    let ffprobe = if config.media_tools.ffprobe.is_some() {
        existing_file(&config.media_tools.ffprobe)
    } else {
        bundled_media.as_ref().map(|media| media.ffprobe.clone())
    };
    let bundled_smoothie = bundled_media
        .as_ref()
        .and_then(|media| bundled_smoothie_runtime(app, media));
    let smoothie_root = if config.smoothie.root.is_some() {
        existing_directory(&config.smoothie.root)
    } else {
        bundled_smoothie
            .as_ref()
            .map(|runtime| runtime.root.clone())
    };
    let smoothie_executable = if config.smoothie.executable.is_some() {
        existing_file(&config.smoothie.executable)
    } else {
        bundled_smoothie
            .as_ref()
            .map(|runtime| runtime.executable.clone())
    };
    let smoothie_recipe = if config.smoothie.recipe.is_some() {
        existing_file(&config.smoothie.recipe)
    } else {
        smoothie_root
            .as_ref()
            .map(|root| root.join("recipe.ini"))
            .filter(|recipe| recipe.is_file())
    };

    let components = vec![
        component_status(
            "rife_python",
            "Python runtime",
            python.clone(),
            "Python 3.11+ executable",
        ),
        component_status(
            "rife_script",
            "cia render RIFE script",
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
        rife_ready,
        smoothie_ready,
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
    let bundled = bundled_media_tools(app);
    let ffmpeg = if config.media_tools.ffmpeg.is_some() {
        required_file(&config.media_tools.ffmpeg, "FFmpeg")?
    } else {
        bundled
            .as_ref()
            .map(|media| media.ffmpeg.clone())
            .ok_or("Bundled FFmpeg is unavailable. Configure Runtime paths.")?
    };
    let ffprobe = if config.media_tools.ffprobe.is_some() {
        required_file(&config.media_tools.ffprobe, "FFprobe")?
    } else {
        bundled
            .as_ref()
            .map(|media| media.ffprobe.clone())
            .ok_or("Bundled FFprobe is unavailable. Configure Runtime paths.")?
    };
    Ok(MediaToolPaths { ffmpeg, ffprobe })
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
    let model_directory = model.parent().ok_or("Invalid RIFE model directory")?;
    if !model_directory.join("RIFE_HDv3.py").is_file()
        || !model_directory.join("IFNet_HDv3.py").is_file()
    {
        return Err("The selected RIFE model directory must contain matching RIFE_HDv3.py and IFNet_HDv3.py code beside flownet.pkl".to_string());
    }
    let script = effective_rife_script(config, app)
        .ok_or("cia render RIFE script is unavailable. Reinstall the application or configure the script path.")?;

    Ok(RifeRuntimePaths {
        python: required_file(&config.rife.python_executable, "Python runtime")?,
        script,
        directory,
        model_directory: model_directory.to_path_buf(),
        media: media_tools(config, app)?,
    })
}

fn smoothie_runtime(
    config: &RuntimeConfig,
    app: &tauri::AppHandle,
) -> Result<SmoothieRuntimePaths, String> {
    let media = media_tools(config, app)?;
    if config.smoothie.root.is_some() {
        required_directory(&config.smoothie.root, "Smoothie root")?;
    }
    if config.smoothie.executable.is_some() {
        required_file(&config.smoothie.executable, "Smoothie executable")?;
    }
    match (
        existing_directory(&config.smoothie.root),
        existing_file(&config.smoothie.executable),
    ) {
        (Some(root), Some(executable)) => Ok(SmoothieRuntimePaths {
            recipe: required_file(&config.smoothie.recipe, "Smoothie recipe")?,
            lut_file: match &config.smoothie.lut_file {
                Some(path) if !path.trim().is_empty() => {
                    Some(required_file(&config.smoothie.lut_file, "Smoothie LUT")?)
                }
                _ => None,
            },
            ffprobe: media.ffprobe.clone(),
            root,
            executable,
            ffmpeg_directory: media
                .ffmpeg
                .parent()
                .ok_or("Invalid FFmpeg path")?
                .to_path_buf(),
        }),
        _ => {
            let mut runtime = bundled_smoothie_runtime(app, &media).ok_or_else(|| {
                "Bundled Smoothie is unavailable. Reinstall cia render or configure Runtime paths."
                    .to_string()
            })?;
            if config.smoothie.recipe.is_some() {
                runtime.recipe = required_file(&config.smoothie.recipe, "Smoothie recipe")?;
            }
            if config
                .smoothie
                .lut_file
                .as_ref()
                .is_some_and(|path| !path.trim().is_empty())
            {
                runtime.lut_file = Some(required_file(&config.smoothie.lut_file, "Smoothie LUT")?);
            }
            if !runtime.recipe.is_file() {
                return Err("The bundled Smoothie recipe is unavailable".to_string());
            }
            Ok(runtime)
        }
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
                bound_pending_log(&mut pending);
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
    pump_scoped(reader, app, None).await
}

async fn pump_render<R>(reader: R, app: tauri::AppHandle, job_id: String) -> Vec<String>
where
    R: AsyncRead + Unpin,
{
    pump_scoped(reader, app, Some(job_id)).await
}

struct PumpTaskGuard(Vec<tokio::task::AbortHandle>);
impl Drop for PumpTaskGuard {
    fn drop(&mut self) {
        for task in &self.0 {
            task.abort();
        }
    }
}

fn emit_log(app: &tauri::AppHandle, job_id: Option<&str>, line: &str) {
    if let Some(job_id) = job_id {
        let _ = app.emit("render-log", json!({ "jobId": job_id, "line": line }));
    } else {
        let _ = app.emit("live-log", line);
    }
}

async fn pump_scoped<R>(reader: R, app: tauri::AppHandle, job_id: Option<String>) -> Vec<String>
where
    R: AsyncRead + Unpin,
{
    let mut reader = reader;
    let mut buf = [0u8; 4096];
    let mut pending = String::new();
    let mut collected = VecDeque::new();
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
                bound_pending_log(&mut pending);
                for line in &lines {
                    emit_log(&app, job_id.as_deref(), line);
                }
                for mut line in lines {
                    bound_pending_log(&mut line);
                    collected.push_back(line);
                    if collected.len() > 256 {
                        collected.pop_front();
                    }
                }
            }
            Err(_) => break,
        }
    }
    let trailing = pending.trim();
    if !trailing.is_empty() {
        emit_log(&app, job_id.as_deref(), trailing);
        collected.push_back(trailing.to_string());
        if collected.len() > 256 {
            collected.pop_front();
        }
    }
    collected.into_iter().collect()
}

fn bound_pending_log(pending: &mut String) {
    const MAX_LINE_BYTES: usize = 8192;
    if pending.len() > MAX_LINE_BYTES {
        let mut end = pending.len() - MAX_LINE_BYTES;
        while !pending.is_char_boundary(end) {
            end += 1;
        }
        pending.drain(..end);
    }
}

async fn probe_video(video_path: &str, ffprobe: &Path) -> Result<VideoInfo, String> {
    probe_video_impl(video_path, ffprobe, None).await
}

async fn probe_video_impl(
    video_path: &str,
    ffprobe: &Path,
    job: Option<&JobGuard<'_>>,
) -> Result<VideoInfo, String> {
    if !Path::new(video_path).is_file() {
        return Err("The selected video is no longer available".to_string());
    }
    let mut command = Command::new(ffprobe);
    command
        .args([
            "-v",
            "error",
            "-show_entries",
            "stream=codec_type,width,height,avg_frame_rate,r_frame_rate,duration,nb_frames,color_transfer,color_space,color_primaries,sample_aspect_ratio:stream_disposition=attached_pic:stream_side_data=rotation:stream_tags=rotate:format=duration",
            "-of",
            "json",
        ])
        .arg(video_path);

    #[cfg(target_os = "windows")]
    command.creation_flags(CREATE_NO_WINDOW);

    command
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    let child = match job {
        Some(job) => job.spawn(&mut command)?,
        None => command
            .spawn()
            .map_err(|error| format!("FFprobe could not start: {error}"))?,
    };
    let output = child
        .wait_with_output()
        .await
        .map_err(|error| format!("FFprobe failed: {error}"))?;
    if let Some(job) = job {
        job.check_cancelled()?;
    }
    if !output.status.success() {
        return Err(format!(
            "FFprobe error: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }

    let raw = String::from_utf8_lossy(&output.stdout);
    let json: Value = serde_json::from_str(&raw)
        .map_err(|error| format!("FFprobe returned invalid JSON: {error}"))?;
    parse_video_info(&json)
}

fn positive_rate(value: &str) -> Option<f64> {
    let rate = if let Some((numerator, denominator)) = value.split_once('/') {
        numerator.parse::<f64>().ok()? / denominator.parse::<f64>().ok()?
    } else {
        value.parse::<f64>().ok()?
    };
    (rate.is_finite() && rate > 0.0).then_some(rate)
}

fn parse_video_info(json: &Value) -> Result<VideoInfo, String> {
    let streams = json["streams"]
        .as_array()
        .ok_or("FFprobe did not return any streams")?;
    let stream = streams
        .iter()
        .find(|stream| {
            stream["codec_type"].as_str() == Some("video")
                && stream["disposition"]["attached_pic"].as_u64().unwrap_or(0) == 0
        })
        .ok_or("The selected file does not contain a video stream")?;
    let mut width = stream["width"]
        .as_u64()
        .and_then(|value| u32::try_from(value).ok())
        .filter(|value| *value > 0)
        .ok_or("The video width is invalid")?;
    let mut height = stream["height"]
        .as_u64()
        .and_then(|value| u32::try_from(value).ok())
        .filter(|value| *value > 0)
        .ok_or("The video height is invalid")?;
    let rotation = stream["side_data_list"]
        .as_array()
        .and_then(|entries| entries.iter().find_map(|entry| entry["rotation"].as_i64()))
        .or_else(|| {
            stream["tags"]["rotate"]
                .as_str()
                .and_then(|value| value.parse().ok())
        })
        .unwrap_or(0);
    let mut sample_aspect_ratio = stream["sample_aspect_ratio"]
        .as_str()
        .filter(|value| positive_aspect_ratio(value).is_some())
        .unwrap_or("1:1")
        .to_string();
    if rotation.rem_euclid(180) == 90 {
        std::mem::swap(&mut width, &mut height);
        if let Some((numerator, denominator)) = sample_aspect_ratio.split_once(':') {
            sample_aspect_ratio = format!("{denominator}:{numerator}");
        }
    }
    let fps = stream["avg_frame_rate"]
        .as_str()
        .and_then(positive_rate)
        .or_else(|| stream["r_frame_rate"].as_str().and_then(positive_rate))
        .ok_or("The video framerate is invalid")?;
    let duration: f64 = stream["duration"]
        .as_str()
        .and_then(|value| value.parse().ok())
        .or_else(|| {
            json["format"]["duration"]
                .as_str()
                .and_then(|value| value.parse().ok())
        })
        .filter(|value: &f64| value.is_finite() && *value > 0.0)
        .ok_or("The video duration is invalid")?;
    let reported_rate = stream["r_frame_rate"].as_str().and_then(positive_rate);
    let frame_count = stream["nb_frames"]
        .as_str()
        .and_then(|value| value.parse::<u64>().ok())
        .filter(|frames| {
            reported_rate.is_some_and(|rate| (rate - fps).abs() <= fps * 0.000001)
                && (*frames as f64 / fps - duration).abs() <= 1.0 / fps + 0.020
        });
    let has_audio = streams
        .iter()
        .any(|stream| stream["codec_type"].as_str() == Some("audio"));

    Ok(VideoInfo {
        width,
        height,
        fps,
        duration,
        has_audio,
        frame_count,
        color_transfer: stream["color_transfer"].as_str().map(str::to_owned),
        color_space: stream["color_space"].as_str().map(str::to_owned),
        color_primaries: stream["color_primaries"].as_str().map(str::to_owned),
        sample_aspect_ratio,
    })
}

const PREVIEW_FRAME_COUNT: usize = 8;
const PREVIEW_FRAME_POSITIONS: [f64; PREVIEW_FRAME_COUNT] =
    [0.06, 0.19, 0.32, 0.45, 0.58, 0.71, 0.84, 0.94];

fn preview_timestamps(duration: f64) -> [f64; PREVIEW_FRAME_COUNT] {
    let safe_duration = if duration.is_finite() && duration > 0.0 {
        duration
    } else {
        0.0
    };
    let last_frame = (safe_duration - 0.04).max(0.0);
    PREVIEW_FRAME_POSITIONS.map(|position| (safe_duration * position).min(last_frame))
}

fn preview_luminance_score(pixels: &[u8]) -> Option<f64> {
    if pixels.len() < 64 {
        return None;
    }

    let mean = pixels.iter().map(|pixel| f64::from(*pixel)).sum::<f64>() / pixels.len() as f64;
    let variance = pixels
        .iter()
        .map(|pixel| {
            let difference = f64::from(*pixel) - mean;
            difference * difference
        })
        .sum::<f64>()
        / pixels.len() as f64;
    let contrast = variance.sqrt();

    // Plain black/white frames have very little usable visual information. The
    // thresholds intentionally stay loose so a dark or bright real shot survives.
    if !(18.0..=237.0).contains(&mean) || contrast < 5.0 {
        return None;
    }

    Some(contrast - (mean - 128.0).abs() * 0.04)
}

fn png_luminance_score(image: &[u8]) -> Result<Option<f64>, String> {
    let mut decoder = png::Decoder::new(std::io::Cursor::new(image));
    decoder.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
    let mut reader = decoder
        .read_info()
        .map_err(|error| format!("Invalid preview PNG: {error}"))?;
    let mut buffer = vec![0; reader.output_buffer_size()];
    let info = reader
        .next_frame(&mut buffer)
        .map_err(|error| format!("Invalid preview PNG: {error}"))?;
    let channels = match info.color_type {
        png::ColorType::Rgb => 3,
        png::ColorType::Rgba => 4,
        png::ColorType::Grayscale => 1,
        png::ColorType::GrayscaleAlpha => 2,
        png::ColorType::Indexed => return Err("Unsupported preview PNG color type".to_string()),
    };
    // Score a small sampled grid from the same decoded image shown to the user.
    let mut gray = Vec::with_capacity(48 * 27);
    for row in 0..27usize {
        for column in 0..48usize {
            let x = (column * info.width as usize / 48).min(info.width as usize - 1);
            let y = (row * info.height as usize / 27).min(info.height as usize - 1);
            let pixel = (y * info.width as usize + x) * channels;
            gray.push(if channels < 3 {
                buffer[pixel]
            } else {
                ((299u32 * u32::from(buffer[pixel])
                    + 587u32 * u32::from(buffer[pixel + 1])
                    + 114u32 * u32::from(buffer[pixel + 2])
                    + 500)
                    / 1000) as u8
            });
        }
    }
    Ok(preview_luminance_score(&gray))
}

async fn extract_preview_png(
    ffmpeg: &Path,
    video_path: &str,
    timestamp: f64,
    blend_frames: u32,
    preview: Option<&PreviewGuard<'_>>,
) -> Result<Vec<u8>, String> {
    if !timestamp.is_finite() || timestamp < 0.0 {
        return Err("The preview timestamp is invalid".to_string());
    }
    let blend_frames = blend_frames.clamp(1, 24);
    let filter = if blend_frames > 1 {
        let weights = std::iter::repeat_n("1", blend_frames as usize)
            .collect::<Vec<_>>()
            .join(" ");
        format!(
            "tmix=frames={blend_frames}:weights='{weights}',trim=start_frame={},scale=480:270:force_original_aspect_ratio=decrease",
            blend_frames - 1
        )
    } else {
        "scale=480:270:force_original_aspect_ratio=decrease".to_string()
    };
    let mut command = Command::new(ffmpeg);
    command
        .args(["-v", "error", "-ss"])
        .arg(format!("{timestamp:.3}"))
        .arg("-i")
        .arg(video_path)
        .args(["-frames:v", "1", "-an", "-vf"])
        .arg(filter)
        .args(["-f", "image2pipe", "-vcodec", "png", "pipe:1"]);
    #[cfg(target_os = "windows")]
    command.creation_flags(CREATE_NO_WINDOW);

    command
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    let child = match preview {
        Some(preview) => preview.spawn(&mut command)?,
        None => command
            .spawn()
            .map_err(|error| format!("FFmpeg could not generate a preview image: {error}"))?,
    };
    let output = child
        .wait_with_output()
        .await
        .map_err(|error| format!("FFmpeg preview failed: {error}"))?;
    if let Some(preview) = preview {
        preview.check_cancelled()?;
    }
    if !output.status.success() || output.stdout.is_empty() {
        return Err(format!(
            "FFmpeg could not generate a preview image: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    Ok(output.stdout)
}

#[tauri::command]
async fn generate_video_preview_set(
    app: tauri::AppHandle,
    registry: tauri::State<'_, JobRegistry>,
    video_path: String,
    duration: f64,
    request_id: Option<String>,
) -> Result<VideoPreviewSet, String> {
    if !duration.is_finite() || duration <= 0.0 {
        return Err("The preview duration is invalid".to_string());
    }
    if !Path::new(&video_path).is_file() {
        return Err("The selected video is no longer available".to_string());
    }
    let config = load_config(&app)?;
    let media = media_tools(&config, &app)?;
    let preview = PreviewGuard::reserve(&registry, request_id)?;
    let metadata = fs::metadata(&video_path).map_err(|error| error.to_string())?;
    let cache_key = format!(
        "{}:{}:{:?}:{}:{}",
        fs::canonicalize(&video_path)
            .map_err(|error| error.to_string())?
            .display(),
        metadata.len(),
        metadata.modified().ok(),
        duration.to_bits(),
        media.ffmpeg.display()
    );
    if let Some((_, cached)) = registry
        .preview_cache
        .lock()
        .map_err(|_| "Preview cache is unavailable")?
        .iter()
        .find(|(key, _)| key == &cache_key)
    {
        return Ok(cached.clone());
    }
    let timestamps = preview_timestamps(duration);

    let mut usable = Vec::new();
    let mut extracted = Vec::with_capacity(PREVIEW_FRAME_COUNT);
    for (index, timestamp) in timestamps.iter().enumerate() {
        let image =
            extract_preview_png(&media.ffmpeg, &video_path, *timestamp, 1, Some(&preview)).await?;
        if let Some(score) = png_luminance_score(&image)? {
            usable.push((index, score));
        }
        extracted.push(format!(
            "data:image/png;base64,{}",
            BASE64_STANDARD.encode(image)
        ));
    }

    let fallback_index = PREVIEW_FRAME_COUNT / 2;
    let chosen_indexes: Vec<usize> = (0..PREVIEW_FRAME_COUNT)
        .map(|index| {
            usable
                .iter()
                .min_by_key(|(candidate, _)| candidate.abs_diff(index))
                .map(|(candidate, _)| *candidate)
                .unwrap_or(fallback_index)
        })
        .collect();
    let cover_index = usable
        .iter()
        .max_by(|(_, left), (_, right)| left.total_cmp(right))
        .map(|(index, _)| *index)
        .unwrap_or(fallback_index);

    let mut frames = Vec::with_capacity(PREVIEW_FRAME_COUNT);
    for index in &chosen_indexes {
        frames.push(extracted[*index].clone());
    }
    let cover_frame_index = chosen_indexes
        .iter()
        .position(|index| *index == cover_index)
        .unwrap_or(0);

    preview.check_cancelled()?;
    let result = VideoPreviewSet {
        cover: frames[cover_frame_index].clone(),
        frames,
    };
    let mut cache = registry
        .preview_cache
        .lock()
        .map_err(|_| "Preview cache is unavailable")?;
    cache.push_back((cache_key, result.clone()));
    while cache.len() > 4 {
        cache.pop_front();
    }
    Ok(result)
}

#[tauri::command]
async fn generate_video_preview_frame(
    app: tauri::AppHandle,
    registry: tauri::State<'_, JobRegistry>,
    video_path: String,
    timestamp: f64,
    blend_frames: Option<u32>,
    request_id: Option<String>,
) -> Result<String, String> {
    if !timestamp.is_finite() {
        return Err("The preview timestamp is invalid".to_string());
    }
    if !Path::new(&video_path).is_file() {
        return Err("The selected video is no longer available".to_string());
    }
    let config = load_config(&app)?;
    let media = media_tools(&config, &app)?;
    let preview = PreviewGuard::reserve(&registry, request_id)?;
    let image = extract_preview_png(
        &media.ffmpeg,
        &video_path,
        timestamp.max(0.0),
        blend_frames.unwrap_or(1),
        Some(&preview),
    )
    .await?;
    Ok(format!(
        "data:image/png;base64,{}",
        BASE64_STANDARD.encode(image)
    ))
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

fn validate_rife_options(
    mode: &str,
    factor: f64,
    crf: u32,
    preset: &str,
    precision: &str,
    encoder: &str,
) -> Result<(), String> {
    if !matches!(mode, "boost" | "slowmo") {
        return Err("Unsupported interpolation mode".to_string());
    }
    if !factor.is_finite() || factor.fract() != 0.0 || !(2.0..=10.0).contains(&factor) {
        return Err("Interpolation factor must be an integer from 2 to 10".to_string());
    }
    if crf > 51 {
        return Err("Video quality must be from 0 to 51".to_string());
    }
    if ![
        "ultrafast",
        "superfast",
        "veryfast",
        "faster",
        "fast",
        "medium",
        "slow",
        "slower",
        "veryslow",
    ]
    .contains(&preset)
    {
        return Err("Unsupported x264 preset".to_string());
    }
    if !matches!(precision, "fp32" | "fp16") {
        return Err("Unsupported RIFE precision".to_string());
    }
    validate_encoder(encoder)
}

fn validate_encoder(encoder: &str) -> Result<(), String> {
    if matches!(encoder, "libx264" | "h264_nvenc") {
        Ok(())
    } else {
        Err("Unsupported video encoder".to_string())
    }
}

fn validate_delivery_source(info: &VideoInfo) -> Result<(), String> {
    if !info.width.is_multiple_of(2) || !info.height.is_multiple_of(2) {
        return Err(
            "H.264 4:2:0 output requires even video dimensions; resize the source explicitly first"
                .to_string(),
        );
    }
    if matches!(
        info.color_transfer.as_deref(),
        Some("smpte2084" | "arib-std-b67")
    ) {
        return Err("HDR sources require an explicit tone-mapping workflow; this SDR H.264 profile does not support HDR".to_string());
    }
    Ok(())
}

fn validate_smoothie_lut_source(info: &VideoInfo, overrides: &[String]) -> Result<(), String> {
    if overrides.iter().any(|value| value == "lut;enabled;yes")
        && [
            info.color_space.as_deref(),
            info.color_transfer.as_deref(),
            info.color_primaries.as_deref(),
        ]
        .iter()
        .any(|value| *value != Some("bt709"))
    {
        return Err("LUT rendering requires an SDR source tagged BT.709 for matrix, transfer and primaries. Convert the source to BT.709 or disable LUT.".to_string());
    }
    Ok(())
}

fn preserve_smoothie_color_metadata(overrides: &mut [String], info: &VideoInfo) {
    if let Some(args) = overrides
        .iter_mut()
        .rev()
        .find(|value| value.starts_with("output;enc args;"))
    {
        let mut frame_parameters = Vec::new();
        for (flag, parameter, value) in [
            ("-colorspace", "colorspace", info.color_space.as_deref()),
            ("-color_trc", "color_trc", info.color_transfer.as_deref()),
            (
                "-color_primaries",
                "color_primaries",
                info.color_primaries.as_deref(),
            ),
        ] {
            if let Some(value) = value.filter(|value| {
                *value != "unknown"
                    && value.chars().all(|character| {
                        character.is_ascii_alphanumeric() || character == '_' || character == '-'
                    })
            }) {
                args.push_str(&format!(" {flag} {value}"));
                frame_parameters.push(format!("{parameter}={value}"));
            }
        }
        // Frame properties can otherwise replace explicit encoder color flags.
        if !frame_parameters.is_empty() {
            if let Some(position) = args.find(" -c:v") {
                args.insert_str(
                    position,
                    &format!(",setparams={}", frame_parameters.join(":")),
                );
            }
        }
    }
}

fn positive_aspect_ratio(value: &str) -> Option<f64> {
    let (numerator, denominator) = value.split_once(':').or_else(|| value.split_once('/'))?;
    let rate = numerator.parse::<f64>().ok()? / denominator.parse::<f64>().ok()?;
    (rate.is_finite() && rate > 0.0).then_some(rate)
}

fn smoothie_overrides(
    overrides: Vec<String>,
    output_fps: u32,
    encoder: &str,
    lut: Option<&Path>,
    sample_aspect_ratio: &str,
) -> Result<Vec<String>, String> {
    validate_encoder(encoder)?;
    if !(1..=480).contains(&output_fps) {
        return Err("Smoothie output framerate must be from 1 to 480".to_string());
    }
    if overrides.len() > 24 {
        return Err("Too many Smoothie parameters".to_string());
    }
    let mut result = Vec::new();
    let mut color_changed = false;
    let mut lut_enabled = false;
    for value in overrides {
        let fields: Vec<&str> = value.split(';').collect();
        if fields.len() != 3 {
            return Err("Invalid Smoothie override".to_string());
        }
        let (section, key, raw) = (fields[0], fields[1], fields[2]);
        let range = match (section, key) {
            ("frame blending", "fps") => Some((1.0, 480.0)),
            ("frame blending", "intensity") => Some((0.0, 4.0)),
            ("color grading", "brightness" | "saturation" | "contrast") => Some((0.0, 2.0)),
            ("lut", "opacity") => Some((0.0, 1.0)),
            ("color grading" | "lut", "enabled") | ("console", "borderless") => {
                if !matches!(raw, "yes" | "no") {
                    return Err(format!("Invalid {section} {key}"));
                }
                if section == "lut" {
                    lut_enabled = raw == "yes";
                }
                None
            }
            _ => return Err(format!("Unsupported Smoothie parameter: {section} {key}")),
        };
        if let Some((minimum, maximum)) = range {
            let number = raw
                .parse::<f64>()
                .map_err(|_| format!("Invalid {section} {key}"))?;
            if !number.is_finite() || !(minimum..=maximum).contains(&number) {
                return Err(format!(
                    "{section} {key} must be from {minimum} to {maximum}"
                ));
            }
            if section == "color grading" && (number - 1.0).abs() > f64::EPSILON {
                color_changed = true;
            }
        }
        result.push(value);
    }
    if lut_enabled && lut.is_none() {
        return Err("Select a valid LUT in Runtime Setup before enabling it".to_string());
    }
    if let Some(lut) = lut {
        let path = lut.to_string_lossy().replace('\\', "/");
        if path.contains(';') {
            return Err("The LUT path cannot contain a semicolon".to_string());
        }
        result.push(format!("lut;path;{path}"));
    }
    result.push(format!(
        "lut;enabled;{}",
        if lut_enabled { "yes" } else { "no" }
    ));
    result.push(format!(
        "color grading;enabled;{}",
        if color_changed { "yes" } else { "no" }
    ));
    result.push("frame blending;enabled;yes".to_string());
    result.push(format!("frame blending;fps;{output_fps}"));
    // The application exposes source-duration rendering, not recipe time warps.
    result.push("timescale;in;1.0".to_string());
    result.push("timescale;out;1.0".to_string());
    let video_args = if encoder == "h264_nvenc" {
        "-c:v h264_nvenc -preset p5 -tune hq -rc vbr -cq 18 -b:v 0"
    } else {
        "-c:v libx264 -preset medium -crf 18"
    };
    let sar = sample_aspect_ratio.replace(':', "/");
    if positive_aspect_ratio(sample_aspect_ratio).is_none() {
        return Err("The source sample aspect ratio is invalid".to_string());
    }
    result.push(format!("output;enc args;-vf setsar={sar} {video_args} -pix_fmt yuv420p -c:a aac -b:a 320k -ar 48000 -movflags +faststart"));
    Ok(result)
}

fn validate_output_info(
    info: &VideoInfo,
    source: &VideoInfo,
    duration: f64,
    fps: f64,
    frames: Option<u64>,
) -> Result<(), String> {
    if info.width != source.width || info.height != source.height {
        return Err(format!(
            "Output dimensions changed unexpectedly: {}x{} instead of {}x{}",
            info.width, info.height, source.width, source.height
        ));
    }
    if source.has_audio && !info.has_audio {
        return Err("The rendered output lost its audio stream".to_string());
    }
    let source_sar = positive_aspect_ratio(&source.sample_aspect_ratio).unwrap_or(1.0);
    let output_sar = positive_aspect_ratio(&info.sample_aspect_ratio).unwrap_or(1.0);
    if (source_sar - output_sar).abs() > 0.0001 {
        return Err("The rendered output changed the display aspect ratio".to_string());
    }
    if (info.fps - fps).abs() > fps.max(1.0) * 0.0001 {
        return Err(format!(
            "Output framerate is invalid: {} instead of {fps}",
            info.fps
        ));
    }
    let tolerance = 1.0 / fps + 0.020;
    if (info.duration - duration).abs() > tolerance {
        return Err(format!(
            "Output video is truncated or retimed: {:.6}s instead of {:.6}s",
            info.duration, duration
        ));
    }
    if let (Some(expected), Some(actual)) = (frames, info.frame_count) {
        if actual != expected {
            return Err(format!(
                "Output frame count is invalid: {actual} instead of {expected}"
            ));
        }
    }
    Ok(())
}

async fn validate_output(
    path: &Path,
    ffprobe: &Path,
    source: &VideoInfo,
    duration: f64,
    fps: f64,
    frames: Option<u64>,
    job: &JobGuard<'_>,
) -> Result<(), String> {
    ensure_nonempty_file(path, "Rendered")?;
    job.check_cancelled()?;
    let info = probe_video_impl(&path.to_string_lossy(), ffprobe, Some(job)).await?;
    validate_output_info(&info, source, duration, fps, frames)
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
                return Ok(OutputReservation {
                    output,
                    lock,
                    work_directory: None,
                    process_owner: None,
                });
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
    update_config(&app, |config| {
        config.ui = UiSettings {
            migrated: true,
            auto_render,
            rife_settings,
            smoothie_settings,
        };
    })?;
    Ok(runtime_snapshot(&app))
}

#[tauri::command]
async fn pick_runtime_path(kind: String) -> Result<Option<String>, String> {
    let selected = tokio::task::spawn_blocking(move || match kind.as_str() {
        "rife_directory" | "smoothie_root" => rfd::FileDialog::new().pick_folder(),
        "smoothie_lut" => rfd::FileDialog::new()
            .add_filter("Color LUT", &["cube"])
            .pick_file(),
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
async fn install_rife_environment(
    app: tauri::AppHandle,
    registry: tauri::State<'_, JobRegistry>,
) -> Result<RuntimeSnapshot, String> {
    let job = JobGuard::reserve(&registry, "install-rife")?;
    let configured = normalize_config(load_config(&app)?);
    if rife_runtime(&configured, &app).is_ok() {
        let _ = app.emit(
            "live-log",
            "[cia render] A configured RIFE environment is already ready.",
        );
        return Ok(runtime_snapshot(&app));
    }

    let detected = auto_detect_config();
    let mut adopted = configured.clone();
    adopted.rife = detected.rife;
    let adopted = normalize_config(adopted);
    if rife_runtime(&adopted, &app).is_ok() {
        update_config(&app, |config| {
            config.rife = adopted.rife;
        })?;
        let _ = app.emit(
            "live-log",
            "[cia render] A complete local RIFE runtime was detected and is now in use.",
        );
        return Ok(runtime_snapshot(&app));
    }

    let _ = app.emit(
        "live-log",
        "[cia render] No complete local RIFE runtime was found. Installing the optional environment...",
    );
    let bootstrap = bundled_resource(&app, "bootstrap/bootstrap-rife.ps1")
        .filter(|path| path.is_file())
        .ok_or("The bundled RIFE installer script is missing. Reinstall cia render.")?;
    let python_installer = bundled_resource(&app, "bootstrap/python-3.11.9-amd64.exe")
        .filter(|path| path.is_file())
        .ok_or("The bundled Python installer is missing. Reinstall cia render.")?;
    let root = rife_install_root(&app)?;
    fs::create_dir_all(&root)
        .map_err(|error| format!("Unable to create {}: {error}", root.display()))?;

    let mut command = Command::new("powershell.exe");
    command
        .args(["-NoProfile", "-ExecutionPolicy", "Bypass", "-File"])
        .arg(&bootstrap)
        .arg("-RuntimeRoot")
        .arg(&root)
        .arg("-PythonInstaller")
        .arg(&python_installer)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    #[cfg(target_os = "windows")]
    command.creation_flags(CREATE_NO_WINDOW);

    let mut child = job.spawn(&mut command)?;
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
    let _pumps = PumpTaskGuard(vec![output_task.abort_handle(), error_task.abort_handle()]);
    let status = child.wait().await.map_err(|error| error.to_string())?;
    if status.success() {
        job.wait_for_descendants().await?;
    } else {
        job.terminate_descendants();
    }
    let _ = output_task.await;
    let stderr_lines = error_task.await.unwrap_or_default();
    job.check_cancelled()?;
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
                "RIFE environment installation failed ({status}). Review logs for the exact step."
            ))
        } else {
            Err(format!("RIFE environment installation failed: {detail}"))
        };
    }

    let installed_rife = RifeConfig {
        python_executable: path_text(Some(root.join("venv").join("Scripts").join("python.exe"))),
        script: None,
        directory: path_text(Some(root.join("Practical-RIFE"))),
        model_file: path_text(Some(
            root.join("Practical-RIFE")
                .join("train_log")
                .join("flownet.pkl"),
        )),
    };
    let mut validation_config = load_config(&app)?;
    validation_config.rife = installed_rife.clone();
    rife_runtime(&normalize_config(validation_config), &app)?;
    update_config(&app, |config| {
        config.rife = installed_rife;
    })?;
    Ok(runtime_snapshot(&app))
}

#[tauri::command]
fn pause_render(job_id: String, registry: tauri::State<JobRegistry>) -> Result<(), String> {
    let job = running_job(&registry, &job_id)?;
    if job.paused {
        return Ok(());
    }
    #[cfg(target_os = "windows")]
    process_control::set_process_tree_paused(
        job.process_id.ok_or("The render job is still preparing")?,
        true,
    )?;
    #[cfg(not(target_os = "windows"))]
    return Err("Pause is currently supported on Windows only".to_string());

    let mut jobs = registry
        .jobs
        .lock()
        .map_err(|_| "Render job registry is unavailable")?;
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
    process_control::set_process_tree_paused(
        job.process_id.ok_or("The render job is still preparing")?,
        false,
    )?;
    #[cfg(not(target_os = "windows"))]
    return Err("Resume is currently supported on Windows only".to_string());

    let mut jobs = registry
        .jobs
        .lock()
        .map_err(|_| "Render job registry is unavailable")?;
    if let Some(entry) = jobs.get_mut(&job_id) {
        entry.paused = false;
    }
    Ok(())
}

#[tauri::command]
fn cancel_render(job_id: String, registry: tauri::State<JobRegistry>) -> Result<(), String> {
    let owner = {
        let mut jobs = registry
            .jobs
            .lock()
            .map_err(|_| "Render job registry is unavailable")?;
        let entry = jobs
            .get_mut(&job_id)
            .ok_or("The render job is no longer running")?;
        entry.cancel_requested = true;
        entry.owner.clone()
    };
    if let Some(owner) = owner {
        owner.terminate();
    }
    Ok(())
}

#[tauri::command]
#[allow(clippy::too_many_arguments)] // Tauri exposes each named frontend parameter.
async fn run_time_remap(
    app: tauri::AppHandle,
    registry: tauri::State<'_, JobRegistry>,
    job_id: String,
    video_path: String,
    mode: String,
    factor: f64,
    crf: u32,
    preset: String,
    scene_threshold: Option<f64>,
    blend_cuts: Option<u32>,
    precision: Option<String>,
    encoder: Option<String>,
) -> Result<String, String> {
    // Accept legacy fields without doing a redundant scene scan.
    let _ = (scene_threshold, blend_cuts);
    let job = JobGuard::reserve(&registry, &job_id)?;
    let precision = precision.unwrap_or_else(|| "fp32".to_string());
    let encoder = encoder.unwrap_or_else(|| "libx264".to_string());
    validate_rife_options(&mode, factor, crf, &preset, &precision, &encoder)?;
    let config = load_config(&app)?;
    let runtime = rife_runtime(&config, &app)?;
    let info = probe_video_impl(&video_path, &runtime.media.ffprobe, Some(&job)).await?;
    validate_delivery_source(&info)?;
    let mut reservation =
        reserve_output_path(&rife_output_path(&video_path, &mode, factor, info.fps)?)?;
    let work_directory = reservation.prepare_work_directory()?;
    let out_path = work_directory.join("output.mp4");

    let mut command = Command::new(&runtime.python);
    command.current_dir(strip_unc_path(&runtime.directory));
    command.env("PYTHONUNBUFFERED", "1");

    if let Some(ffmpeg_dir) = runtime.media.ffmpeg.parent() {
        let clean_ffmpeg_dir = strip_unc_path(ffmpeg_dir);
        let current_path = env::var_os("PATH").unwrap_or_default();
        let mut paths = vec![clean_ffmpeg_dir];
        paths.extend(env::split_paths(&current_path).map(|p| strip_unc_path(&p)));
        if let Ok(new_path) = env::join_paths(paths) {
            command.env("PATH", &new_path);
            command.env("Path", &new_path);
        }
    }
    command
        .arg(strip_unc_path(&runtime.script))
        .arg("--video")
        .arg(strip_unc_path(Path::new(&video_path)))
        .arg("--mode")
        .arg(&mode)
        .arg("--factor")
        .arg(factor.to_string())
        .arg("--crf")
        .arg(crf.to_string())
        .arg("--preset")
        .arg(&preset)
        .arg("--precision")
        .arg(&precision)
        .arg("--encoder")
        .arg(&encoder)
        .arg("--model-dir")
        .arg(strip_unc_path(&runtime.model_directory))
        .arg("--work-dir")
        .arg(strip_unc_path(&work_directory))
        .arg("--output")
        .arg(strip_unc_path(&out_path))
        .arg("--ffmpeg")
        .arg(strip_unc_path(&runtime.media.ffmpeg))
        .arg("--ffprobe")
        .arg(strip_unc_path(&runtime.media.ffprobe))
        .arg("--rife-dir")
        .arg(strip_unc_path(&runtime.directory))
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    #[cfg(target_os = "windows")]
    command.creation_flags(CREATE_NO_WINDOW);

    let mut child = job.spawn(&mut command)?;
    reservation.process_owner = running_job(&registry, &job_id)?.owner;
    let stdout = child.stdout.take().ok_or("Python stdout was unavailable")?;
    let stderr = child.stderr.take().ok_or("Python stderr was unavailable")?;
    let output_app = app.clone();
    let error_app = app.clone();
    let output_job_id = job_id.clone();
    let error_job_id = job_id.clone();
    let output_task =
        tokio::spawn(async move { pump_render(stdout, output_app, output_job_id).await });
    let error_task =
        tokio::spawn(async move { pump_render(stderr, error_app, error_job_id).await });
    let _pumps = PumpTaskGuard(vec![output_task.abort_handle(), error_task.abort_handle()]);
    let status = child.wait().await.map_err(|error| error.to_string())?;
    if status.success() {
        job.wait_for_descendants().await?;
    } else {
        job.terminate_descendants();
    }
    let _ = output_task.await;
    let stderr_lines = error_task.await.unwrap_or_default();
    job.check_cancelled()?;

    if status.success() {
        let output_fps = if mode == "boost" {
            info.fps * factor
        } else {
            info.fps
        };
        let output_frames = info
            .frame_count
            .and_then(|frames| frames.checked_mul(factor as u64));
        let output_duration = output_frames
            .map(|frames| frames as f64 / output_fps)
            .unwrap_or(info.duration * if mode == "slowmo" { factor } else { 1.0 });
        validate_output(
            &out_path,
            &runtime.media.ffprobe,
            &info,
            output_duration,
            output_fps,
            output_frames,
            &job,
        )
        .await?;
        job.check_cancelled()?;
        reservation.publish(&out_path)
    } else {
        let detail = stderr_lines
            .iter()
            .rev()
            .find(|line| {
                let lower = line.to_ascii_lowercase();
                lower.contains("error")
                    || lower.contains("exception")
                    || lower.contains("assert")
                    || lower.contains("failed")
            })
            .or_else(|| stderr_lines.last())
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
            .unwrap_or("Unknown failure");
        Err(format!("RIFE process failed ({status}): {detail}"))
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
    encoder: Option<String>,
) -> Result<String, String> {
    let job = JobGuard::reserve(&registry, &job_id)?;
    let config = load_config(&app)?;
    let runtime = smoothie_runtime(&config, &app)?;
    let info = probe_video_impl(&video_path, &runtime.ffprobe, Some(&job)).await?;
    validate_delivery_source(&info)?;
    let encoder = encoder.unwrap_or_else(|| "libx264".to_string());
    let mut overrides = smoothie_overrides(
        overrides,
        output_fps,
        &encoder,
        runtime.lut_file.as_deref(),
        &info.sample_aspect_ratio,
    )?;
    validate_smoothie_lut_source(&info, &overrides)?;
    preserve_smoothie_color_metadata(&mut overrides, &info);
    let mut reservation = reserve_output_path(&smoothie_output_path(&video_path, output_fps)?)?;
    let work_directory = reservation.prepare_work_directory()?;
    let out_path = work_directory.join("output.mp4");
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
        .arg("--recipe")
        .arg(&runtime.recipe)
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

    let mut child = job.spawn(&mut command)?;
    reservation.process_owner = running_job(&registry, &job_id)?.owner;
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
    let output_job_id = job_id.clone();
    let error_job_id = job_id.clone();
    let output_task =
        tokio::spawn(async move { pump_render(stdout, output_app, output_job_id).await });
    let error_task =
        tokio::spawn(async move { pump_render(stderr, error_app, error_job_id).await });
    let _pumps = PumpTaskGuard(vec![output_task.abort_handle(), error_task.abort_handle()]);
    let status = child.wait().await.map_err(|error| error.to_string())?;
    if status.success() {
        job.wait_for_descendants().await?;
    } else {
        job.terminate_descendants();
    }
    let _ = output_task.await;
    let stderr_lines = error_task.await.unwrap_or_default();
    job.check_cancelled()?;

    if status.success() {
        validate_output(
            &out_path,
            &runtime.ffprobe,
            &info,
            info.duration,
            f64::from(output_fps),
            None,
            &job,
        )
        .await?;
        job.check_cancelled()?;
        reservation.publish(&out_path)
    } else {
        let detail = stderr_lines
            .last()
            .map(String::as_str)
            .unwrap_or("Review the render log");
        Err(format!("smoothie-rs process failed ({status}): {detail}"))
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
                if let (Some(window), Some(icon)) =
                    (app.get_webview_window("main"), app.default_window_icon())
                {
                    let _ = window.set_icon(icon.clone());
                }
            }
            Ok(())
        })
        .manage(JobRegistry::default())
        .on_window_event(|window, event| {
            if matches!(
                event,
                tauri::WindowEvent::CloseRequested { .. } | tauri::WindowEvent::Destroyed
            ) {
                window.app_handle().state::<JobRegistry>().shutdown();
            }
        })
        .invoke_handler(tauri::generate_handler![
            get_runtime_snapshot,
            get_app_version,
            save_runtime_config,
            save_ui_preferences,
            pick_runtime_path,
            analyze_video,
            generate_video_preview_set,
            generate_video_preview_frame,
            cancel_video_previews,
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
        .build(tauri::generate_context!())
        .expect("error while building cia render")
        .run(|app, event| {
            if matches!(
                event,
                tauri::RunEvent::ExitRequested { .. } | tauri::RunEvent::Exit
            ) {
                app.state::<JobRegistry>().shutdown();
            }
        });
}

#[cfg(test)]
mod tests {
    use super::{
        preview_luminance_score, preview_timestamps, reserve_output_path, rife_output_path,
        smoothie_output_path, PREVIEW_FRAME_COUNT,
    };

    #[test]
    fn preview_selection_rejects_blank_frames_but_keeps_a_real_scene() {
        assert!(preview_luminance_score(&vec![0; 48 * 27]).is_none());
        assert!(preview_luminance_score(&vec![255; 48 * 27]).is_none());

        let scene: Vec<u8> = (0..(48 * 27))
            .map(|index| if index % 2 == 0 { 64 } else { 192 })
            .collect();
        assert!(preview_luminance_score(&scene).is_some());
    }

    #[test]
    fn preview_selection_always_produces_eight_timeline_positions() {
        let timestamps = preview_timestamps(20.0);
        assert_eq!(timestamps.len(), PREVIEW_FRAME_COUNT);
        assert!(timestamps.windows(2).all(|pair| pair[0] <= pair[1]));
        assert!(timestamps[0] > 0.0);
        assert!(timestamps[PREVIEW_FRAME_COUNT - 1] < 20.0);
    }

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
    fn job_owner_terminates_a_paused_parent_and_descendant() {
        use tokio::io::{AsyncBufReadExt, BufReader};
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        runtime.block_on(async {
            let mut command = super::Command::new("powershell.exe");
            command.args(["-NoProfile", "-Command", "$renderChild = Start-Process -FilePath 'powershell.exe' -ArgumentList '-NoProfile','-Command','Start-Sleep -Seconds 20' -WindowStyle Hidden -PassThru; Write-Output $renderChild.Id; Start-Sleep -Seconds 20"])
                .stdout(std::process::Stdio::piped()).stderr(std::process::Stdio::null());
            let (mut child, owner) = super::process_control::spawn_owned(&mut command).unwrap();
            let mut reader = BufReader::new(child.stdout.take().unwrap());
            let mut line = String::new();
            reader.read_line(&mut line).await.unwrap();
            let descendant: u32 = line.trim().parse().expect("descendant PID");
            super::process_control::set_process_tree_paused(child.id().unwrap(), true).unwrap();
            owner.terminate();
            assert!(!child.wait().await.unwrap().success());
            assert!(!process_is_running(descendant), "owned descendant must be terminated even while paused");
        });
    }

    #[cfg(target_os = "windows")]
    fn process_is_running(process_id: u32) -> bool {
        use std::ffi::c_void;
        #[link(name = "kernel32")]
        extern "system" {
            fn OpenProcess(access: u32, inherit: i32, pid: u32) -> *mut c_void;
            fn GetExitCodeProcess(handle: *mut c_void, code: *mut u32) -> i32;
            fn CloseHandle(handle: *mut c_void) -> i32;
        }
        unsafe {
            let handle = OpenProcess(0x1000, 0, process_id);
            if handle.is_null() {
                return false;
            }
            let mut code = 0;
            let running = GetExitCodeProcess(handle, &mut code) != 0 && code == 259;
            CloseHandle(handle);
            running
        }
    }

    #[test]
    fn render_job_reservation_rejects_empty_and_concurrent_ids_and_cleans_on_drop() {
        let registry = super::JobRegistry::default();
        assert!(super::JobGuard::reserve(&registry, " ").is_err());
        let job = super::JobGuard::reserve(&registry, "first").unwrap();
        assert!(super::JobGuard::reserve(&registry, "first").is_err());
        assert!(super::JobGuard::reserve(&registry, "second").is_err());
        drop(job);
        assert!(super::JobGuard::reserve(&registry, "second").is_ok());
    }

    #[test]
    fn cancellation_during_preparation_prevents_a_process_spawn() {
        let registry = super::JobRegistry::default();
        let job = super::JobGuard::reserve(&registry, "preparing").unwrap();
        registry.shutdown();
        assert!(job.check_cancelled().is_err());
        assert!(job
            .spawn(&mut super::Command::new("this-program-must-never-run"))
            .unwrap_err()
            .contains("CIA_RENDER_CANCELLED"));
    }

    #[test]
    fn invalid_media_and_invalid_execution_options_are_rejected() {
        assert!(super::parse_video_info(
            &serde_json::json!({"streams": [{"codec_type": "audio"}]})
        )
        .is_err());
        assert!(super::parse_video_info(&serde_json::json!({"streams": [{"codec_type": "video", "width": 1920, "height": 1080, "avg_frame_rate": "0/0", "duration": "1"}]})).is_err());
        for factor in [f64::NAN, 1.0, 2.5, 11.0] {
            assert!(
                super::validate_rife_options("boost", factor, 18, "medium", "fp32", "libx264")
                    .is_err()
            );
        }
        assert!(
            super::validate_rife_options("boost", 2.0, 52, "medium", "fp32", "libx264").is_err()
        );
        assert!(
            super::validate_rife_options("boost", 2.0, 18, "medium", "fp16", "h264_nvenc").is_ok()
        );
    }

    #[test]
    fn a_single_probe_preserves_fractional_rate_audio_and_rotation() {
        let info = super::parse_video_info(&serde_json::json!({"streams": [{"codec_type": "video", "width": 1920, "height": 1080, "avg_frame_rate": "30000/1001", "r_frame_rate": "30/1", "duration": "1.001", "nb_frames": "30", "sample_aspect_ratio": "4:3", "side_data_list": [{"rotation": 90}]}, {"codec_type":"audio"}]})).unwrap();
        assert_eq!((info.width, info.height), (1080, 1920));
        assert!((info.fps - 30000.0 / 1001.0).abs() < 0.000001);
        assert_eq!(info.sample_aspect_ratio, "3:4");
        assert!(info.has_audio);
        assert_eq!(
            info.frame_count, None,
            "a conflicting nominal rate cannot prove CFR frame count"
        );
    }

    #[test]
    fn smoothie_delivery_preserves_source_geometry_and_applies_color_lut_encoder() {
        let overrides = super::smoothie_overrides(
            vec![
                "color grading;brightness;1.2".to_string(),
                "lut;enabled;yes".to_string(),
            ],
            30,
            "h264_nvenc",
            Some(std::path::Path::new(r"C:\media\grade.cube")),
            "4:3",
        )
        .unwrap();
        assert!(overrides
            .iter()
            .any(|value| value == "color grading;enabled;yes"));
        assert!(overrides
            .iter()
            .any(|value| value == "lut;path;C:/media/grade.cube"));
        let args = overrides
            .iter()
            .find(|value| value.starts_with("output;enc args;"))
            .unwrap();
        assert!(args.contains("setsar=4/3"));
        assert!(args.contains("h264_nvenc"));
        assert!(!args.contains("scale="));
        assert!(!args.contains("rubberband"));
        assert!(super::smoothie_overrides(
            vec!["lut;enabled;yes".to_string()],
            30,
            "libx264",
            None,
            "1:1"
        )
        .is_err());
        assert!(super::smoothie_overrides(
            vec!["color grading;brightness;NaN".to_string()],
            30,
            "libx264",
            None,
            "1:1"
        )
        .is_err());
    }

    #[test]
    fn output_validation_rejects_truncation_missing_audio_wrong_geometry_and_cadence() {
        let source = super::parse_video_info(&serde_json::json!({"streams": [{"codec_type": "video", "width": 320, "height": 200, "avg_frame_rate": "30/1", "r_frame_rate": "30/1", "duration": "1", "nb_frames": "30", "sample_aspect_ratio": "4:3"}, {"codec_type": "audio"}]})).unwrap();
        assert!(super::validate_output_info(&source, &source, 1.0, 30.0, Some(30)).is_ok());
        let mut invalid = source.clone();
        invalid.duration = 0.8;
        assert!(super::validate_output_info(&invalid, &source, 1.0, 30.0, Some(30)).is_err());
        invalid = source.clone();
        invalid.has_audio = false;
        assert!(super::validate_output_info(&invalid, &source, 1.0, 30.0, Some(30)).is_err());
        invalid = source.clone();
        invalid.width = 1920;
        assert!(super::validate_output_info(&invalid, &source, 1.0, 30.0, Some(30)).is_err());
        invalid = source.clone();
        invalid.sample_aspect_ratio = "1:1".to_string();
        assert!(super::validate_output_info(&invalid, &source, 1.0, 30.0, Some(30)).is_err());
        invalid = source.clone();
        invalid.fps = 60.0;
        assert!(super::validate_output_info(&invalid, &source, 1.0, 30.0, Some(30)).is_err());
        invalid = source.clone();
        invalid.frame_count = Some(29);
        assert!(super::validate_output_info(&invalid, &source, 1.0, 30.0, Some(30)).is_err());
        assert!(
            super::validate_smoothie_lut_source(&source, &["lut;enabled;yes".to_string()]).is_err()
        );
        assert!(
            super::validate_smoothie_lut_source(&source, &["lut;enabled;no".to_string()]).is_ok()
        );
    }

    #[test]
    fn publishing_and_drop_never_touch_a_late_preexisting_destination() {
        let directory = std::env::temp_dir().join(format!(
            "cia-render-publish-test-{}-{}",
            std::process::id(),
            super::TEMPORARY_ID.fetch_add(1, super::Ordering::Relaxed)
        ));
        std::fs::create_dir(&directory).unwrap();
        let preferred = directory.join("final.mp4");
        let mut reservation = super::reserve_output_path(&preferred).unwrap();
        let work = reservation.prepare_work_directory().unwrap();
        let output = work.join("output.mp4");
        std::fs::write(&output, b"rendered").unwrap();
        std::fs::write(&preferred, b"external file").unwrap();
        assert!(reservation.publish(&output).is_err());
        drop(reservation);
        assert_eq!(std::fs::read(&preferred).unwrap(), b"external file");
        assert!(!work.exists());
        std::fs::remove_dir_all(directory).unwrap();
    }

    #[cfg(target_os = "windows")]
    #[test]
    #[ignore = "requires the local bundled FFmpeg and Smoothie runtime"]
    fn bundled_smoothie_pipeline_preserves_geometry_audio_and_publishes_safely() {
        let resource_root =
            std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("resources/runtime");
        let ffmpeg = resource_root.join("ffmpeg/ffmpeg.exe");
        let ffprobe = resource_root.join("ffmpeg/ffprobe.exe");
        let smoothie_root = resource_root.join("smoothie");
        let smoothie = smoothie_root.join("bin/smoothie-rs.exe");
        assert!(
            ffmpeg.is_file() && ffprobe.is_file() && smoothie.is_file(),
            "prepare the local runtime before this opt-in integration test"
        );
        let directory = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("target")
            .join(format!(
                "pipeline-integration-{}-{}",
                std::process::id(),
                super::TEMPORARY_ID.fetch_add(1, super::Ordering::Relaxed)
            ));
        std::fs::create_dir(&directory).unwrap();
        struct TestDirectory(std::path::PathBuf);
        impl Drop for TestDirectory {
            fn drop(&mut self) {
                let _ = std::fs::remove_dir_all(&self.0);
            }
        }
        let _cleanup = TestDirectory(directory.clone());
        let source = directory.join("source [v1].mp4");
        let untagged_source = directory.join("untagged.mp4");
        let lut = directory.join("invert.cube");
        std::fs::write(&lut, "TITLE \"Invert red\"\nLUT_3D_SIZE 2\nDOMAIN_MIN 0 0 0\nDOMAIN_MAX 1 1 1\n1 0 0\n0 0 0\n1 1 0\n0 1 0\n1 0 1\n0 0 1\n1 1 1\n0 1 1\n").unwrap();
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        runtime.block_on(async {
            let generated = super::Command::new(&ffmpeg).args(["-hide_banner", "-v", "error", "-f", "lavfi", "-i", "testsrc2=size=320x200:rate=30000/1001", "-f", "lavfi", "-i", "sine=frequency=440:sample_rate=48000", "-t", "1.001", "-vf", "setsar=4/3", "-c:v", "libx264", "-pix_fmt", "yuv420p", "-c:a", "aac", "-shortest", "-n"]).arg(&untagged_source).output().await.unwrap();
            assert!(generated.status.success(), "{}", String::from_utf8_lossy(&generated.stderr));
            let tagged = super::Command::new(&ffmpeg).args(["-hide_banner", "-v", "error", "-i"]).arg(&untagged_source).args(["-vf", "setparams=colorspace=bt709:color_primaries=bt709:color_trc=bt709", "-c:v", "libx264", "-pix_fmt", "yuv420p", "-c:a", "copy", "-colorspace", "bt709", "-color_trc", "bt709", "-color_primaries", "bt709", "-n"]).arg(&source).output().await.unwrap();
            assert!(tagged.status.success(), "{}", String::from_utf8_lossy(&tagged.stderr));
            let original = std::fs::read(&source).unwrap();
            let info = super::probe_video(&source.to_string_lossy(), &ffprobe).await.unwrap();
            assert_eq!((info.width, info.height), (320, 200));
            assert!(info.has_audio);
            assert_eq!(info.sample_aspect_ratio, "4:3");
            assert_eq!(info.color_space.as_deref(), Some("bt709"));
            assert_eq!(info.color_transfer.as_deref(), Some("bt709"));
            assert_eq!(info.color_primaries.as_deref(), Some("bt709"));
            let registry = super::JobRegistry::default();
            let preview = super::PreviewGuard::reserve(&registry, Some("integration-preview".to_string())).unwrap();
            for timestamp in super::preview_timestamps(info.duration) {
                let image = super::extract_preview_png(&ffmpeg, &source.to_string_lossy(), timestamp, 1, Some(&preview)).await.unwrap();
                assert!(super::png_luminance_score(&image).unwrap().is_some());
            }
            drop(preview);
            let preferred = directory.join("final.mp4");
            std::fs::write(&preferred, b"existing final must be kept").unwrap();
            let mut pixel_checksums = Vec::new();
            for (case, input, brightness, lut_enabled) in [("neutral", &source, "1.0", false), ("grade", &source, "1.2", false), ("grade-lut", &source, "1.2", true), ("untagged-grade", &untagged_source, "1.2", false)] {
                let source_info = super::probe_video(&input.to_string_lossy(), &ffprobe).await.unwrap();
                let job = super::JobGuard::reserve(&registry, case).unwrap();
                let mut reservation = super::reserve_output_path(&preferred).unwrap();
                let work = reservation.prepare_work_directory().unwrap();
                let output = work.join("output.mp4");
                let mut overrides = super::smoothie_overrides(vec![format!("color grading;brightness;{brightness}"), "color grading;saturation;1.0".to_string(), "color grading;contrast;1.0".to_string(), format!("lut;enabled;{}", if lut_enabled { "yes" } else { "no" }), "lut;opacity;1.0".to_string(), "frame blending;intensity;1.0".to_string()], 30, "libx264", Some(&lut), &source_info.sample_aspect_ratio).unwrap();
                super::validate_smoothie_lut_source(&source_info, &overrides).unwrap();
                super::preserve_smoothie_color_metadata(&mut overrides, &source_info);
                let inherited_path = std::env::var_os("PATH").unwrap_or_default();
                let scoped_path = std::env::join_paths(std::iter::once(ffmpeg.parent().unwrap().to_path_buf()).chain(std::env::split_paths(&inherited_path))).unwrap();
                let mut command = super::Command::new(&smoothie);
                command.current_dir(&smoothie_root).env("PATH", scoped_path).arg("-i").arg(input).arg("-o").arg(&output).arg("--recipe").arg(smoothie_root.join("recipe.ini")).arg("--progress").arg("--override").args(&overrides).stdout(std::process::Stdio::piped()).stderr(std::process::Stdio::piped());
                let child = job.spawn(&mut command).unwrap();
                reservation.process_owner = super::running_job(&registry, case).unwrap().owner;
                let result = child.wait_with_output().await.unwrap();
                assert!(result.status.success(), "{case}: stdout: {}\nstderr: {}", String::from_utf8_lossy(&result.stdout), String::from_utf8_lossy(&result.stderr));
                job.wait_for_descendants().await.unwrap();
                super::validate_output(&output, &ffprobe, &source_info, source_info.duration, 30.0, Some(30), &job).await.unwrap_or_else(|error| panic!("{case}: {error}\nstdout: {}\nstderr: {}", String::from_utf8_lossy(&result.stdout), String::from_utf8_lossy(&result.stderr)));
                let output_info = super::probe_video_impl(&output.to_string_lossy(), &ffprobe, Some(&job)).await.unwrap();
                assert_eq!(output_info.sample_aspect_ratio, "4:3");
                if input == &source { assert_eq!(output_info.color_space.as_deref(), Some("bt709")); assert_eq!(output_info.color_transfer.as_deref(), Some("bt709")); assert_eq!(output_info.color_primaries.as_deref(), Some("bt709")); }
                let hash = super::Command::new(&ffmpeg).args(["-hide_banner", "-v", "error", "-i"]).arg(&output).args(["-frames:v", "1", "-an", "-vf", "format=rgb24", "-f", "hash", "-hash", "sha256", "pipe:1"]).output().await.unwrap();
                assert!(hash.status.success());
                pixel_checksums.push(hash.stdout);
                let published = reservation.publish(&output).unwrap();
                assert_ne!(std::path::Path::new(&published), preferred);
                assert!(std::path::Path::new(&published).is_file());
                drop(job);
                drop(reservation);
                assert!(!work.exists());
                assert!(registry.jobs.lock().unwrap().is_empty());
            }
            assert_ne!(pixel_checksums[0], pixel_checksums[1], "color grading must change actual decoded pixels");
            assert_ne!(pixel_checksums[1], pixel_checksums[2], "LUT must change actual decoded pixels");
            let untagged_info = super::probe_video(&untagged_source.to_string_lossy(), &ffprobe).await.unwrap();
            assert!(super::validate_smoothie_lut_source(&untagged_info, &["lut;enabled;yes".to_string()]).unwrap_err().contains("BT.709"));
            assert_eq!(std::fs::read(&source).unwrap(), original);
            assert_eq!(std::fs::read(&preferred).unwrap(), b"existing final must be kept");
            println!("Validated neutral, graded, graded+LUT and untagged graded renders: 320x200, SAR4:3, fractional source FPS, output30frames@30FPS, audio, BT709 tags, different decoded pixel checksums, eight PNG previews, source/preexisting destination preserved and cleanup");
        });
    }
}
