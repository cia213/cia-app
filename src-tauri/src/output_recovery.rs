//! Recover only locks whose recorded Windows process owner has terminated.
//!
//! A journal is evidence for one file, never authority to delete a directory.
//! Legacy locks, changed files, unsupported filesystems and inaccessible owners
//! remain untouched. Temporary work directories are deliberately not removed.

#[derive(Debug, Default)]
pub struct RecoveryReport {
    pub recovered: usize,
    pub retained: usize,
}

#[cfg(target_os = "windows")]
mod platform {
    use super::RecoveryReport;
    use serde::{Deserialize, Serialize};
    use std::ffi::c_void;
    use std::fs::{self, File, OpenOptions};
    use std::io::{self, Read, Write};
    use std::mem::size_of;
    use std::os::windows::fs::{MetadataExt, OpenOptionsExt};
    use std::os::windows::io::AsRawHandle;
    use std::path::{Component, Path, PathBuf};

    const REPARSE_POINT: u32 = 0x400;
    const MAX_JOURNAL_BYTES: u64 = 16 * 1024;
    const MAX_LOCK_BYTES: u64 = 1024;
    const MAX_JOURNALS: usize = 1024;
    type Handle = *mut c_void;

    #[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
    #[serde(deny_unknown_fields)]
    struct FileIdentity {
        volume: u64,
        id: [u8; 16],
        created: i64,
    }

    #[derive(Clone, Copy, Debug, Serialize, Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Owner {
        pid: u32,
        created: u64,
    }

    #[derive(Clone, Debug, Serialize, Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Record {
        schema_version: u32,
        nonce: String,
        owner: Owner,
        output: PathBuf,
        lock: PathBuf,
        identity: FileIdentity,
        lock_bytes: Vec<u8>,
    }

    /// Retain this token alongside an output reservation. Call `finish` after
    /// closing the original lock handle and stopping all owned engine processes.
    pub struct Journal {
        path: PathBuf,
        record: Record,
    }

    impl Journal {
        /// Remove only the unchanged file identified by this reservation.
        /// Failure preserves the journal so a later startup can inspect it.
        pub fn finish(&self) -> Result<(), String> {
            delete_matching_lock(&self.record).map_err(|error| error.to_string())?;
            remove_matching_journal(&self.path, &self.record).map_err(|error| error.to_string())
        }
    }

    /// The lock must be the empty `create_new` file that reserves `output`.
    /// Writes and flushes ownership evidence before publishing its journal.
    pub fn register(
        recovery_root: &Path,
        lock_file: &mut File,
        output: &Path,
        lock: &Path,
    ) -> Result<Journal, String> {
        let owner = Owner {
            pid: std::process::id(),
            created: process_creation(unsafe { GetCurrentProcess() })
                .map_err(|error| error.to_string())?,
        };
        register_for_owner(recovery_root, lock_file, output, lock, owner)
            .map_err(|error| format!("Unable to journal output reservation: {error}"))
    }

    fn register_for_owner(
        recovery_root: &Path,
        lock_file: &mut File,
        output: &Path,
        lock: &Path,
        owner: Owner,
    ) -> io::Result<Journal> {
        if lock_file.metadata()?.len() != 0 {
            return Err(invalid("The reservation lock was not freshly created"));
        }
        reject_reparse_paths(lock)?;
        let identity = file_identity(lock_file)?;
        let lock = fs::canonicalize(lock)?;
        let parent = lock
            .parent()
            .ok_or_else(|| invalid("Missing lock parent"))?;
        let output_parent = output
            .parent()
            .ok_or_else(|| invalid("Missing output parent"))?;
        if fs::canonicalize(output_parent)? != parent {
            return Err(invalid(
                "The output and reservation lock have different parents",
            ));
        }
        let output = parent.join(
            output
                .file_name()
                .ok_or_else(|| invalid("Missing output filename"))?,
        );
        if !lock_matches_output(&lock, &output) {
            return Err(invalid("Unexpected reservation lock filename"));
        }
        reject_reparse_ancestors(recovery_root, true)?;
        fs::create_dir_all(recovery_root)?;
        reject_reparse_paths(recovery_root)?;
        let recovery_root = fs::canonicalize(recovery_root)?;
        let nonce = random_nonce()?;
        let lock_bytes = format!(
            "CIA_RENDER_OUTPUT_LOCK_V1\nnonce={nonce}\npid={}\ncreated={}\n",
            owner.pid, owner.created
        )
        .into_bytes();
        lock_file.write_all(&lock_bytes)?;
        lock_file.sync_all()?;
        // A path replaced between creation and registration must not become
        // evidence for the original handle, even if its filename is identical.
        let path_file = OpenOptions::new()
            .read(true)
            .share_mode(0x1 | 0x2 | 0x4)
            .custom_flags(0x0020_0000)
            .open(&lock)?;
        if file_identity(&path_file)? != identity {
            return Err(invalid("The reservation lock changed during registration"));
        }
        drop(path_file);
        let record = Record {
            schema_version: 1,
            nonce: nonce.clone(),
            owner,
            output,
            lock,
            identity,
            lock_bytes,
        };
        let path = recovery_root.join(format!("lock-{nonce}.json"));
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)?;
        let bytes = serde_json::to_vec(&record)?;
        file.write_all(&bytes)?;
        file.sync_all()?;
        Ok(Journal { path, record })
    }

    /// Bound startup work and preserve any file that cannot be proven safe.
    /// Do not infer ownership from a lock filename or its age.
    pub fn recover_stale(recovery_root: &Path) -> RecoveryReport {
        let mut report = RecoveryReport::default();
        if reject_reparse_paths(recovery_root).is_err() {
            return report;
        }
        let Ok(entries) = fs::read_dir(recovery_root) else {
            return report;
        };
        for entry in entries.take(MAX_JOURNALS) {
            let Ok(entry) = entry else {
                report.retained += 1;
                continue;
            };
            let path = entry.path();
            let result = read_record(&path).and_then(|record| {
                if !owner_terminated(record.owner)? {
                    return Err(invalid("The reservation owner is still running"));
                }
                delete_matching_lock(&record)?;
                remove_matching_journal(&path, &record)
            });
            if result.is_ok() {
                report.recovered += 1;
            } else {
                report.retained += 1;
            }
        }
        report
    }

    fn read_record(path: &Path) -> io::Result<Record> {
        reject_reparse_paths(path)?;
        let mut file = open_exclusive(path)?;
        let _ = file_identity(&file)?;
        let mut bytes = Vec::new();
        Read::by_ref(&mut file)
            .take(MAX_JOURNAL_BYTES + 1)
            .read_to_end(&mut bytes)?;
        if bytes.len() as u64 > MAX_JOURNAL_BYTES {
            return Err(invalid("Recovery journal exceeds its size limit"));
        }
        let record: Record = serde_json::from_slice(&bytes)?;
        validate_record(path, &record)?;
        Ok(record)
    }

    fn validate_record(path: &Path, record: &Record) -> io::Result<()> {
        if record.schema_version != 1
            || record.owner.pid == 0
            || record.owner.created == 0
            || record.nonce.len() != 32
            || !record.nonce.bytes().all(|b| b.is_ascii_hexdigit())
            || path.file_name().and_then(|value| value.to_str())
                != Some(format!("lock-{}.json", record.nonce).as_str())
            || !lock_matches_output(&record.lock, &record.output)
            || record.lock_bytes.len() as u64 > MAX_LOCK_BYTES
            || record.lock_bytes
                != format!(
                    "CIA_RENDER_OUTPUT_LOCK_V1\nnonce={}\npid={}\ncreated={}\n",
                    record.nonce, record.owner.pid, record.owner.created
                )
                .as_bytes()
        {
            return Err(invalid("Invalid recovery journal ownership evidence"));
        }
        Ok(())
    }

    fn lock_matches_output(lock: &Path, output: &Path) -> bool {
        lock.is_absolute()
            && output.is_absolute()
            && lock.parent() == output.parent()
            && !lock
                .components()
                .any(|part| matches!(part, Component::ParentDir))
            && !output
                .components()
                .any(|part| matches!(part, Component::ParentDir))
            && output
                .file_name()
                .and_then(|value| value.to_str())
                .is_some_and(|name| {
                    lock.file_name().and_then(|value| value.to_str())
                        == Some(format!(".{name}.cia-render.lock").as_str())
                })
    }

    fn delete_matching_lock(record: &Record) -> io::Result<()> {
        reject_reparse_paths(&record.lock)?;
        let mut file = open_exclusive(&record.lock)?;
        if file_identity(&file)? != record.identity {
            return Err(invalid("The reservation lock file identity changed"));
        }
        let mut bytes = Vec::new();
        Read::by_ref(&mut file)
            .take(MAX_LOCK_BYTES + 1)
            .read_to_end(&mut bytes)?;
        if bytes != record.lock_bytes {
            return Err(invalid("The reservation lock content changed"));
        }
        delete_handle(&file)
    }

    fn remove_matching_journal(path: &Path, record: &Record) -> io::Result<()> {
        reject_reparse_paths(path)?;
        let mut file = open_exclusive(path)?;
        let _ = file_identity(&file)?;
        let mut bytes = Vec::new();
        Read::by_ref(&mut file)
            .take(MAX_JOURNAL_BYTES + 1)
            .read_to_end(&mut bytes)?;
        let current: Record = serde_json::from_slice(&bytes)?;
        if serde_json::to_vec(&current)? != serde_json::to_vec(record)? {
            return Err(invalid("The recovery journal changed"));
        }
        delete_handle(&file)
    }

    fn reject_reparse_paths(path: &Path) -> io::Result<()> {
        reject_reparse_ancestors(path, false)
    }

    fn reject_reparse_ancestors(path: &Path, allow_missing: bool) -> io::Result<()> {
        if !path.is_absolute() {
            return Err(invalid("Recovery paths must be absolute"));
        }
        let mut part = PathBuf::new();
        for component in path.components() {
            if matches!(component, Component::ParentDir | Component::CurDir) {
                return Err(invalid("Recovery paths must be normalized"));
            }
            part.push(component.as_os_str());
            if matches!(component, Component::Prefix(_)) {
                continue;
            }
            let metadata = match fs::symlink_metadata(&part) {
                Ok(metadata) => metadata,
                Err(error) if allow_missing && error.kind() == io::ErrorKind::NotFound => break,
                Err(error) => return Err(error),
            };
            if metadata.file_attributes() & REPARSE_POINT != 0 || metadata.is_symlink() {
                return Err(invalid("Recovery paths must not contain reparse points"));
            }
        }
        Ok(())
    }

    fn open_exclusive(path: &Path) -> io::Result<File> {
        OpenOptions::new()
            .access_mode(0x8000_0000 | 0x0001_0000) // GENERIC_READ | DELETE
            .share_mode(0x1) // No writer or path replacement while checking/deleting.
            .custom_flags(0x0020_0000 | 0x0200_0000) // OPEN_REPARSE_POINT | BACKUP_SEMANTICS
            .open(path)
    }

    #[repr(C)]
    #[derive(Default)]
    struct IdInfo {
        volume: u64,
        id: [u8; 16],
    }

    #[repr(C)]
    #[derive(Default)]
    struct BasicInfo {
        created: i64,
        accessed: i64,
        written: i64,
        changed: i64,
        attributes: u32,
    }

    #[repr(C)]
    #[derive(Default)]
    struct StandardInfo {
        allocated: i64,
        size: i64,
        links: u32,
        delete_pending: u8,
        directory: u8,
    }

    fn file_identity(file: &File) -> io::Result<FileIdentity> {
        let mut id = IdInfo::default();
        let mut basic = BasicInfo::default();
        let mut standard = StandardInfo::default();
        let handle = file.as_raw_handle();
        for (class, information, length) in [
            (
                18,
                (&mut id as *mut IdInfo).cast::<c_void>(),
                size_of::<IdInfo>(),
            ),
            (
                0,
                (&mut basic as *mut BasicInfo).cast::<c_void>(),
                size_of::<BasicInfo>(),
            ),
            (
                1,
                (&mut standard as *mut StandardInfo).cast::<c_void>(),
                size_of::<StandardInfo>(),
            ),
        ] {
            if unsafe { GetFileInformationByHandleEx(handle, class, information, length as u32) }
                == 0
            {
                return Err(io::Error::last_os_error());
            }
        }
        if basic.attributes & REPARSE_POINT != 0
            || standard.directory != 0
            || standard.links != 1
            || basic.created <= 0
        {
            return Err(invalid(
                "The reservation must be one regular non-reparse file",
            ));
        }
        Ok(FileIdentity {
            volume: id.volume,
            id: id.id,
            created: basic.created,
        })
    }

    fn delete_handle(file: &File) -> io::Result<()> {
        // FILE_DISPOSITION_INFO contains a one-byte BOOLEAN.
        let delete: u8 = 1;
        if unsafe {
            SetFileInformationByHandle(file.as_raw_handle(), 4, (&delete as *const u8).cast(), 1)
        } == 0
        {
            return Err(io::Error::last_os_error());
        }
        Ok(())
    }

    #[repr(C)]
    #[derive(Default)]
    struct FileTime {
        low: u32,
        high: u32,
    }

    fn process_creation(handle: Handle) -> io::Result<u64> {
        let mut created = FileTime::default();
        let mut exited = FileTime::default();
        let mut kernel = FileTime::default();
        let mut user = FileTime::default();
        if unsafe { GetProcessTimes(handle, &mut created, &mut exited, &mut kernel, &mut user) }
            == 0
        {
            return Err(io::Error::last_os_error());
        }
        Ok((u64::from(created.high) << 32) | u64::from(created.low))
    }

    fn owner_terminated(owner: Owner) -> io::Result<bool> {
        let handle = unsafe { OpenProcess(0x0010_0000 | 0x1000, 0, owner.pid) };
        if handle.is_null() {
            let error = io::Error::last_os_error();
            return if error.raw_os_error() == Some(87) && owner.pid != 0 {
                Ok(true) // Positive process identifier is absent; access failures are not proof.
            } else {
                Err(error)
            };
        }
        let result = process_creation(handle).and_then(|created| {
            if created != owner.created {
                return Ok(true); // PID reused: this kernel object cannot be the recorded owner.
            }
            match unsafe { WaitForSingleObject(handle, 0) } {
                0 => Ok(true),
                258 => Ok(false),
                _ => Err(io::Error::last_os_error()),
            }
        });
        unsafe { CloseHandle(handle) };
        result
    }

    fn random_nonce() -> io::Result<String> {
        let mut bytes = [0u8; 16];
        if unsafe { BCryptGenRandom(std::ptr::null_mut(), bytes.as_mut_ptr(), 16, 2) } < 0 {
            return Err(invalid("Unable to generate reservation ownership nonce"));
        }
        Ok(bytes.iter().map(|byte| format!("{byte:02x}")).collect())
    }

    fn invalid(message: &str) -> io::Error {
        io::Error::new(io::ErrorKind::InvalidData, message)
    }

    #[link(name = "kernel32")]
    extern "system" {
        fn GetCurrentProcess() -> Handle;
        fn GetProcessTimes(
            handle: Handle,
            created: *mut FileTime,
            exited: *mut FileTime,
            kernel: *mut FileTime,
            user: *mut FileTime,
        ) -> i32;
        fn OpenProcess(access: u32, inherit: i32, pid: u32) -> Handle;
        fn WaitForSingleObject(handle: Handle, milliseconds: u32) -> u32;
        fn CloseHandle(handle: Handle) -> i32;
        fn GetFileInformationByHandleEx(
            handle: Handle,
            class: i32,
            information: *mut c_void,
            length: u32,
        ) -> i32;
        fn SetFileInformationByHandle(
            handle: Handle,
            class: i32,
            information: *const c_void,
            length: u32,
        ) -> i32;
    }

    #[link(name = "bcrypt")]
    extern "system" {
        fn BCryptGenRandom(algorithm: Handle, buffer: *mut u8, length: u32, flags: u32) -> i32;
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        use std::os::windows::process::CommandExt;

        struct Fixture(PathBuf);
        impl Fixture {
            fn new() -> Self {
                let path = std::env::temp_dir()
                    .join(format!("cia-lock-tests-{}", random_nonce().unwrap()));
                fs::create_dir(&path).unwrap();
                Self(path)
            }
            fn reserve(&self, owner: Owner) -> Journal {
                let output = self.0.join("test.mp4");
                let lock = self.0.join(".test.mp4.cia-render.lock");
                let mut file = OpenOptions::new()
                    .write(true)
                    .read(true)
                    .create_new(true)
                    .open(&lock)
                    .unwrap();
                register_for_owner(&self.0.join("recovery"), &mut file, &output, &lock, owner)
                    .unwrap()
            }
        }
        impl Drop for Fixture {
            fn drop(&mut self) {
                let _ = fs::remove_dir_all(&self.0);
            }
        }

        fn current_owner() -> Owner {
            Owner {
                pid: std::process::id(),
                created: process_creation(unsafe { GetCurrentProcess() }).unwrap(),
            }
        }

        fn exited_owner() -> Owner {
            let mut child = std::process::Command::new("powershell.exe")
                .args([
                    "-NoProfile",
                    "-NonInteractive",
                    "-Command",
                    "Start-Sleep -Seconds 60",
                ])
                .creation_flags(0x0800_0000)
                .spawn()
                .unwrap();
            let owner = Owner {
                pid: child.id(),
                created: process_creation(child.as_raw_handle()).unwrap(),
            };
            child.kill().unwrap();
            child.wait().unwrap();
            owner
        }

        #[test]
        fn running_owner_and_unknown_legacy_lock_are_retained() {
            let fixture = Fixture::new();
            let journal = fixture.reserve(current_owner());
            let legacy = fixture.0.join(".legacy.mp4.cia-render.lock");
            fs::write(&legacy, b"").unwrap();
            let report = recover_stale(&fixture.0.join("recovery"));
            assert_eq!(report.recovered, 0);
            assert_eq!(report.retained, 1);
            assert!(journal.record.lock.exists());
            assert!(legacy.exists());
            journal.finish().unwrap();
            assert!(!journal.record.lock.exists());
            assert!(!journal.path.exists());
            assert!(legacy.exists());
        }

        #[test]
        fn terminated_owner_recovers_only_its_lock() {
            let fixture = Fixture::new();
            let journal = fixture.reserve(exited_owner());
            let output = &journal.record.output;
            fs::write(output, b"published video must remain").unwrap();
            let workspace = fixture.0.join(".cia-render-owned-test");
            fs::create_dir(&workspace).unwrap();
            fs::write(workspace.join("unpublished.mp4"), b"keep for diagnosis").unwrap();
            let report = recover_stale(&fixture.0.join("recovery"));
            assert_eq!(report.recovered, 1);
            assert_eq!(report.retained, 0);
            assert!(!journal.record.lock.exists());
            assert!(!journal.path.exists());
            assert_eq!(fs::read(output).unwrap(), b"published video must remain");
            assert!(workspace.join("unpublished.mp4").exists());
        }

        #[test]
        fn changed_content_or_replaced_file_is_never_deleted() {
            let fixture = Fixture::new();
            let journal = fixture.reserve(exited_owner());
            fs::write(&journal.record.lock, b"external modification").unwrap();
            assert_eq!(recover_stale(&fixture.0.join("recovery")).recovered, 0);
            assert!(journal.record.lock.exists());
            fs::remove_file(&journal.record.lock).unwrap();
            fs::write(&journal.record.lock, &journal.record.lock_bytes).unwrap();
            assert_eq!(recover_stale(&fixture.0.join("recovery")).recovered, 0);
            assert!(journal.record.lock.exists());
            assert!(journal.path.exists());
        }

        #[test]
        fn recycled_pid_is_distinguished_by_kernel_creation_identity() {
            let mut owner = current_owner();
            assert!(!owner_terminated(owner).unwrap());
            owner.created -= 1;
            assert!(owner_terminated(owner).unwrap());
        }

        #[test]
        fn malformed_journal_and_hard_link_are_retained() {
            let fixture = Fixture::new();
            let journal = fixture.reserve(exited_owner());
            fs::hard_link(&journal.record.lock, fixture.0.join("second-link")).unwrap();
            fs::write(fixture.0.join("recovery").join("unknown.json"), b"{}").unwrap();
            let report = recover_stale(&fixture.0.join("recovery"));
            assert_eq!(report.recovered, 0);
            assert_eq!(report.retained, 2);
            assert!(journal.record.lock.exists());
        }

        #[test]
        fn reparse_parent_is_rejected_even_when_file_identity_matches() {
            let fixture = Fixture::new();
            let journal = fixture.reserve(exited_owner());
            let alias = fixture.0.join("alias");
            let status = std::process::Command::new("cmd.exe")
                .args(["/D", "/C", "mklink", "/J"])
                .arg(&alias)
                .arg(&fixture.0)
                .creation_flags(0x0800_0000)
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .status()
                .unwrap();
            assert!(
                status.success(),
                "The NTFS junction fixture must be created"
            );
            let mut record = journal.record.clone();
            record.lock = alias.join(".test.mp4.cia-render.lock");
            record.output = alias.join("test.mp4");
            fs::write(&journal.path, serde_json::to_vec(&record).unwrap()).unwrap();
            let report = recover_stale(&fixture.0.join("recovery"));
            assert_eq!(report.recovered, 0);
            assert_eq!(report.retained, 1);
            assert!(journal.record.lock.exists());
            assert!(journal.path.exists());
            // Remove the junction itself with the native filesystem API.
            fs::remove_dir(alias).unwrap();
        }
    }
}

#[cfg(target_os = "windows")]
pub use platform::{recover_stale, register, Journal};
