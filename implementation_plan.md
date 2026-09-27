# Phase 2: Windows-Native File Transfer Backend

Replace the 8 MiB buffered copy loop with `CopyFileExW` on Windows, keeping all existing
safety invariants intact. This makes Blaze's throughput roughly comparable to Windows Explorer
for typical workloads without introducing complex I/O infrastructure.

---

## Background

The current copy path in `copy_source_to_file` uses an 8 MiB read/write loop with
`sync_all()` at the end. `CopyFileExW` is the same API Windows Explorer uses internally;
it uses internally-tuned, OS-managed buffering and is generally faster on Windows for
large sequential files and same-volume copies.

No new Cargo dependencies are needed — we declare the Win32 function with
`extern "system"` exactly as already done for `MoveFileExW` and `SetFileAttributesW`.

---

## User Review Required

> [!IMPORTANT]
> **Temp-file copy strategy**: `CopyFileExW` copies directly to a path we supply.
> We will point it at the `.blaze-part-*` temporary path (already created by
> `create_temporary_file`), not the final destination. After the native copy
> completes, the same `verify_copy_completeness` + `publish_temporary_file` chain
> runs as before. This preserves all Phase 1 safety guarantees.

> [!IMPORTANT]
> **Progress reporting**: `CopyFileExW` fires a `CopyProgressRoutine` callback with
> `TotalBytesTransferred` and `TotalFileSize` during the copy. We will use this to
> emit Tauri events (`transfer-progress`) per file. The frontend currently shows
> only the simulated interval-based progress bar in dev mode; in Tauri mode it jumps
> to 100% when `execute_organization` returns. Emitting per-file progress events will
> let us update the progress bar and ratio incrementally *while files are being copied*.
> This requires a small addition to the frontend to listen for these events.

> [!IMPORTANT]
> **Cancellation**: `CopyFileExW` supports cancellation via a `pbCancel: *mut i32`
> pointer that we can set to 1 from a Tauri command. However, the current execution
> flow has no cancellation mechanism. For this phase we will pass a null cancel
> pointer (no cancellation) and note the hook for a future phase.

---

## Open Questions

None blocking implementation. The strategy above is unambiguous given the existing code.

---

## Proposed Changes

### Backend (`src-tauri/src/lib.rs`)

#### [MODIFY] [lib.rs](file:///c:/Github/My-GUI-Applications/Blaze-File-Organizer/blaze-file-organizer/src-tauri/src/lib.rs)

**1. New Windows-only `CopyProgressState` and `copy_file_ex_w` wrapper** (inside `#[cfg(target_os = "windows")]`)

- Declare `CopyFileExW` with `extern "system"`, matching the MSDN signature.
- Accept a closure/callback for progress. The `LPPROGRESS_ROUTINE` C callback will
  forward `TotalBytesTransferred` to an `AtomicU64` that a Tauri event-emitter function
  reads.
- Return `CopyOutcome` (expected_size + copied_bytes) so the existing
  `verify_copy_completeness` call can remain unchanged.

**2. Replace `copy_source_to_file` on Windows**

The function `copy_source_to_file(source, writer)` currently opens `source` and
streams into an already-open `writer` file handle. `CopyFileExW` copies by path, not
handle. The cleanest approach:

- Add a **new Windows-only function**:
  ```rust
  #[cfg(target_os = "windows")]
  fn copy_file_native(source: &Path, dest_path: &Path, progress_fn: impl Fn(u64, u64))
      -> io::Result<CopyOutcome>
  ```
- In `copy_to_unique_destination` (Windows only): instead of calling
  `copy_source_to_file(source, &mut temporary_file)`, close the temporary file
  handle after creation (we only needed it to atomically reserve the path), then
  call `copy_file_native(source, &temporary_path, …)`.
- On non-Windows: no change; existing `copy_source_to_file` remains.

**3. Tauri app handle for progress events**

- Change `execute_organization` to accept a `tauri::AppHandle`.
- Pass the handle into `execute_organization_blocking` (or thread-local).
- From the `CopyFileExW` progress callback, emit a `transfer-progress` Tauri event:
  ```json
  { "file_index": 3, "total_files": 12, "bytes_done": 1048576, "bytes_total": 10485760 }
  ```
- Keep this optional / guarded: if the emit fails, ignore and continue.

**4. No changes to**: `publish_temporary_file`, `publish_candidate`,
`try_same_filesystem_move`, `verify_copy_completeness`, or any test.

---

### Frontend (`src/main.ts`)

#### [MODIFY] [main.ts](file:///c:/Github/My-GUI-Applications/Blaze-File-Organizer/blaze-file-organizer/src/main.ts)

Add a Tauri event listener for `transfer-progress` before the `invoke` call, and
remove it after. On each event update the progress bar and ratio text in real time.

```typescript
// listen for per-file progress emitted by the native copy callback
const unlisten = await listen<TransferProgress>("transfer-progress", (event) => {
  const { file_index, total_files } = event.payload;
  const pct = Math.round(((file_index) / total_files) * 100);
  execPercentage.textContent = `${pct}%`;
  execRatio.textContent = `${file_index} / ${total_files}`;
  execBarFill.style.width = `${pct}%`;
});
// ... invoke execute_organization ...
unlisten();
```

This is additive; if no events arrive (non-Windows build or < 1 file) the bar stays
at 0% until `finishExecution` sets it to 100%.

---

## Verification Plan

### Automated Tests

```
cargo test --manifest-path src-tauri/Cargo.toml
```
All existing tests must pass unchanged.

### Build Verification

```
cargo build --manifest-path src-tauri/Cargo.toml
npm run tauri build -- --debug
```

### Manual Verification

1. Run `npm run tauri dev` and organize a folder with 20+ files. Verify progress bar
   updates while files are copying (not just at the end).
2. Test with a large file (> 100 MB) to see the progress callback fire mid-file.
3. Test copy, move (same volume), and move (cross-volume) each produce correct results.
4. Verify no files are overwritten (existing collision-resolution behavior preserved).

### Benchmark (informational, not a gate)

Run `cargo bench` (or a simple `hyperfine`) comparing copy throughput for:
- 100 × 1 MB files same volume
- 1 × 500 MB file same volume

Record results in `AI_CONTEXT.md`. Do not claim a performance improvement without data.
