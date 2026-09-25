# Blaze File Organizer — AI Context

## Project Overview

- Desktop utility that previews files in a selected directory, classifies them by extension, then moves or copies selected files into category folders.
- The product is a Tauri 2 shell around a vanilla TypeScript/HTML/CSS interface; it is not React/Vue.

## Technology Stack

| Layer | Technology |
|---|---|
| UI | Vanilla TypeScript, static `index.html`, CSS, canvas-confetti |
| Desktop bridge | Tauri 2, Dialog and Opener plugins |
| Native engine | Rust 2021, serde, serde_json |
| Tooling | Vite 8, TypeScript 6 |

## Repository Structure

- `src/main.ts` — all frontend state, DOM orchestration, modal dialogs, and Tauri IPC calls.
- `index.html` / `src/style.css` — static application shell, modal structure, and styling.
- `src-tauri/src/lib.rs` — Tauri commands, filesystem engine, collision avoidance, and transfer tests.
- `src-tauri/src/main.rs` — native binary entry point calling `blaze_file_organizer_lib::run`.
- `src-tauri/tauri.conf.json` / `src-tauri/Cargo.toml` — desktop, CSP security, and Rust configuration.
- `vite.config.ts` — Vite dev server (port 3000) and HMR configuration.

## Architecture

- One imperative UI module owns client state and renders category cards, filters, table, dock, and modals directly into the DOM using safe DOM APIs.
- Rust is a stateless command service running blocking operations inside `spawn_blocking` off the async runtime.
- Scan produces advisory preview DTOs; execution consumes action DTOs and performs atomic, no-overwrite transfers with guaranteed collision prevention.
- `run` registers plugins (`tauri-plugin-dialog`, `tauri-plugin-opener`) and command handlers (`greet`, `scan_directory`, `execute_organization`, `get_folder_stats`). No database, persistence layer, or shared state exists.

## Frontend

- `src/main.ts — DEFAULT_CATEGORIES`: built-in extension rules; copied into mutable in-memory `categories`.
- `src/main.ts — setupEventListeners`: connects folder choice, rules, filtering, selection, and execution UI.
- `src/main.ts — triggerScan`: builds `ScanRules`, calls native scan, replaces `allPreviews`, selects all results.
- `src/main.ts — renderCategoryCards` / `renderTable` / `renderCustomTags`: programmatic DOM rendering of rules and previews avoiding `innerHTML` interpolation.
- `src/main.ts — handleExecute` / `finishExecution`: submits selected actions, displays actual success/failure stats, and updates previews with finalized destination paths.
- `src/main.ts — closeExecutionModal`: ensures modal close via 'X' or 'Done' button triggers a UI rescan so file state does not remain stale.

## Tauri/Rust Backend

- `src-tauri/src/lib.rs — scan_directory`: async command running blocking traversal; filters dotfiles and Windows `FILE_ATTRIBUTE_HIDDEN` attributes, skips organizer folders during recursion, skips files already organized at destination (`is_same_file`), and reports `skipped_paths` for unreadable directories or metadata.
- `src-tauri/src/lib.rs — resolve_preview_destination`: derives advisory `_N` preview names only.
- `src-tauri/src/lib.rs — copy_to_unique_destination` / `transfer_item`: execution-time collision reservation and guarded move/copy engine. On Windows, copy uses `CopyFileExW` (via `copy_file_native`) instead of the portable 8 MiB read/write loop; the same temp-file-safe publish chain applies on all platforms.
- `src-tauri/src/lib.rs — copy_file_native`: Windows-only (`#[cfg(target_os = "windows")]`) wrapper around `CopyFileExW`; copies source to the `.blaze-part-*` temp path, then `verify_copy_completeness` and `publish_temporary_file` run unchanged.
- `src-tauri/src/lib.rs — try_same_filesystem_move`: link+unlink same-volume move with pre-deletion size checks.
- `src-tauri/src/lib.rs — copy_then_remove_source`: guarded copy+unlink for cross-volume transfers and filesystems without hard-link support.
- `src-tauri/src/lib.rs — is_same_file`: detects self-collision (source is already at nominal destination) and skips/preserves without creating `_1` suffixes.
- `src-tauri/src/lib.rs — run`: registers commands and plugins.

## Frontend ↔ Backend Communication

- `src/main.ts` imports `invoke` from `@tauri-apps/api/core`, `listen` from `@tauri-apps/api/event`, and dialog `open` from `@tauri-apps/plugin-dialog`.
- `invoke("scan_directory", { sourceDir, rules })` maps camel-case call arguments to Rust `source_dir` / `ScanRules` and returns `ScanResult` (`previews` plus `skipped_paths`).
- `invoke("execute_organization", { items, mode })` sends selected `FileAction[]` and gets `ExecutionSummary`, including actual destinations in `completed`. Accepts `AppHandle` injected by Tauri to emit `transfer-progress` events.
- `listen("transfer-progress", ...)` in `handleExecute` receives `TransferProgress` (`file_index`, `total_files`) events emitted by the backend after each file completes, updating the progress bar in real time.
- Shared serializable DTO contracts live in `src/main.ts` interfaces and `src-tauri/src/lib.rs` structs; field names are aligned.

## File Organization / Transfer Flow

1. `handleBrowseFolder` opens native folder picker; `triggerScan` executes and also triggers after rule/filter changes and modal close.
2. `scan_directory` maps extensions to categories, walks source root (optionally subfolders), skips hidden files (dotfiles and Windows `FILE_ATTRIBUTE_HIDDEN` attributes) unless enabled, skips organizer folders during recursion, and skips files already residing at their intended destination.
3. Matching files become previews with source, advisory destination, size, relative path, and initial conflict flag; all are selected by default. Scan continues around unreadable paths and returns skipped paths.
4. `handleExecute` converts selected previews to actions and invokes `execute_organization` in `MOVE` or `COPY` mode.
5. Executor atomically claims final names at execution time. Same-volume move uses link+unlink; fallback copy writes a private temp file, validates byte count/size, atomically publishes it (with safe non-overwriting fallback for FAT32/exFAT/USB volumes), then deletes source only after rechecks. Source already at destination is preserved without renaming.

## Important Files & Symbols

| File | Symbols / purpose |
|---|---|
| `src/main.ts` | `triggerScan`, `renderTable`, `handleExecute`, `finishExecution`, frontend DTOs/state |
| `src-tauri/src/lib.rs` | `scan_directory`, `execute_organization`, `publish_temporary_file`, `transfer_item`, `is_hidden_path`, `is_same_file`, `run` |
| `src-tauri/src/main.rs` | binary entry point |
| `src-tauri/tauri.conf.json` | Vite lifecycle, window, bundle, CSP configuration |
| `package.json` | JS commands and Tauri dependencies |
| `vite.config.ts` | fixed dev port 3000; Tauri HMR host support |

## Current Functionality

- Editable categories and custom extensions, category filters, text search, sortable selectable preview table, move/copy mode, and an execution modal are implemented.
- Defaults include common Images, Documents, Videos, Audio, Archives, Code, and Other categories.
- Category/custom-extension edits are session-only; no settings persistence is implemented.
- `get_folder_stats` is registered but currently unused by the UI.
- Collision avoidance and non-overwriting guarantees apply across NTFS, APFS, ext4, FAT32, exFAT, and USB drives.
- Safe DOM construction prevents XSS/injection vulnerabilities.
- CSP allows local HMR (`ws://localhost:3000` and `ws://localhost:1421`).

## Completed Phase 1 Correctness & Safety Work

1. **CSP/HMR**: Added `ws://localhost:3000` to `connect-src` in `tauri.conf.json` to support local Vite HMR alongside `ws://localhost:1421`.
2. **Windows Hidden Files**: Implemented `is_hidden_path` checking Windows `FILE_ATTRIBUTE_HIDDEN` via `std::os::windows::fs::MetadataExt` behind `#[cfg(target_os = "windows")]`, while preserving cross-platform dotfile fallback.
3. **Self-Collision**: Added `is_same_file` check in `scan_directory` (skips files already at destination) and in `transfer_item` (safely succeeds without creating `_1` or unlinking). Genuinely distinct files with colliding names remain collision-protected.
4. **Destination Publishing Fallback**: Added `publish_fallback_no_overwrite` utilizing `MoveFileExW` without overwrite flag on Windows and exclusive create-and-stream fallback on other filesystems, ensuring FAT32, exFAT, and USB flash drives publish safely without overwriting.
5. **Execution UI Refresh**: Ensured both `execModalClose` and `execDoneBtn` close the modal and trigger `triggerScan()`, preventing stale table states after execution.
6. **Critical Correctness Tests**: 10 unit tests in `src-tauri/src/lib.rs` verifying advisory names, collision resolution, duplicate batches, stale scan destinations, failed copy preservation, same-volume moves, cross-volume fallback moves, self-collision preservation, destination publishing fallbacks, and Windows hidden file detection.

## Completed Phase 2 — Windows-Native Transfer Backend

7. **CopyFileExW copy path**: On Windows, `copy_to_unique_destination` now calls `copy_file_native` which uses `CopyFileExW` (the same Win32 API Explorer uses). The empty `.blaze-part-*` temp file is created to reserve the name, its handle is dropped, then `CopyFileExW` writes the file, and the existing `verify_copy_completeness` + `publish_temporary_file` chain runs unchanged. Non-Windows builds retain the portable 8 MiB buffered copy path.
8. **Real-time progress events**: `execute_organization` now accepts a `tauri::AppHandle` (injected by Tauri) and emits a `transfer-progress` event (payload: `{ file_index, total_files }`) after each file completes. `handleExecute` in `main.ts` subscribes with `listen()` before the invoke and unlistens in `finally`.
9. **cfg-guarded dead code**: `FAST_COPY_BUFFER_SIZE`, `Read` import, and `copy_source_to_file` are now `#[cfg(not(target_os = "windows"))]`-guarded — zero dead_code warnings on Windows, zero compilation errors on other platforms.

## Known Issues / Deferred to Later Phases

- Size-only copy checks are completeness checks, not cryptographic content verification; stronger optional hashing (SHA-256) is deferred.
- Skipped scan paths trigger a toast and console warning; a dedicated UI viewer for all skipped paths is deferred.
- Integration tests for Tauri command IPC contracts and automated end-to-end frontend tests are deferred.
- `CopyFileExW` cancellation hook is wired (null cancel pointer for now); an active cancel mechanism requires a future Tauri command and shared `AtomicBool`.

## Architectural Constraints

- Preserve the JSON field contract: frontend uses snake_case payload fields matching Rust serde structs.
- Keep filesystem work inside `spawn_blocking`; do not block the Tauri async runtime.
- Do not follow directory symlinks during recursive scans; this avoids loops.
- Never overwrite destination data: maintain collision protection at execution time, not only preview time; preview paths are advisory.
- Release Cargo profile favors size/performance (`lto`, `opt-level=3`, `panic=abort`, strip).
- Windows copy path uses `CopyFileExW`; do not add a large buffer pool or custom I/O scheduler. The temp-file-publish safety chain must remain intact for any copy implementation.

## Development Guidelines

- Use `npm run dev` for browser UI; use `npm run tauri dev` for the desktop app. `npm run build` runs `tsc && vite build`; `npm run lint` is `tsc --noEmit`; `npm run tauri build` packages the app.
- Tauri expects Vite port 3000 and `dist` output; retain matching values in `vite.config.ts` and `tauri.conf.json`.
- CSP permits `ipc:`/`http://ipc.localhost` for Tauri commands, the local HMR WebSockets (`ws://localhost:3000` and `ws://localhost:1421`), asset protocols, and `'unsafe-inline'` styles for existing inline UI style mutations; do not broaden scripts.
- Keep frontend transport types and Rust serde structs in sync when adding a command or field.
- Prefer structured DOM APIs or escaping for any filesystem/user-provided string rendered by the UI.

## AI Handoff Notes

- Start feature work in `src/main.ts` and `src-tauri/src/lib.rs`; they are the core engine and dominant change surface.
- Treat scan results as advisory: filesystem state can change before execute.
- Verify both same-volume rename and cross-volume copy/delete paths when changing transfers.
- `README.md` remains the stock Tauri template and needs product-specific documentation.

Last analyzed: 2026-09-23 (Phase 2 complete)
