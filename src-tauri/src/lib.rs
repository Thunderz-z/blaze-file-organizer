use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Instant;

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
pub struct FileAction {
    pub id: String,
    pub source_path: String,
    pub destination_path: String,
    pub category: String,
    pub file_name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionSummary {
    pub total_processed: usize,
    pub successful: usize,
    pub failed: usize,
    pub time_taken_ms: u64,
    pub mode: String,
    pub errors: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DirectoryStats {
    pub path: String,
    pub total_files: usize,
    pub total_size_bytes: u64,
}

// FastCopy-style 8MB buffer for maximum I/O throughput across partitions
const FAST_COPY_BUFFER_SIZE: usize = 8 * 1024 * 1024;

fn resolve_unique_path(target_path: &Path) -> PathBuf {
    if !target_path.exists() {
        return target_path.to_path_buf();
    }

    let parent = target_path.parent().unwrap_or_else(|| Path::new(""));
    let stem = target_path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("file");
    let ext = target_path
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| format!(".{}", e))
        .unwrap_or_default();

    let mut counter = 1;
    loop {
        let new_file_name = format!("{}_{}{}", stem, counter, ext);
        let candidate = parent.join(new_file_name);
        if !candidate.exists() {
            return candidate;
        }
        counter += 1;
        if counter > 10000 {
            // Circuit breaker
            return parent.join(format!("{}_{}_{}", stem, counter, ext));
        }
    }
}

// FastCopy-inspired high throughput buffered copy with size & verification
fn fast_buffered_copy(src: &Path, dest: &Path) -> io::Result<u64> {
    let mut reader = File::open(src)?;
    let mut writer = OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .open(dest)?;

    let mut buffer = vec![0u8; FAST_COPY_BUFFER_SIZE];
    let mut total_copied = 0u64;

    loop {
        let bytes_read = reader.read(&mut buffer)?;
        if bytes_read == 0 {
            break;
        }
        writer.write_all(&buffer[..bytes_read])?;
        total_copied += bytes_read as u64;
    }

    writer.flush()?;
    Ok(total_copied)
}

#[tauri::command]
pub async fn scan_directory(
    source_dir: String,
    rules: ScanRules,
) -> Result<Vec<FilePreview>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let root = Path::new(&source_dir);
        if !root.exists() || !root.is_dir() {
            return Err(format!("Directory does not exist or is not a folder: {}", source_dir));
        }

        // Map extension -> (Category Name, Target Folder)
        let mut ext_map: HashMap<String, (String, String)> = HashMap::new();
        let mut organizer_folder_names: Vec<String> = Vec::new();

        for cat in &rules.categories {
            if cat.enabled {
                organizer_folder_names.push(cat.target_folder.clone());
                organizer_folder_names.push(cat.name.clone());
                for ext in &cat.extensions {
                    let clean = ext.trim().trim_start_matches('.').to_lowercase();
                    if !clean.is_empty() {
                        ext_map.insert(clean, (cat.name.clone(), cat.target_folder.clone()));
                    }
                }
            }
        }

        // Custom extensions map to "Custom" category
        organizer_folder_names.push("Custom".to_string());
        for ext in &rules.custom_extensions {
            let clean = ext.trim().trim_start_matches('.').to_lowercase();
            if !clean.is_empty() {
                ext_map.insert(clean, ("Custom".to_string(), "Custom".to_string()));
            }
        }

        let target_root = match &rules.custom_target_dir {
            Some(t) if !t.trim().is_empty() => PathBuf::from(t),
            _ => root.to_path_buf(),
        };

        let mut previews = Vec::new();
        let mut dir_stack: Vec<PathBuf> = vec![root.to_path_buf()];

        while let Some(current_dir) = dir_stack.pop() {
            let entries = match fs::read_dir(&current_dir) {
                Ok(e) => e,
                Err(_) => continue, // Gracefully skip unreadable folders
            };

            for entry in entries.flatten() {
                let path = entry.path();
                let file_name = match path.file_name().and_then(|n| n.to_str()) {
                    Some(n) => n.to_string(),
                    None => continue,
                };

                // Skip hidden files unless explicitly requested
                if !rules.include_hidden && file_name.starts_with('.') {
                    continue;
                }

                if path.is_dir() {
                    if rules.include_subfolders {
                        // Skip system and recursive target folders to avoid loop
                        let is_organizer_folder = organizer_folder_names.iter().any(|target| target == &file_name);
                        if !is_organizer_folder {
                            dir_stack.push(path);
                        }
                    }
                    continue;
                }

                if !path.is_file() {
                    continue;
                }

                let ext = path
                    .extension()
                    .and_then(|e| e.to_str())
                    .map(|e| e.to_lowercase())
                    .unwrap_or_default();

                if let Some((cat_name, target_folder)) = ext_map.get(&ext) {
                    let dest_folder = target_root.join(target_folder);
                    let nominal_dest = dest_folder.join(&file_name);
                    let conflict = nominal_dest.exists();
                    let final_dest = resolve_unique_path(&nominal_dest);

                    let size_bytes = entry.metadata().map(|m| m.len()).unwrap_or(0);
                    let relative_path = path.strip_prefix(root).ok().map(|p| p.to_string_lossy().to_string());

                    previews.push(FilePreview {
                        id: format!("{}:{}", previews.len(), path.display()),
                        file_name,
                        extension: ext,
                        category: cat_name.clone(),
                        source_path: path.to_string_lossy().to_string(),
                        destination_path: final_dest.to_string_lossy().to_string(),
                        size_bytes,
                        conflict_detected: conflict,
                        relative_path,
                    });
                }
            }
        }

        Ok(previews)
    })
    .await
    .map_err(|e| format!("Task execution failed: {}", e))?
}

#[tauri::command]
pub async fn execute_organization(
    items: Vec<FileAction>,
    mode: String,
) -> Result<ExecutionSummary, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let start_time = Instant::now();
        let is_move = mode.to_uppercase() == "MOVE";

        let successful = Arc::new(AtomicUsize::new(0));
        let failed = Arc::new(AtomicUsize::new(0));
        let mut errors = Vec::new();

        // Process sequentially or multi-thread depending on disk strategy
        for item in &items {
            let src = Path::new(&item.source_path);
            let dest = Path::new(&item.destination_path);

            if !src.exists() {
                failed.fetch_add(1, Ordering::Relaxed);
                errors.push(format!("Source does not exist: {}", item.source_path));
                continue;
            }

            // Ensure destination directory exists
            if let Some(parent) = dest.parent() {
                if let Err(e) = fs::create_dir_all(parent) {
                    failed.fetch_add(1, Ordering::Relaxed);
                    errors.push(format!("Failed to create folder {}: {}", parent.display(), e));
                    continue;
                }
            }

            if is_move {
                // Try instant atomic filesystem rename first (same disk volume)
                match fs::rename(src, dest) {
                    Ok(_) => {
                        successful.fetch_add(1, Ordering::Relaxed);
                    }
                    Err(rename_err) => {
                        // FastCopy-style 8MB buffered copy across physical drive boundaries
                        match fast_buffered_copy(src, dest) {
                            Ok(copied_bytes) => {
                                let src_size = fs::metadata(src).map(|m| m.len()).unwrap_or(0);
                                if copied_bytes == src_size {
                                    if let Err(del_err) = fs::remove_file(src) {
                                        errors.push(format!(
                                            "Copied but could not remove source {}: {}",
                                            item.source_path, del_err
                                        ));
                                    }
                                    successful.fetch_add(1, Ordering::Relaxed);
                                } else {
                                    let _ = fs::remove_file(dest);
                                    failed.fetch_add(1, Ordering::Relaxed);
                                    errors.push(format!(
                                        "Integrity check failed for {}: expected {} bytes, copied {}",
                                        item.source_path, src_size, copied_bytes
                                    ));
                                }
                            }
                            Err(copy_err) => {
                                failed.fetch_add(1, Ordering::Relaxed);
                                errors.push(format!(
                                    "Move failed for {}: (rename: {}, copy: {})",
                                    item.source_path, rename_err, copy_err
                                ));
                            }
                        }
                    }
                }
            } else {
                // High-performance buffered copy
                match fast_buffered_copy(src, dest) {
                    Ok(_) => {
                        successful.fetch_add(1, Ordering::Relaxed);
                    }
                    Err(e) => {
                        failed.fetch_add(1, Ordering::Relaxed);
                        errors.push(format!("Copy failed for {}: {}", item.source_path, e));
                    }
                }
            }
        }

        let elapsed = start_time.elapsed().as_millis() as u64;

        Ok(ExecutionSummary {
            total_processed: items.len(),
            successful: successful.load(Ordering::Relaxed),
            failed: failed.load(Ordering::Relaxed),
            time_taken_ms: elapsed,
            mode,
            errors,
        })
    })
    .await
    .map_err(|e| format!("Organization thread failed: {}", e))?
}

#[tauri::command]
pub async fn get_folder_stats(path: String) -> Result<DirectoryStats, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let p = Path::new(&path);
        if !p.is_dir() {
            return Err("Path is not a directory".to_string());
        }

        let mut count = 0;
        let mut total_size = 0;

        if let Ok(entries) = fs::read_dir(p) {
            for entry in entries.flatten() {
                if let Ok(meta) = entry.metadata() {
                    if meta.is_file() {
                        count += 1;
                        total_size += meta.len();
                    }
                }
            }
        }

        Ok(DirectoryStats {
            path,
            total_files: count,
            total_size_bytes: total_size,
        })
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
fn greet(name: &str) -> String {
    format!("Blaze Engine ready. Hello, {}!", name)
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




