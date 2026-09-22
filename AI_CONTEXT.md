# Blaze File Organizer — AI Context

## Project Overview

- Desktop utility that previews files in a selected directory, classifies them by extension, then moves or copies selected files into category folders.
- The product is a Tauri 2 shell around a vanilla TypeScript/HTML/CSS interface; it is not React/Vue.

## Technology Stack

| Layer | Technology |
|---|---|
| UI | Vanilla TypeScript, static `index.html`, CSS, canvas-confetti |
| Desktop bridge | Tauri 2, Dialog and Opener plugins |
| Native engine | Rust 2021, serde |
| Tooling | Vite 8, TypeScript 6 |

## Repository Structure

- `src/main.ts` — all frontend state, DOM orchestration, and Tauri calls.
- `index.html` / `src/style.css` — static application shell and styling.
- `src-tauri/src/lib.rs` — Tauri commands and filesystem engine.
- `src-tauri/src/main.rs` — native binary entry point calling `blaze_file_organizer_lib::run`.
- `src-tauri/tauri.conf.json` / `src-tauri/Cargo.toml` — desktop and Rust configuration.

## Architecture

- One large imperative UI module owns state and renders category cards, filters, table, dock, and modals directly into the DOM.
- Rust is a stateless command service: scan produces preview DTOs; execution consumes selected action DTOs.
- `run` registers plugins and the command allowlist; no database, persistence layer, or shared Tauri state exists.

## Frontend

- `src/main.ts — DEFAULT_CATEGORIES`: built-in extension rules; copied into mutable in-memory `categories`.
- `src/main.ts — setupEventListeners`: connects folder choice, rules, filtering, selection, and execution UI.
- `src/main.ts — triggerScan`: builds `ScanRules`, calls native scan, replaces `allPreviews`, selects all results.
- `src/main.ts — renderCategoryCards` / `renderTable`: direct DOM rendering of rules and previews.
- `src/main.ts — handleExecute` / `finishExecution`: submits selected actions and shows summary/log/confetti.

## Tauri/Rust Backend

- `src-tauri/src/lib.rs — scan_directory`: async command which runs blocking filesystem traversal off the async runtime.
- `src-tauri/src/lib.rs — execute_organization`: sequential move/copy executor returning `ExecutionSummary`.
- `src-tauri/src/lib.rs — resolve_preview_destination`: derives advisory `_N` preview names only.
- `src-tauri/src/lib.rs — copy_to_unique_destination` / `transfer_item`: execution-time collision reservation and guarded move/copy engine.
- `src-tauri/src/lib.rs — run`: registers `greet`, `scan_directory`, `execute_organization`, `get_folder_stats`, dialog, and opener.

## Frontend ↔ Backend Communication

- `src/main.ts` imports `invoke` from `@tauri-apps/api/core` and dialog `open` from `@tauri-apps/plugin-dialog`.
- `invoke("scan_directory", { sourceDir, rules })` maps camel-case call arguments to Rust `source_dir` / `ScanRules` and returns `ScanResult` (`previews` plus `skipped_paths`).
- `invoke("execute_organization", { items, mode })` sends selected `FileAction[]` and gets `ExecutionSummary`, including actual destinations in `completed`.
- Shared serializable DTO contracts live redundantly in `src/main.ts` interfaces and `src-tauri/src/lib.rs` structs; keep field names aligned.

## File Organization / Transfer Flow

1. `handleBrowseFolder` uses native dialog; `triggerScan` also runs after rule/filter changes.
2. `scan_directory` builds extension → category/folder map, walks root (optionally subfolders), skips dotfiles unless enabled, and skips organizer folders during recursion.
3. Matching files become previews with source, advisory destination, size, relative path, and initial conflict flag; all are selected by default. Scan continues around unreadable paths and returns warnings.
4. `handleExecute` converts selected previews to actions and invokes `execute_organization` in `MOVE` or `COPY` mode.
5. Executor atomically claims final names at execution time. Same-volume move uses link+unlink; fallback copy writes a private temp file, validates byte count/size, atomically publishes it, then deletes source only after rechecks.

## Important Files & Symbols

| File | Symbols / purpose |
|---|---|
| `src/main.ts` | `triggerScan`, `renderTable`, `handleExecute`, frontend DTOs/state |
| `src-tauri/src/lib.rs` | `scan_directory`, `execute_organization`, `fast_buffered_copy`, `run` |
| `src-tauri/src/main.rs` | binary entry point |
| `src-tauri/tauri.conf.json` | Vite lifecycle, window, bundle, CSP configuration |
| `package.json` | JS commands and Tauri dependencies |
| `vite.config.ts` | fixed dev port 3000; Tauri HMR host support |

## Current Functionality

- Editable categories and custom extensions, category filters, text search, sortable selectable preview table, move/copy mode, and an execution modal are implemented.
- Defaults include common Images, Documents, Videos, Audio, Archives, Code, and Other categories.
- Category/custom-extension edits are session-only; no settings persistence is implemented.
- `get_folder_stats` is registered but currently unused by the UI.

## Known Issues / TODOs

- Size-only copy checks are completeness checks, not cryptographic content verification; a stronger optional verifier is intentionally deferred.
- Hard-link publication targets normal NTFS/APFS/ext-family filesystems. Filesystems without hard-link support currently fail safely rather than overwrite a destination.
- Hidden-file filtering remains dotfile-only; platform-specific Windows hidden attributes are not yet covered.
- No end-to-end Tauri/UI test suite exists; Rust unit tests cover reservation, collisions, stale state, and guarded move/copy behavior.

## Architectural Constraints

- Preserve the JSON field contract: frontend uses snake_case payload fields matching Rust serde structs.
- Keep filesystem work inside `spawn_blocking`; do not block the Tauri async runtime.
- Do not follow directory symlinks during recursive scans; this avoids loops.
- Never overwrite destination data: maintain collision protection at execution time, not only preview time; preview paths are advisory.
- Release Cargo profile favors size/performance (`lto`, `opt-level=3`, `panic=abort`, strip).

## Development Guidelines

- Use `npm run dev` for browser UI; use `npm run tauri dev` for the desktop app. `npm run build` runs `tsc && vite build`; `npm run lint` is `tsc --noEmit`; `npm run tauri build` packages the app.
- Tauri expects Vite port 3000 and `dist` output; retain matching values in `vite.config.ts` and `tauri.conf.json`.
- CSP permits `ipc:`/`http://ipc.localhost` for Tauri commands, the local HMR WebSocket, asset protocols, and `'unsafe-inline'` styles for existing inline UI style mutations; do not broaden scripts.
- Keep frontend transport types and Rust serde structs in sync when adding a command or field.
- Prefer structured DOM APIs or escaping for any filesystem/user-provided string rendered by the UI.

## AI Handoff Notes

- Start feature work in `src/main.ts` and `src-tauri/src/lib.rs`; they are the core engine and dominant change surface.
- Treat scan results as advisory: filesystem state can change before execute.
- Verify both same-volume rename and cross-volume copy/delete paths when changing transfers.
- `README.md` remains the stock Tauri template and needs product-specific documentation.

Last analyzed: 2026-09-22

`AI_CONTEXT.md` is a living document and should be updated when significant implementation or architectural changes are made.
