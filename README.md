<div align="center">



\# Blaze File Organizer



\*\*A fast, privacy-friendly desktop file organizer for Windows\*\*



Organize files by type with a preview-first workflow, collision-safe transfers, and a native Windows copy backend.



<p>

&nbsp; <img src="https://img.shields.io/badge/Platform-Windows%20x64-0078D4?style=flat-square\&logo=windows\&logoColor=white" alt="Windows x64" />

&nbsp; <img src="https://img.shields.io/badge/Tauri-2-FFC131?style=flat-square\&logo=tauri\&logoColor=black" alt="Tauri 2" />

&nbsp; <img src="https://img.shields.io/badge/Rust-2021-000000?style=flat-square\&logo=rust\&logoColor=white" alt="Rust 2021" />

&nbsp; <img src="https://img.shields.io/badge/TypeScript-Vanilla-3178C6?style=flat-square\&logo=typescript\&logoColor=white" alt="TypeScript" />

</p>



</div>



---



\## Blaze vs. File Juggler \& DropIt



For the core \*\*scan → organize → transfer\*\* workflow, Blaze is designed as a focused, lightweight Windows desktop organizer.



| Capability | Blaze File Organizer | File Juggler | DropIt |

|---|:---:|:---:|:---:|

| \*\*Transfer speed\*\* | 🟢 \*\*Extremely Fast\*\* | 🔴 Very slow | 🔴 Slow |

| \*\*Windows-native `CopyFileExW` transfer backend\*\* | 🟢 \*\*Yes\*\* | ❌ | ❌ |

| \*\*Preview-first organization workflow\*\* | 🟢 \*\*Yes\*\* | ❌ | ❌ |

| \*\*Collision-safe transfer with no blind overwrites\*\* | 🟢 \*\*Yes\*\* | ❌ | ❌ |



> \*\*Performance note:\*\* Transfer speed above is based on direct testing of Blaze against the compared applications. Actual performance depends on storage hardware, file sizes, file counts, filesystem, and workload.



---



\## Overview



Blaze File Organizer scans a selected directory, classifies files by extension, previews the proposed changes, and then moves or copies the selected files into organized category folders.



The application is designed around a simple principle:



> \*\*Preview first. Transfer second.\*\*



Blaze does not blindly reorganize a directory. It scans the filesystem, shows what will happen, lets you review the changes, and performs collision-safe transfers only after confirmation.



---



\## Features



| Feature | Description |

|---|---|

| \*\*Preview-first organization\*\* | Scan a directory and review proposed destinations before making changes. |

| \*\*Move \& Copy\*\* | Choose whether files should be moved or copied into category folders. |

| \*\*Category-based rules\*\* | Organize common file types into Images, Documents, Videos, Audio, Archives, Code, and other categories. |

| \*\*Custom extensions\*\* | Edit category rules and add custom file extensions with persistent configuration. |

| \*\*Subfolder scanning\*\* | Optionally scan files inside subdirectories. |

| \*\*Collision protection\*\* | Existing files are never blindly overwritten; conflicting names are resolved safely. |

| \*\*Execution-time safety\*\* | Final destinations are resolved again when execution begins, protecting against filesystem changes after the preview. |

| \*\*Copy verification\*\* | Copied files are checked for complete byte/size transfer before they are published. |

| \*\*Safe MOVE operations\*\* | A source is removed only after the destination copy has completed and passed verification. |

| \*\*Windows-native transfers\*\* | Windows uses the native `CopyFileExW` API for file copying. |

| \*\*Transfer progress\*\* | The execution dialog reports transfer progress while files are being processed. |

| \*\*Hidden-file handling\*\* | Windows hidden attributes and dot-prefixed files can be excluded from scans unless hidden files are enabled. |

| \*\*Safe DOM rendering\*\* | Filesystem-provided names are rendered using safe DOM APIs rather than interpolated HTML. |

| \*\*No database required\*\* | Blaze has no database or cloud service; organization state is handled locally. |



---



\## How It Works



```text

Select Folder

&nbsp;     │

&nbsp;     ▼

&nbsp;   Scan

&nbsp;     │

&nbsp;     ▼

Preview proposed destinations

&nbsp;     │

&nbsp;     ▼

Review / filter / select files

&nbsp;     │

&nbsp;     ▼

Choose COPY or MOVE

&nbsp;     │

&nbsp;     ▼

Resolve final destinations

&nbsp;     │

&nbsp;     ▼

Collision-safe transfer

&nbsp;     │

&nbsp;     ▼

Verify completed copies

&nbsp;     │

&nbsp;     ▼

Publish destination

&nbsp;     │

&nbsp;     ▼

Refresh the file view

```



The scan is advisory: the filesystem can change between preview and execution. Blaze therefore performs destination checks and collision resolution again at execution time.



---



\## Transfer Safety



File operations are deliberately designed to avoid destructive behavior.



\### COPY



On Windows:



```text

Source

&nbsp; │

&nbsp; ▼

CopyFileExW

&nbsp; │

&nbsp; ▼

Temporary .blaze-part-\* file

&nbsp; │

&nbsp; ▼

Size / completeness verification

&nbsp; │

&nbsp; ▼

Safe destination publication

```



\### MOVE



For a same-filesystem move, Blaze uses the existing safe move path.



For a cross-volume transfer:



```text

Source

&nbsp; │

&nbsp; ▼

Copy to temporary destination

&nbsp; │

&nbsp; ▼

Verify copy

&nbsp; │

&nbsp; ▼

Publish destination

&nbsp; │

&nbsp; ▼

Remove source

```



The source is not removed when the copy fails.



Existing destination files are preserved and collisions are resolved with suffixes such as:



```text

photo.jpg

photo\_1.jpg

photo\_2.jpg

```



Blaze also detects when a file is already at its intended destination so that repeatedly organizing the same directory does not unnecessarily create new copies.



> \*\*Verification note:\*\* current copy verification checks transfer completeness using file size/byte counts. It does not perform cryptographic SHA-256 or BLAKE3 content hashing.



---



\## Screenshots



!\[Blaze File Organizer](screenshots/photo\_1.jpg)



---



\## Installation



\### Windows



Download the latest Windows installer from the repository's \*\*Releases\*\* page.



Blaze currently targets \*\*Windows x64\*\*.



Two installer formats can be produced by the Tauri build:



| Package | Format | Use |

|---|---|---|

| NSIS | `.exe` | Standard Windows installer |

| MSI | `.msi` | Windows Installer package |



The portable release executable is also generated during development builds, but the installer is recommended for normal installation.



---



\## Quick Start



1\. Launch Blaze File Organizer.

2\. Select the folder you want to organize.

3\. Choose whether subfolders should be included.

4\. Review the detected files and proposed categories.

5\. Use filters or search to narrow the selection if needed.

6\. Select \*\*COPY\*\* or \*\*MOVE\*\*.

7\. Review the execution summary.

8\. Start the operation.

9\. Verify the resulting category folders.





---



\## Default Categories



Blaze currently includes categories for common file types such as:



| Category | Examples |

|---|---|

| \*\*Images\*\* | JPG, JPEG, PNG, GIF, WEBP and other image formats |

| \*\*Documents\*\* | PDF, DOC, DOCX, TXT and common document formats |

| \*\*Videos\*\* | MP4, MKV, AVI, MOV and common video formats |

| \*\*Audio\*\* | MP3, FLAC, WAV and common audio formats |

| \*\*Archives\*\* | ZIP, RAR, 7Z and common archive formats |

| \*\*Code\*\* | Common source-code and development file extensions |

| \*\*Other\*\* | Category available for file types outside the main groups |



Category and extension edits are persisted between launches, data is stored in Appdata/Roaming.



---



\## Tech Stack



| Layer | Technology |

|---|---|

| Desktop framework | \[Tauri 2](https://tauri.app/) |

| Frontend | Vanilla TypeScript, HTML, CSS |

| Backend | Rust 2021 |

| Build tooling | Vite 8 |

| Serialization | Serde / serde\_json |

| Windows transfer API | Win32 `CopyFileExW` |

| Desktop plugins | Tauri Dialog / Opener |

| Package formats | NSIS / MSI |



Blaze intentionally avoids a large frontend framework. The interface is implemented with vanilla TypeScript and direct DOM APIs.



---



\## Architecture



```text

┌─────────────────────────────────────────────┐

│                  Blaze UI                   │

│                                             │

│  Scan · Preview · Filter · Select · Execute │

└──────────────────────┬──────────────────────┘

&nbsp;                      │ Tauri IPC

&nbsp;                      ▼

┌─────────────────────────────────────────────┐

│               Rust Backend                  │

│                                             │

│  Directory Scan                             │

│  Category Matching                          │

│  Collision Resolution                       │

│  Transfer Safety                            │

│  Progress Events                            │

└──────────────────────┬──────────────────────┘

&nbsp;                      │

&nbsp;                      ▼

┌─────────────────────────────────────────────┐

│             Windows Filesystem              │

│                                             │

│  CopyFileExW · Rename/Move · File Metadata  │

└─────────────────────────────────────────────┘

```



Filesystem work is kept away from the Tauri async runtime using blocking worker execution.



The frontend and Rust backend communicate through explicit serializable DTOs.



---



\## Project Structure



```text

blaze-file-organizer/

├── src/

│   ├── main.ts              # Frontend state, rendering and Tauri calls

│   └── styles.css           # Application styling

│

├── src-tauri/

│   ├── src/

│   │   ├── lib.rs           # Tauri commands, scanner and transfer engine

│   │   └── main.rs          # Native application entry point

│   ├── Cargo.toml           # Rust dependencies

│   └── tauri.conf.json      # Tauri configuration

│

├── index.html

├── vite.config.ts

├── package.json

├── AI\_CONTEXT.md            # Development context / architecture notes

└── README.md

```



---



\## Development



\### Prerequisites



Install:



\- Node.js

\- npm

\- Rust

\- Tauri 2 development prerequisites for Windows



Refer to the \[Tauri prerequisites](https://v2.tauri.app/start/prerequisites/) documentation for the current Windows setup requirements.



\### Install dependencies



```powershell

npm install

```



\### Run the frontend



```powershell

npm run dev

```



\### Run the desktop application



```powershell

npm run tauri dev

```



\### Build the production application



```powershell

npm run tauri build

```



The generated Windows packages are placed under:



```text

src-tauri/target/release/bundle/

```



---



\## Validation



The project currently uses a small set of focused Rust tests for transfer correctness and safety.



Current validation includes:



| Check | Purpose |

|---|---|

| `cargo fmt --check` | Rust formatting |

| `cargo check` | Rust compilation/type checking |

| `cargo clippy` | Rust linting |

| `cargo test` | Transfer safety tests |

| `npm run lint` | TypeScript type checking |

| `npm run build` | Frontend production build |



The transfer tests cover collision handling, failed-copy source preservation, same-filesystem moves, cross-volume moves, self-collision, destination publishing, duplicate destinations, and Windows hidden-file detection.



---



\## Current Scope



Blaze is currently developed as a \*\*Windows-first desktop application\*\*.



The transfer implementation uses Windows-native APIs where appropriate. The project does not currently include dedicated Linux or macOS transfer backends.



The application is intentionally focused on:



\- local filesystem organization

\- predictable file operations

\- safe collision handling

\- simple category-based organization

\- a lightweight desktop UI



---



\## Roadmap



Planned or optional future work includes:



\- \[ ] Dedicated skipped-path viewer

\- \[ ] Active transfer cancellation

\- \[ ] Optional cryptographic file verification

\- \[ ] Tauri IPC / end-to-end test coverage

\- \[ ] Improved documentation and release automation

\- \[ ] Evaluate additional filesystem/platform backends if cross-platform support becomes a priority



The roadmap is intentionally conservative: performance changes should be justified by real benchmarks rather than theoretical optimization.



---



\## Contributing



Contributions and bug reports are welcome.



Before making larger changes:



1\. Open an issue describing the problem or proposed feature.

2\. Keep filesystem operations conservative and non-destructive.

3\. Preserve execution-time collision protection.

4\. Avoid introducing unnecessary dependencies or abstractions.

5\. Run the project's formatting, linting, build, and test checks before submitting a pull request.



---



\## License



See the \[`LICENSE`](LICENSE) file for the project's current license.



---



<div align="center">



\*\*Blaze File Organizer\*\*



Simple organization. Predictable file operations.



</div>



