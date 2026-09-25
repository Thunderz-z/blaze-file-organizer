use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::fs::{self, File, OpenOptions};
#[cfg(not(target_os = "windows"))]
use std::io::Read;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;
use tauri::Emitter;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CategoryRule {
    pub id: String,
    pub name: String,
    pub enabled: bool,
    pub target_folder: String,
    pub extensions: Vec<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScanRules {
    pub categories: Vec<CategoryRule>,
    pub custom_extensions: Vec<String>,
    pub include_subfolders: bool,
    pub include_hidden: bool,
    pub custom_target_dir: Option<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FilePreview {
    pub id: String,
    pub file_name: String,
    pub extension: String,
    pub category: String,
    pub source_path: String,
    pub destination_path: String,
    pub size_bytes: u64,
    pub conflict_detected: bool,
    pub relative_path: Option<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScanResult {
    pub previews: Vec<FilePreview>,
    pub skipped_paths: Vec<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileAction {
    pub id: String,
    pub source_path: String,
    pub destination_path: String,
    pub category: String,
    pub file_name: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompletedFile {
    pub id: String,
    pub destination_path: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionSummary {
    pub total_processed: usize,
    pub successful: usize,
    pub failed: usize,
    pub time_taken_ms: u64,
    pub mode: String,
    pub errors: Vec<String>,
    pub completed: Vec<CompletedFile>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DirectoryStats {
    pub path: String,
    pub total_files: usize,
    pub total_size_bytes: u64,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
struct TransferProgress {
    file_index: usize,
    total_files: usize,
}

// Used only on non-Windows builds; Windows uses copy_file_native (CopyFileExW) instead.
#[cfg(not(target_os = "windows"))]
const FAST_COPY_BUFFER_SIZE: usize = 8 * 1024 * 1024;
const MAX_DESTINATION_ATTEMPTS: u32 = 10_000;
static TEMP_FILE_COUNTER: AtomicU64 = AtomicU64::new(0);

struct CopyOutcome {
    expected_size: u64,
    copied_bytes: u64,
}

#[cfg(target_os = "windows")]
fn is_hidden_path(path: &Path, metadata: Option<&fs::Metadata>) -> bool {
    use std::os::windows::fs::MetadataExt;
    const FILE_ATTRIBUTE_HIDDEN: u32 = 0x0000_0002;

    if let Some(file_name) = path.file_name().and_then(|n| n.to_str()) {
        if file_name.starts_with('.') {
            return true;
        }
    }
    if let Some(meta) = metadata {
        return (meta.file_attributes() & FILE_ATTRIBUTE_HIDDEN) != 0;
    }
    if let Ok(meta) = fs::symlink_metadata(path) {
        return (meta.file_attributes() & FILE_ATTRIBUTE_HIDDEN) != 0;
    }
    false
}

#[cfg(not(target_os = "windows"))]
fn is_hidden_path(path: &Path, _metadata: Option<&fs::Metadata>) -> bool {
    path.file_name()
        .and_then(|n| n.to_str())
        .map(|name| name.starts_with('.'))
        .unwrap_or(false)
}

fn is_same_file(source: &Path, destination: &Path) -> bool {
    if source == destination {
        return true;
    }
    match (fs::canonicalize(source), fs::canonicalize(destination)) {
        (Ok(src), Ok(dst)) => src == dst,
        _ => false,
    }
}

fn destination_candidate(target: &Path, counter: u32) -> PathBuf {
    if counter == 0 {
        return target.to_path_buf();
    }
    let parent = target.parent().unwrap_or_else(|| Path::new(""));
    let stem = target
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("file");
    let ext = target
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| format!(".{e}"))
        .unwrap_or_default();
    parent.join(format!("{stem}_{counter}{ext}"))
}

// Preview paths are advisory only. Execution uses atomic creation/link operations instead.
fn resolve_preview_destination(target: &Path, reserved: &HashSet<PathBuf>) -> PathBuf {
    for counter in 0..=MAX_DESTINATION_ATTEMPTS {
        let candidate = destination_candidate(target, counter);
        if !candidate.exists() && !reserved.contains(&candidate) {
            return candidate;
        }
    }
    destination_candidate(target, MAX_DESTINATION_ATTEMPTS + 1)
}

fn create_temporary_file(parent: &Path, destination: &Path) -> io::Result<(PathBuf, File)> {
    let name = destination
        .file_name()
        .and_then(|v| v.to_str())
        .unwrap_or("transfer");
    for _ in 0..MAX_DESTINATION_ATTEMPTS {
        let sequence = TEMP_FILE_COUNTER.fetch_add(1, Ordering::Relaxed);
        let candidate = parent.join(format!(
            ".{name}.blaze-part-{}-{sequence}",
            std::process::id()
        ));
        match OpenOptions::new()
            .read(true)
            .write(true)
            .create_new(true)
            .open(&candidate)
        {
            Ok(file) => return Ok((candidate, file)),
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(error),
        }
    }
    Err(io::Error::new(
        io::ErrorKind::AlreadyExists,
        "could not reserve a temporary transfer file",
    ))
}

// This verifies transfer completeness (byte count and resulting size), not file content.
// A stronger optional verifier can be added here later without hashing every file by default.
fn verify_copy_completeness(destination: &Path, outcome: &CopyOutcome) -> io::Result<()> {
    let destination_size = fs::metadata(destination)?.len();
    if outcome.copied_bytes != outcome.expected_size || destination_size != outcome.expected_size {
        return Err(io::Error::new(
            io::ErrorKind::UnexpectedEof,
            format!(
                "copy size mismatch: expected {} bytes, copied {} bytes, destination has {} bytes",
                outcome.expected_size, outcome.copied_bytes, destination_size
            ),
        ));
    }
    Ok(())
}

// Used only on non-Windows builds; Windows uses copy_file_native (CopyFileExW) instead.
#[cfg(not(target_os = "windows"))]
fn copy_source_to_file(source: &Path, writer: &mut File) -> io::Result<CopyOutcome> {
    let mut reader = File::open(source)?;
    let expected_size = reader.metadata()?.len();
    let mut buffer = vec![0u8; FAST_COPY_BUFFER_SIZE];
    let mut copied_bytes = 0u64;
    loop {
        let bytes_read = reader.read(&mut buffer)?;
        if bytes_read == 0 {
            break;
        }
        writer.write_all(&buffer[..bytes_read])?;
        copied_bytes += bytes_read as u64;
    }
    writer.flush()?;
    writer.sync_all()?;
    Ok(CopyOutcome {
        expected_size,
        copied_bytes,
    })
}

/// Windows-native copy using CopyFileExW.
/// Copies `source` to `dest` using the same OS kernel path Explorer uses.
/// `dest` must exist (we created it as an empty temp placeholder) and will be overwritten.
#[cfg(target_os = "windows")]
fn copy_file_native(source: &Path, dest: &Path) -> io::Result<CopyOutcome> {
    use std::os::windows::ffi::OsStrExt;

    let expected_size = fs::metadata(source)?.len();

    let wide_src: Vec<u16> = source.as_os_str().encode_wide().chain(Some(0)).collect();
    let wide_dst: Vec<u16> = dest.as_os_str().encode_wide().chain(Some(0)).collect();

    extern "system" {
        fn CopyFileExW(
            lpExistingFileName: *const u16,
            lpNewFileName: *const u16,
            lpProgressRoutine: *mut std::ffi::c_void,
            lpData: *mut std::ffi::c_void,
            pbCancel: *mut i32,
            dwCopyFlags: u32,
        ) -> i32;
    }

    // dwCopyFlags = 0: overwrite the empty temp-file placeholder we created to reserve the name.
    // Progress routine and cancel pointer are null for this phase.
    let ok = unsafe {
        CopyFileExW(
            wide_src.as_ptr(),
            wide_dst.as_ptr(),
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            0,
        )
    };
    if ok == 0 {
        return Err(io::Error::last_os_error());
    }
    let copied_bytes = fs::metadata(dest)?.len();
    Ok(CopyOutcome {
        expected_size,
        copied_bytes,
    })
}

#[cfg(target_os = "windows")]
fn rename_no_overwrite(source: &Path, destination: &Path) -> io::Result<()> {
    use std::os::windows::ffi::OsStrExt;
    let wide_src: Vec<u16> = source.as_os_str().encode_wide().chain(Some(0)).collect();
    let wide_dst: Vec<u16> = destination
        .as_os_str()
        .encode_wide()
        .chain(Some(0))
        .collect();

    extern "system" {
        fn MoveFileExW(src: *const u16, dst: *const u16, flags: u32) -> i32;
    }

    let success = unsafe { MoveFileExW(wide_src.as_ptr(), wide_dst.as_ptr(), 0) };
    if success != 0 {
        Ok(())
    } else {
        Err(io::Error::last_os_error())
    }
}

fn fallback_copy_stream_no_overwrite(temp: &Path, candidate: &Path) -> io::Result<()> {
    let mut writer = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(candidate)?;
    let mut reader = File::open(temp)?;
    io::copy(&mut reader, &mut writer)?;
    writer.flush()?;
    writer.sync_all()?;
    drop(writer);
    drop(reader);
    let _ = fs::remove_file(temp);
    Ok(())
}

fn publish_fallback_no_overwrite(temp: &Path, candidate: &Path) -> io::Result<()> {
    #[cfg(target_os = "windows")]
    {
        match rename_no_overwrite(temp, candidate) {
            Ok(()) => Ok(()),
            Err(err) if err.kind() == io::ErrorKind::AlreadyExists => Err(err),
            Err(_) => fallback_copy_stream_no_overwrite(temp, candidate),
        }
    }
    #[cfg(not(target_os = "windows"))]
    {
        fallback_copy_stream_no_overwrite(temp, candidate)
    }
}

fn publish_candidate(temp: &Path, candidate: &Path) -> io::Result<()> {
    match fs::hard_link(temp, candidate) {
        Ok(()) => {
            let _ = fs::remove_file(temp);
            Ok(())
        }
        Err(error) if error.kind() == io::ErrorKind::AlreadyExists => Err(error),
        Err(_) => {
            // Hard links are unavailable or unsupported on this filesystem (e.g. FAT32/exFAT).
            // Fall back to safe publishing without overwriting.
            publish_fallback_no_overwrite(temp, candidate)
        }
    }
}

fn publish_temporary_file(temp: &Path, planned: &Path) -> io::Result<PathBuf> {
    for counter in 0..=MAX_DESTINATION_ATTEMPTS {
        let candidate = destination_candidate(planned, counter);
        match publish_candidate(temp, &candidate) {
            Ok(()) => return Ok(candidate),
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(error),
        }
    }
    Err(io::Error::new(
        io::ErrorKind::AlreadyExists,
        "could not reserve a unique destination name",
    ))
}

fn copy_to_unique_destination(source: &Path, planned: &Path) -> io::Result<(PathBuf, u64)> {
    let parent = planned.parent().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            "destination path does not have a parent directory",
        )
    })?;
    fs::create_dir_all(parent)?;
    let (temporary_path, temporary_file) = create_temporary_file(parent, planned)?;

    let outcome = {
        #[cfg(target_os = "windows")]
        {
            // Drop the handle so CopyFileExW can open the temp path exclusively.
            drop(temporary_file);
            match copy_file_native(source, &temporary_path) {
                Ok(outcome) => outcome,
                Err(error) => {
                    let _ = fs::remove_file(&temporary_path);
                    return Err(error);
                }
            }
        }
        #[cfg(not(target_os = "windows"))]
        {
            let mut tf = temporary_file;
            match copy_source_to_file(source, &mut tf) {
                Ok(outcome) => {
                    drop(tf);
                    outcome
                }
                Err(error) => {
                    drop(tf);
                    let _ = fs::remove_file(&temporary_path);
                    return Err(error);
                }
            }
        }
    };

    if let Err(error) = verify_copy_completeness(&temporary_path, &outcome) {
        let _ = fs::remove_file(&temporary_path);
        return Err(error);
    }
    match publish_temporary_file(&temporary_path, planned) {
        Ok(destination) => Ok((destination, outcome.expected_size)),
        Err(error) => {
            let _ = fs::remove_file(&temporary_path);
            Err(error)
        }
    }
}

fn try_same_filesystem_move(source: &Path, planned: &Path) -> io::Result<Option<PathBuf>> {
    let expected_size = fs::metadata(source)?.len();
    let parent = planned.parent().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            "destination path does not have a parent directory",
        )
    })?;
    fs::create_dir_all(parent)?;
    // Link+unlink is an atomic no-overwrite same-filesystem move. Unlike rename, it cannot replace a destination.
    for counter in 0..=MAX_DESTINATION_ATTEMPTS {
        let candidate = destination_candidate(planned, counter);
        match fs::hard_link(source, &candidate) {
            Ok(()) => {
                verify_copy_completeness(
                    &candidate,
                    &CopyOutcome {
                        expected_size,
                        copied_bytes: expected_size,
                    },
                )?;
                if fs::metadata(source)?.len() != expected_size {
                    return Err(io::Error::other(
                        "source changed during move; source was preserved",
                    ));
                }
                fs::remove_file(source)?;
                return Ok(Some(candidate));
            }
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
            // Cross-volume and hard-link-unsupported filesystems use the guarded copy fallback.
            Err(_) => return Ok(None),
        }
    }
    Ok(None)
}

fn copy_then_remove_source(source: &Path, planned: &Path) -> io::Result<PathBuf> {
    let (destination, expected_size) = copy_to_unique_destination(source, planned)?;
    // Only remove the source after the published destination exists with the expected size.
    if fs::metadata(&destination)?.len() != expected_size {
        return Err(io::Error::new(
            io::ErrorKind::UnexpectedEof,
            "destination changed before source removal; source was preserved",
        ));
    }
    if fs::metadata(source)?.len() != expected_size {
        return Err(io::Error::other(
            "source changed during copy; source was preserved",
        ));
    }
    fs::remove_file(source)?;
    Ok(destination)
}

fn transfer_item(source: &Path, planned: &Path, is_move: bool) -> io::Result<PathBuf> {
    let source_metadata = fs::symlink_metadata(source)?;
    if source_metadata.file_type().is_symlink() || !source_metadata.is_file() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "source is not a regular file",
        ));
    }
    if is_same_file(source, planned) {
        return Ok(planned.to_path_buf());
    }
    if !is_move {
        return copy_to_unique_destination(source, planned).map(|(destination, _)| destination);
    }
    if let Some(destination) = try_same_filesystem_move(source, planned)? {
        return Ok(destination);
    }
    copy_then_remove_source(source, planned)
}

#[tauri::command]
async fn scan_directory(source_dir: String, rules: ScanRules) -> Result<ScanResult, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let root = Path::new(&source_dir);
        if !root.exists() || !root.is_dir() {
            return Err(format!(
                "Directory does not exist or is not a folder: {source_dir}"
            ));
        }
        let mut ext_map = HashMap::new();
        let mut organizer_folder_names = Vec::new();
        for category in &rules.categories {
            if category.enabled {
                organizer_folder_names.push(category.target_folder.clone());
                organizer_folder_names.push(category.name.clone());
                for extension in &category.extensions {
                    let clean = extension.trim().trim_start_matches('.').to_lowercase();
                    if !clean.is_empty() {
                        ext_map.insert(
                            clean,
                            (category.name.clone(), category.target_folder.clone()),
                        );
                    }
                }
            }
        }
        organizer_folder_names.push("Custom".to_string());
        for extension in &rules.custom_extensions {
            let clean = extension.trim().trim_start_matches('.').to_lowercase();
            if !clean.is_empty() {
                ext_map.insert(clean, ("Custom".to_string(), "Custom".to_string()));
            }
        }
        let target_root = match &rules.custom_target_dir {
            Some(target) if !target.trim().is_empty() => PathBuf::from(target),
            _ => root.to_path_buf(),
        };
        let mut previews = Vec::new();
        let mut skipped_paths = Vec::new();
        let mut preview_destinations = HashSet::new();
        let mut dir_stack = vec![root.to_path_buf()];
        while let Some(current_dir) = dir_stack.pop() {
            let entries = match fs::read_dir(&current_dir) {
                Ok(entries) => entries,
                Err(error) => {
                    skipped_paths.push(format!("{}: {error}", current_dir.display()));
                    continue;
                }
            };
            for entry_result in entries {
                let entry = match entry_result {
                    Ok(entry) => entry,
                    Err(error) => {
                        skipped_paths.push(format!("{}: {error}", current_dir.display()));
                        continue;
                    }
                };
                let path = entry.path();
                let file_name = match path.file_name().and_then(|name| name.to_str()) {
                    Some(name) => name.to_string(),
                    None => {
                        skipped_paths.push(format!("{}: non-Unicode file name", path.display()));
                        continue;
                    }
                };
                let metadata = entry.metadata().ok();
                if !rules.include_hidden && is_hidden_path(&path, metadata.as_ref()) {
                    continue;
                }
                let file_type = match entry.file_type() {
                    Ok(file_type) => file_type,
                    Err(error) => {
                        skipped_paths.push(format!("{}: {error}", path.display()));
                        continue;
                    }
                };
                // Never follow symlinks: directory links cannot create recursive traversal loops.
                if file_type.is_symlink() {
                    continue;
                }
                if file_type.is_dir() {
                    if rules.include_subfolders
                        && !organizer_folder_names
                            .iter()
                            .any(|target| target == &file_name)
                    {
                        dir_stack.push(path);
                    }
                    continue;
                }
                if !file_type.is_file() {
                    continue;
                }
                let extension = path
                    .extension()
                    .and_then(|extension| extension.to_str())
                    .map(|extension| extension.to_lowercase())
                    .unwrap_or_default();
                if let Some((category_name, target_folder)) = ext_map.get(&extension) {
                    let nominal = target_root.join(target_folder).join(&file_name);
                    if is_same_file(&path, &nominal) {
                        continue;
                    }
                    let conflict_detected =
                        nominal.exists() || preview_destinations.contains(&nominal);
                    let destination = resolve_preview_destination(&nominal, &preview_destinations);
                    let size_bytes = match metadata {
                        Some(ref m) => m.len(),
                        None => match entry.metadata() {
                            Ok(m) => m.len(),
                            Err(error) => {
                                skipped_paths.push(format!("{}: {error}", path.display()));
                                continue;
                            }
                        },
                    };
                    preview_destinations.insert(destination.clone());
                    previews.push(FilePreview {
                        id: format!("{}:{}", previews.len(), path.display()),
                        file_name,
                        extension,
                        category: category_name.clone(),
                        source_path: path.to_string_lossy().to_string(),
                        destination_path: destination.to_string_lossy().to_string(),
                        size_bytes,
                        conflict_detected,
                        relative_path: path
                            .strip_prefix(root)
                            .ok()
                            .map(|relative| relative.to_string_lossy().to_string()),
                    });
                }
            }
        }
        Ok(ScanResult {
            previews,
            skipped_paths,
        })
    })
    .await
    .map_err(|error| format!("Scan task failed: {error}"))?
}

fn execute_organization_blocking(
    items: &[FileAction],
    mode: &str,
    on_progress: &dyn Fn(usize, usize),
) -> ExecutionSummary {
    let start_time = Instant::now();
    let is_move = mode.eq_ignore_ascii_case("MOVE");
    let total_files = items.len();
    let mut successful = 0;
    let mut failed = 0;
    let mut errors = Vec::new();
    let mut completed = Vec::new();
    for item in items {
        match transfer_item(
            Path::new(&item.source_path),
            Path::new(&item.destination_path),
            is_move,
        ) {
            Ok(destination) => {
                successful += 1;
                completed.push(CompletedFile {
                    id: item.id.clone(),
                    destination_path: destination.to_string_lossy().to_string(),
                });
            }
            Err(error) => {
                failed += 1;
                errors.push(format!("{}: {error}", item.source_path));
            }
        }
        // Report per-file progress so the frontend can update the progress bar in real time.
        on_progress(successful + failed, total_files);
    }
    ExecutionSummary {
        total_processed: total_files,
        successful,
        failed,
        time_taken_ms: start_time.elapsed().as_millis() as u64,
        mode: mode.to_string(),
        errors,
        completed,
    }
}

#[tauri::command]
async fn execute_organization(
    app_handle: tauri::AppHandle,
    items: Vec<FileAction>,
    mode: String,
) -> Result<ExecutionSummary, String> {
    tauri::async_runtime::spawn_blocking(move || {
        Ok(execute_organization_blocking(&items, &mode, &|done, total| {
            let _ = app_handle.emit(
                "transfer-progress",
                TransferProgress {
                    file_index: done,
                    total_files: total,
                },
            );
        }))
    })
    .await
    .map_err(|error| format!("Organization task failed: {error}"))?
}

#[tauri::command]
async fn get_folder_stats(path: String) -> Result<DirectoryStats, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let directory = Path::new(&path);
        if !directory.is_dir() {
            return Err("Path is not a directory".to_string());
        }
        let mut total_files = 0;
        let mut total_size_bytes = 0;
        if let Ok(entries) = fs::read_dir(directory) {
            for entry in entries.flatten() {
                if let Ok(metadata) = entry.metadata() {
                    if metadata.is_file() {
                        total_files += 1;
                        total_size_bytes += metadata.len();
                    }
                }
            }
        }
        Ok(DirectoryStats {
            path,
            total_files,
            total_size_bytes,
        })
    })
    .await
    .map_err(|error| error.to_string())?
}

#[tauri::command]
fn greet(name: &str) -> String {
    format!("Blaze Engine ready. Hello, {name}!")
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![
            greet,
            scan_directory,
            execute_organization,
            get_folder_stats
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};
    fn test_directory(name: &str) -> PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let directory = std::env::temp_dir().join(format!(
            "blaze-file-organizer-{name}-{}-{nonce}",
            std::process::id()
        ));
        fs::create_dir_all(&directory).unwrap();
        directory
    }
    fn action(id: &str, source: &Path, destination: &Path) -> FileAction {
        FileAction {
            id: id.to_string(),
            source_path: source.to_string_lossy().to_string(),
            destination_path: destination.to_string_lossy().to_string(),
            category: "Test".to_string(),
            file_name: source.file_name().unwrap().to_string_lossy().to_string(),
        }
    }

    #[test]
    fn advisory_names_avoid_existing_and_reserved_destinations() {
        let directory = test_directory("advisory-names");
        let target = directory.join("report.txt");
        fs::write(&target, "existing").unwrap();
        let mut reserved = HashSet::new();
        reserved.insert(directory.join("report_1.txt"));
        assert_eq!(
            resolve_preview_destination(&target, &reserved),
            directory.join("report_2.txt")
        );
        fs::remove_dir_all(directory).unwrap();
    }
    #[test]
    fn copy_preserves_existing_destination_and_resolves_collision() {
        let directory = test_directory("copy-collision");
        let source = directory.join("source.txt");
        let target = directory.join("target.txt");
        fs::write(&source, "new").unwrap();
        fs::write(&target, "existing").unwrap();
        let (actual, _) = copy_to_unique_destination(&source, &target).unwrap();
        assert_eq!(actual, directory.join("target_1.txt"));
        assert_eq!(fs::read_to_string(&target).unwrap(), "existing");
        assert_eq!(fs::read_to_string(&actual).unwrap(), "new");
        fs::remove_dir_all(directory).unwrap();
    }
    #[test]
    fn duplicate_batch_destinations_are_uniquely_resolved() {
        let directory = test_directory("duplicate-batch");
        let first = directory.join("first/same.txt");
        let second = directory.join("second/same.txt");
        let target = directory.join("organized/same.txt");
        fs::create_dir_all(first.parent().unwrap()).unwrap();
        fs::create_dir_all(second.parent().unwrap()).unwrap();
        fs::write(&first, "one").unwrap();
        fs::write(&second, "two").unwrap();
        let summary = execute_organization_blocking(
            &[
                action("first", &first, &target),
                action("second", &second, &target),
            ],
            "COPY",
            &|_, _| {},
        );
        assert_eq!(summary.successful, 2);
        assert_eq!(summary.failed, 0);
        assert_eq!(
            summary.completed[0].destination_path,
            target.to_string_lossy()
        );
        assert_eq!(
            Path::new(&summary.completed[1].destination_path),
            directory.join("organized").join("same_1.txt")
        );
        fs::remove_dir_all(directory).unwrap();
    }
    #[test]
    fn stale_destination_created_after_preview_is_preserved() {
        let directory = test_directory("stale-destination");
        let source = directory.join("source.txt");
        let target = directory.join("organized/source.txt");
        fs::write(&source, "source").unwrap();
        fs::create_dir_all(target.parent().unwrap()).unwrap();
        fs::write(&target, "created after scan").unwrap();
        let summary = execute_organization_blocking(&[action("source", &source, &target)], "COPY", &|_, _| {});
        assert_eq!(summary.successful, 1);
        assert_eq!(fs::read_to_string(&target).unwrap(), "created after scan");
        assert_eq!(
            fs::read_to_string(directory.join("organized/source_1.txt")).unwrap(),
            "source"
        );
        fs::remove_dir_all(directory).unwrap();
    }
    #[test]
    fn copy_failure_preserves_source() {
        let directory = test_directory("copy-failure");
        let source = directory.join("source.txt");
        let invalid_parent = directory.join("not-a-directory");
        fs::write(&source, "keep me").unwrap();
        fs::write(&invalid_parent, "blocker").unwrap();
        assert!(copy_to_unique_destination(&source, &invalid_parent.join("target.txt")).is_err());
        assert_eq!(fs::read_to_string(&source).unwrap(), "keep me");
        fs::remove_dir_all(directory).unwrap();
    }
    #[test]
    fn same_filesystem_move_removes_source_after_safe_publish() {
        let directory = test_directory("same-volume-move");
        let source = directory.join("source.txt");
        let target = directory.join("organized/source.txt");
        fs::write(&source, "move me").unwrap();
        let summary = execute_organization_blocking(&[action("source", &source, &target)], "MOVE", &|_, _| {});
        assert_eq!(summary.successful, 1);
        assert!(!source.exists());
        assert_eq!(fs::read_to_string(&target).unwrap(), "move me");
        fs::remove_dir_all(directory).unwrap();
    }
    #[test]
    fn copy_fallback_move_removes_source_only_after_verified_copy() {
        let directory = test_directory("copy-fallback-move");
        let source = directory.join("source.txt");
        let target = directory.join("organized/source.txt");
        fs::write(&source, "fallback payload").unwrap();
        let actual = copy_then_remove_source(&source, &target).unwrap();
        assert!(!source.exists());
        assert_eq!(actual, target);
        assert_eq!(fs::read_to_string(&actual).unwrap(), "fallback payload");
        fs::remove_dir_all(directory).unwrap();
    }
    #[test]
    fn self_collision_preserves_destination_without_suffix() {
        let directory = test_directory("self-collision");
        let target = directory.join("organized/source.txt");
        fs::create_dir_all(target.parent().unwrap()).unwrap();
        fs::write(&target, "already organized").unwrap();
        let summary_move =
            execute_organization_blocking(&[action("self-move", &target, &target)], "MOVE", &|_, _| {});
        assert_eq!(summary_move.successful, 1);
        assert_eq!(
            summary_move.completed[0].destination_path,
            target.to_string_lossy()
        );
        assert_eq!(fs::read_to_string(&target).unwrap(), "already organized");
        assert!(!directory.join("organized/source_1.txt").exists());

        let summary_copy =
            execute_organization_blocking(&[action("self-copy", &target, &target)], "COPY", &|_, _| {});
        assert_eq!(summary_copy.successful, 1);
        assert_eq!(
            summary_copy.completed[0].destination_path,
            target.to_string_lossy()
        );
        assert_eq!(fs::read_to_string(&target).unwrap(), "already organized");
        assert!(!directory.join("organized/source_1.txt").exists());
        fs::remove_dir_all(directory).unwrap();
    }
    #[test]
    fn destination_publishing_fallback_does_not_overwrite() {
        let directory = test_directory("publishing-fallback");
        let temp = directory.join("temp.blaze-part");
        let target = directory.join("target.txt");
        fs::write(&temp, "new content").unwrap();
        fs::write(&target, "original content").unwrap();

        assert!(publish_fallback_no_overwrite(&temp, &target).is_err());
        assert_eq!(fs::read_to_string(&target).unwrap(), "original content");

        let available = directory.join("available.txt");
        assert!(publish_fallback_no_overwrite(&temp, &available).is_ok());
        assert_eq!(fs::read_to_string(&available).unwrap(), "new content");
        assert!(!temp.exists());
        fs::remove_dir_all(directory).unwrap();
    }
    #[test]
    #[cfg(target_os = "windows")]
    fn windows_hidden_file_detection() {
        use std::os::windows::ffi::OsStrExt;
        let directory = test_directory("hidden-files");
        let normal_file = directory.join("normal.txt");
        let hidden_file = directory.join("hidden.txt");
        let dot_file = directory.join(".dotfile.txt");
        fs::write(&normal_file, "normal").unwrap();
        fs::write(&hidden_file, "hidden").unwrap();
        fs::write(&dot_file, "dot").unwrap();

        extern "system" {
            fn SetFileAttributesW(lpFileName: *const u16, dwFileAttributes: u32) -> i32;
        }
        let wide: Vec<u16> = hidden_file
            .as_os_str()
            .encode_wide()
            .chain(Some(0))
            .collect();
        unsafe { SetFileAttributesW(wide.as_ptr(), 0x0000_0002) };

        assert!(!is_hidden_path(&normal_file, None));
        assert!(is_hidden_path(&hidden_file, None));
        assert!(is_hidden_path(&dot_file, None));
        fs::remove_dir_all(directory).unwrap();
    }
}
