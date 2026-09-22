import { invoke } from "@tauri-apps/api/core";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import confetti from "canvas-confetti";

// Interfaces
export interface CategoryRule {
  id: string;
  name: string;
  enabled: boolean;
  target_folder: string;
  extensions: string[];
  icon?: string;
}

export interface ScanRules {
  categories: CategoryRule[];
  custom_extensions: string[];
  include_subfolders: boolean;
  include_hidden: boolean;
  custom_target_dir: string | null;
}

export interface FilePreview {
  id: string;
  file_name: string;
  extension: string;
  category: string;
  source_path: string;
  destination_path: string;
  size_bytes: number;
  conflict_detected: boolean;
  relative_path?: string;
}

export interface FileAction {
  id: string;
  source_path: string;
  destination_path: string;
  category: string;
  file_name: string;
}

export interface ScanResult {
  previews: FilePreview[];
  skipped_paths: string[];
}

export interface CompletedFile {
  id: string;
  destination_path: string;
}

export interface ExecutionSummary {
  total_processed: number;
  successful: number;
  failed: number;
  time_taken_ms: number;
  mode: string;
  errors: string[];
  completed: CompletedFile[];
}

// Built-in Default Categories
const DEFAULT_CATEGORIES: CategoryRule[] = [
  {
    id: "images",
    name: "Images",
    icon: "🖼",
    enabled: true,
    target_folder: "Images",
    extensions: ["jpg", "jpeg", "png", "gif", "webp", "svg", "heic", "raw"],
  },
  {
    id: "documents",
    name: "Documents",
    icon: "📄",
    enabled: true,
    target_folder: "Documents",
    extensions: ["pdf", "docx", "xlsx", "txt", "csv", "pptx", "md", "epub"],
  },
  {
    id: "videos",
    name: "Videos",
    icon: "🎬",
    enabled: true,
    target_folder: "Videos",
    extensions: ["mp4", "mkv", "mov", "avi", "webm", "m4v"],
  },
  {
    id: "audio",
    name: "Audio",
    icon: "🎵",
    enabled: true,
    target_folder: "Audio",
    extensions: ["mp3", "wav", "flac", "aac", "ogg", "m4a"],
  },
  {
    id: "archives",
    name: "Archives",
    icon: "📦",
    enabled: true,
    target_folder: "Archives",
    extensions: ["zip", "rar", "7z", "tar", "gz", "bz2", "iso"],
  },
  {
    id: "code",
    name: "Code",
    icon: "💻",
    enabled: true,
    target_folder: "Code",
    extensions: ["rs", "ts", "tsx", "js", "jsx", "py", "html", "css", "json", "sql"],
  },
];

// Global State
let currentPath = "";
let isSubfoldersEnabled = false;
let ignoreHidden = true;
let operationMode: "MOVE" | "COPY" = "MOVE";
let customExtensions: string[] = ["blend", "iso", "psd"];
let categories: CategoryRule[] = JSON.parse(JSON.stringify(DEFAULT_CATEGORIES));

let allPreviews: FilePreview[] = [];
let selectedFileIds: Set<string> = new Set();
let activeCategoryFilter = "all";
let searchQuery = "";
let sortColumn: "name" | "category" | "size" = "name";
let sortDirection: "asc" | "desc" = "asc";
let isExecuting = false;

// Modal State for Editing Category
let editingCategoryId: string | null = null;

// Environment check
function isTauri(): boolean {
  return typeof window !== "undefined" && ("__TAURI_INTERNALS__" in window || "__TAURI__" in window);
}

// Format byte size cleanly
function formatBytes(bytes: number): string {
  if (bytes === 0) return "0 B";
  const k = 1024;
  const sizes = ["B", "KB", "MB", "GB", "TB"];
  const i = Math.floor(Math.log(bytes) / Math.log(k));
  return `${parseFloat((bytes / Math.pow(k, i)).toFixed(1))} ${sizes[i]}`;
}

// Helpers for soft muted category styling & compact path display
function getCategoryColorClass(name: string): string {
  const n = name.toLowerCase();
  if (n.includes("image") || n.includes("photo") || n.includes("picture")) return "cat-color-images";
  if (n.includes("doc") || n.includes("text") || n.includes("pdf")) return "cat-color-documents";
  if (n.includes("video") || n.includes("movie")) return "cat-color-videos";
  if (n.includes("audio") || n.includes("music") || n.includes("sound")) return "cat-color-audio";
  if (n.includes("archive") || n.includes("zip")) return "cat-color-archives";
  if (n.includes("code") || n.includes("dev") || n.includes("script")) return "cat-color-code";
  return "cat-color-custom";
}

function getCategoryBadgeClass(category: string): string {
  const n = category.toLowerCase();
  if (n.includes("image") || n.includes("photo")) return "cat-badge-images";
  if (n.includes("doc") || n.includes("text") || n.includes("pdf")) return "cat-badge-documents";
  if (n.includes("video") || n.includes("movie")) return "cat-badge-videos";
  if (n.includes("audio") || n.includes("music")) return "cat-badge-audio";
  if (n.includes("archive") || n.includes("zip")) return "cat-badge-archives";
  if (n.includes("code") || n.includes("dev")) return "cat-badge-code";
  return "cat-badge-custom";
}

function formatCompactPath(pathStr: string, maxLen = 30): string {
  if (!pathStr || pathStr.length <= maxLen) return pathStr;
  const sep = pathStr.includes("/") ? "/" : "\\";
  const parts = pathStr.split(sep);
  if (parts.length <= 2) return pathStr;
  const last = parts[parts.length - 1];
  const secondLast = parts[parts.length - 2];
  const compact = `...${sep}${secondLast}${sep}${last}`;
  return compact.length < pathStr.length ? compact : pathStr;
}

// Show clean toast
function showToast(message: string) {
  const container = document.getElementById("toast-container");
  if (!container) return;

  const toast = document.createElement("div");
  toast.className = "toast";
  toast.textContent = message;

  container.appendChild(toast);
  setTimeout(() => {
    toast.style.opacity = "0";
    setTimeout(() => toast.remove(), 200);
  }, 2800);
}

// DOM Elements
let sourcePathInput: HTMLInputElement;
let browseDirBtn: HTMLButtonElement;
let subfolderCheckbox: HTMLInputElement;
let ignoreHiddenCheckbox: HTMLInputElement;
let tableSearchInput: HTMLInputElement;
let selectAllRowsCheckbox: HTMLInputElement;
let previewBody: HTMLElement;
let tableEmptyState: HTMLElement;
let stageSummaryText: HTMLElement;
let dockStatusMessage: HTMLElement;
let executeBtn: HTMLButtonElement;
let executeBtnText: HTMLElement;
let rescanBtn: HTMLButtonElement;
let categoryCardsContainer: HTMLElement;
let customTagsContainer: HTMLElement;
let categoryFilterChips: HTMLElement;

// Resizer Elements
let panelResizer: HTMLElement;
let controlPanel: HTMLElement;

// Category Modal Elements
let categoryModal: HTMLElement;
let catModalTitle: HTMLElement;
let catModalClose: HTMLButtonElement;
let catNameInput: HTMLInputElement;
let catIconInput: HTMLInputElement;
let catFolderInput: HTMLInputElement;
let catExtsInput: HTMLInputElement;
let catEnabledInput: HTMLInputElement;
let catDeleteBtn: HTMLButtonElement;
let catCancelBtn: HTMLButtonElement;
let catSaveBtn: HTMLButtonElement;

// Settings Modal Elements
let settingsBtn: HTMLButtonElement;
let settingsModal: HTMLElement;
let settingsModalClose: HTMLButtonElement;
let settingsDoneBtn: HTMLButtonElement;

// Execution Modal Elements
let executionModal: HTMLElement;
let execModalClose: HTMLButtonElement;
let execDoneBtn: HTMLButtonElement;
let execPercentage: HTMLElement;
let execRatio: HTMLElement;
let execBarFill: HTMLElement;
let execTimeVal: HTMLElement;
let execSpeedVal: HTMLElement;
let execSuccessVal: HTMLElement;
let execFailedVal: HTMLElement;
let execLogConsole: HTMLElement;

// Initialize
window.addEventListener("DOMContentLoaded", () => {
  cacheDOM();
  setupDraggableResizer();
  setupEventListeners();
  renderCategoryCards();
  renderCustomTags();
  renderFilterChips();
  triggerScan();
});

function cacheDOM() {
  sourcePathInput = document.getElementById("source-path-input") as HTMLInputElement;
  browseDirBtn = document.getElementById("browse-dir-btn") as HTMLButtonElement;
  subfolderCheckbox = document.getElementById("subfolder-checkbox") as HTMLInputElement;
  ignoreHiddenCheckbox = document.getElementById("ignore-hidden-checkbox") as HTMLInputElement;
  tableSearchInput = document.getElementById("table-search-input") as HTMLInputElement;
  selectAllRowsCheckbox = document.getElementById("select-all-rows-checkbox") as HTMLInputElement;
  previewBody = document.getElementById("preview-body") as HTMLElement;
  tableEmptyState = document.getElementById("table-empty-state") as HTMLElement;
  stageSummaryText = document.getElementById("stage-summary-text") as HTMLElement;
  dockStatusMessage = document.getElementById("dock-status-message") as HTMLElement;
  executeBtn = document.getElementById("execute-btn") as HTMLButtonElement;
  executeBtnText = document.getElementById("execute-btn-text") as HTMLElement;
  rescanBtn = document.getElementById("rescan-btn") as HTMLButtonElement;
  categoryCardsContainer = document.getElementById("category-cards-container") as HTMLElement;
  customTagsContainer = document.getElementById("custom-tags-container") as HTMLElement;
  categoryFilterChips = document.getElementById("category-filter-chips") as HTMLElement;

  panelResizer = document.getElementById("panel-resizer") as HTMLElement;
  controlPanel = document.getElementById("control-panel") as HTMLElement;

  categoryModal = document.getElementById("category-modal") as HTMLElement;
  catModalTitle = document.getElementById("cat-modal-title") as HTMLElement;
  catModalClose = document.getElementById("cat-modal-close") as HTMLButtonElement;
  catNameInput = document.getElementById("cat-name-input") as HTMLInputElement;
  catIconInput = document.getElementById("cat-icon-input") as HTMLInputElement;
  catFolderInput = document.getElementById("cat-folder-input") as HTMLInputElement;
  catExtsInput = document.getElementById("cat-exts-input") as HTMLInputElement;
  catEnabledInput = document.getElementById("cat-enabled-input") as HTMLInputElement;
  catDeleteBtn = document.getElementById("cat-delete-btn") as HTMLButtonElement;
  catCancelBtn = document.getElementById("cat-cancel-btn") as HTMLButtonElement;
  catSaveBtn = document.getElementById("cat-save-btn") as HTMLButtonElement;

  settingsBtn = document.getElementById("settings-btn") as HTMLButtonElement;
  settingsModal = document.getElementById("settings-modal") as HTMLElement;
  settingsModalClose = document.getElementById("settings-modal-close") as HTMLButtonElement;
  settingsDoneBtn = document.getElementById("settings-done-btn") as HTMLButtonElement;

  executionModal = document.getElementById("execution-modal") as HTMLElement;
  execModalClose = document.getElementById("exec-modal-close") as HTMLButtonElement;
  execDoneBtn = document.getElementById("exec-done-btn") as HTMLButtonElement;
  execPercentage = document.getElementById("exec-percentage") as HTMLElement;
  execRatio = document.getElementById("exec-ratio") as HTMLElement;
  execBarFill = document.getElementById("exec-bar-fill") as HTMLElement;
  execTimeVal = document.getElementById("exec-time-val") as HTMLElement;
  execSpeedVal = document.getElementById("exec-speed-val") as HTMLElement;
  execSuccessVal = document.getElementById("exec-success-val") as HTMLElement;
  execFailedVal = document.getElementById("exec-failed-val") as HTMLElement;
  execLogConsole = document.getElementById("exec-log-console") as HTMLElement;
}

// SETUP DRAGGABLE RESIZER FOR DUAL-PANEL
function setupDraggableResizer() {
  let isDragging = false;

  panelResizer.addEventListener("mousedown", (e) => {
    isDragging = true;
    panelResizer.classList.add("is-dragging");
    document.body.style.cursor = "col-resize";
    document.body.style.userSelect = "none";
    e.preventDefault();
  });

  window.addEventListener("mousemove", (e) => {
    if (!isDragging) return;
    const newWidth = Math.max(300, Math.min(e.clientX, window.innerWidth - 400));
    controlPanel.style.width = `${newWidth}px`;
  });

  window.addEventListener("mouseup", () => {
    if (isDragging) {
      isDragging = false;
      panelResizer.classList.remove("is-dragging");
      document.body.style.cursor = "";
      document.body.style.userSelect = "";
    }
  });
}

function setupEventListeners() {
  // Folder browsing
  browseDirBtn.addEventListener("click", handleBrowseFolder);
  document.getElementById("empty-browse-btn")?.addEventListener("click", handleBrowseFolder);
  sourcePathInput.addEventListener("change", () => {
    currentPath = sourcePathInput.value.trim() || currentPath;
    triggerScan();
  });

  // Subfolder switch
  subfolderCheckbox.addEventListener("change", () => {
    isSubfoldersEnabled = subfolderCheckbox.checked;
    triggerScan();
    showToast(isSubfoldersEnabled ? "Including subfolders" : "Top-level files only");
  });

  // Hidden files
  ignoreHiddenCheckbox.addEventListener("change", () => {
    ignoreHidden = ignoreHiddenCheckbox.checked;
    triggerScan();
  });

  // Operation Mode
  document.querySelectorAll<HTMLInputElement>('input[name="operation-mode"]').forEach((radio) => {
    radio.addEventListener("change", () => {
      if (radio.checked) {
        operationMode = radio.value as "MOVE" | "COPY";
        document.querySelectorAll(".mode-pill-option").forEach((el) => el.classList.remove("active"));
        radio.closest(".mode-pill-option")?.classList.add("active");
        updateDockSummary();
        renderTable();
      }
    });
  });

  // Category All / None
  document.getElementById("select-all-rules-btn")?.addEventListener("click", () => {
    categories.forEach((c) => (c.enabled = true));
    renderCategoryCards();
    triggerScan();
  });

  document.getElementById("clear-all-rules-btn")?.addEventListener("click", () => {
    categories.forEach((c) => (c.enabled = false));
    renderCategoryCards();
    triggerScan();
  });

  // Add Category
  document.getElementById("add-category-btn")?.addEventListener("click", () => {
    openCategoryModal(null);
  });

  // Custom Extension Add
  const addExtBtn = document.getElementById("add-ext-btn");
  const customExtInput = document.getElementById("custom-ext-input") as HTMLInputElement;

  const handleAddCustomExt = () => {
    const val = customExtInput.value.trim().toLowerCase().replace(/^\./, "");
    if (val && !customExtensions.includes(val)) {
      customExtensions.push(val);
      renderCustomTags();
      customExtInput.value = "";
      triggerScan();
    }
  };

  addExtBtn?.addEventListener("click", handleAddCustomExt);
  customExtInput?.addEventListener("keydown", (e) => {
    if (e.key === "Enter" || e.key === ",") {
      e.preventDefault();
      handleAddCustomExt();
    }
  });

  // Search input
  tableSearchInput.addEventListener("input", () => {
    searchQuery = tableSearchInput.value.trim().toLowerCase();
    renderTable();
  });

  // Table sorting
  document.querySelectorAll<HTMLTableCellElement>("th.sortable").forEach((th) => {
    th.addEventListener("click", () => {
      const col = th.getAttribute("data-sort") as "name" | "category" | "size";
      if (sortColumn === col) {
        sortDirection = sortDirection === "asc" ? "desc" : "asc";
      } else {
        sortColumn = col;
        sortDirection = "asc";
      }
      renderTable();
    });
  });

  // Select all table rows
  selectAllRowsCheckbox.addEventListener("change", () => {
    const checked = selectAllRowsCheckbox.checked;
    const visible = getFilteredFiles();
    if (checked) {
      visible.forEach((f) => selectedFileIds.add(f.id));
    } else {
      visible.forEach((f) => selectedFileIds.delete(f.id));
    }
    renderTable();
    updateDockSummary();
  });

  // Re-scan button
  rescanBtn.addEventListener("click", () => {
    triggerScan();
    showToast("Re-scanning folder...");
  });

  // Execute button
  executeBtn.addEventListener("click", handleExecute);

  // Icon Preset Chips
  document.querySelectorAll<HTMLButtonElement>(".icon-chip-btn").forEach((chip) => {
    chip.addEventListener("click", () => {
      const icon = chip.getAttribute("data-icon");
      if (icon && catIconInput) {
        catIconInput.value = icon;
      }
    });
  });

  // Category Modal Handlers
  catModalClose.addEventListener("click", closeCategoryModal);
  catCancelBtn.addEventListener("click", closeCategoryModal);
  catSaveBtn.addEventListener("click", saveCategory);
  catDeleteBtn.addEventListener("click", deleteCategory);

  // Settings Modal Handlers
  settingsBtn.addEventListener("click", () => {
    settingsModal.classList.remove("hidden");
  });
  settingsModalClose.addEventListener("click", () => {
    settingsModal.classList.add("hidden");
  });
  settingsDoneBtn.addEventListener("click", () => {
    settingsModal.classList.add("hidden");
  });

  // Execution Modal Handlers
  execModalClose.addEventListener("click", () => {
    if (isExecuting) return;
    executionModal.classList.add("hidden");
    triggerScan();
  });
  execDoneBtn.addEventListener("click", () => {
    executionModal.classList.add("hidden");
    triggerScan();
  });
}

// Folder Browser
async function handleBrowseFolder() {
  if (isTauri()) {
    try {
      const selected = await openDialog({
        directory: true,
        multiple: false,
        title: "Select Folder to Organize",
      });

      if (selected && typeof selected === "string") {
        currentPath = selected;
        sourcePathInput.value = currentPath;
        triggerScan();
      }
    } catch (err) {
      console.error(err);
      showToast("Could not open folder picker");
    }
  } else {
    if ("showDirectoryPicker" in window) {
      try {
        // @ts-expect-error browser picker
        const dir = await window.showDirectoryPicker();
        currentPath = `/${dir.name}`;
        sourcePathInput.value = currentPath;
        triggerScan();
        showToast(`Selected "${dir.name}"`);
      } catch {
        // canceled
      }
    } else {
      showToast("Folder selection requires running as desktop app");
    }
  }
}

// Render Category Cards (Editable & Toggleable)
function renderCategoryCards() {
  categoryCardsContainer.innerHTML = "";

  const activeCount = categories.filter((c) => c.enabled).length;
  const summaryBadge = document.getElementById("categories-summary-badge");
  if (summaryBadge) {
    summaryBadge.textContent = `${activeCount} active`;
  }

  categories.forEach((cat) => {
    const card = document.createElement("div");
    card.className = `category-card ${cat.enabled ? "active" : "disabled"}`;
    card.setAttribute("data-id", cat.id);
    const count = allPreviews.filter((p) => p.category.toLowerCase() === cat.name.toLowerCase()).length;

    const left = document.createElement("div");
    left.className = "category-left";
    left.title = "Click to toggle inclusion";
    const label = document.createElement("label");
    label.className = "toggle-option";
    const checkbox = document.createElement("input");
    checkbox.type = "checkbox";
    checkbox.className = "cat-checkbox";
    checkbox.dataset.id = cat.id;
    checkbox.checked = cat.enabled;
    const toggleBox = document.createElement("span");
    toggleBox.className = "toggle-box";
    label.append(checkbox, toggleBox);
    const icon = document.createElement("div");
    icon.className = `category-icon-box ${getCategoryColorClass(cat.name)}`;
    icon.textContent = cat.icon || "📁";
    const info = document.createElement("div");
    info.className = "category-info";
    const titleRow = document.createElement("div");
    titleRow.className = "category-title-row";
    const name = document.createElement("span");
    name.className = "category-name-text";
    name.textContent = cat.name;
    const target = document.createElement("span");
    target.className = "category-target-path";
    target.textContent = `→ ${cat.target_folder}/`;
    titleRow.append(name, target);
    const extensions = document.createElement("span");
    extensions.className = "category-exts-preview";
    extensions.textContent = cat.extensions.join(" · ");
    info.append(titleRow, extensions);
    left.append(label, icon, info);

    const right = document.createElement("div");
    right.className = "category-right";
    const countLabel = document.createElement("span");
    countLabel.className = "category-count";
    countLabel.textContent = String(count);
    const editButton = document.createElement("button");
    editButton.className = "category-edit-btn";
    editButton.dataset.id = cat.id;
    editButton.title = "Edit category rules";
    const svg = document.createElementNS("http://www.w3.org/2000/svg", "svg");
    svg.setAttribute("width", "13"); svg.setAttribute("height", "13"); svg.setAttribute("viewBox", "0 0 24 24");
    svg.setAttribute("fill", "none"); svg.setAttribute("stroke", "currentColor"); svg.setAttribute("stroke-width", "2");
    for (const d of ["M12 20h9", "M16.5 3.5a2.121 2.121 0 0 1 3 3L7 19l-4 1 1-4L16.5 3.5z"]) {
      const path = document.createElementNS("http://www.w3.org/2000/svg", "path"); path.setAttribute("d", d); svg.appendChild(path);
    }
    editButton.appendChild(svg);
    right.append(countLabel, editButton);
    card.append(left, right);

    left.addEventListener("click", (event) => {
      if ((event.target as HTMLElement).closest(".toggle-option")) return;
      cat.enabled = !cat.enabled;
      renderCategoryCards();
      triggerScan();
    });
    checkbox.addEventListener("change", (e) => {
      cat.enabled = (e.target as HTMLInputElement).checked;
      renderCategoryCards();
      triggerScan();
    });
    editButton.addEventListener("click", (e) => {
      e.stopPropagation();
      openCategoryModal(cat.id);
    });

    categoryCardsContainer.appendChild(card);
  });

  renderFilterChips();
}

// Render Filter Chips on Stage
function renderFilterChips() {
  categoryFilterChips.innerHTML = "";

  const allChip = document.createElement("button");
  allChip.className = `filter-chip ${activeCategoryFilter === "all" ? "active" : ""}`;
  allChip.setAttribute("data-filter", "all");
  allChip.textContent = "All";
  allChip.addEventListener("click", () => {
    activeCategoryFilter = "all";
    renderFilterChips();
    renderTable();
  });
  categoryFilterChips.appendChild(allChip);

  categories.forEach((cat) => {
    const chip = document.createElement("button");
    chip.className = `filter-chip ${activeCategoryFilter === cat.name ? "active" : ""}`;
    chip.setAttribute("data-filter", cat.name);
    chip.textContent = cat.name;
    chip.addEventListener("click", () => {
      activeCategoryFilter = cat.name;
      renderFilterChips();
      renderTable();
    });
    categoryFilterChips.appendChild(chip);
  });

  if (customExtensions.length > 0) {
    const custChip = document.createElement("button");
    custChip.className = `filter-chip ${activeCategoryFilter === "Custom" ? "active" : ""}`;
    custChip.setAttribute("data-filter", "Custom");
    custChip.textContent = "Custom";
    custChip.addEventListener("click", () => {
      activeCategoryFilter = "Custom";
      renderFilterChips();
      renderTable();
    });
    categoryFilterChips.appendChild(custChip);
  }
}

// Category Modal Functions
function openCategoryModal(catId: string | null) {
  editingCategoryId = catId;

  if (catId) {
    const cat = categories.find((c) => c.id === catId);
    if (!cat) return;
    catModalTitle.textContent = `Edit "${cat.name}"`;
    catNameInput.value = cat.name;
    catIconInput.value = cat.icon || "📁";
    catFolderInput.value = cat.target_folder;
    catExtsInput.value = cat.extensions.join(", ");
    catEnabledInput.checked = cat.enabled;
    catDeleteBtn.style.display = "inline-flex";
  } else {
    catModalTitle.textContent = "New Category";
    catNameInput.value = "";
    catIconInput.value = "📁";
    catFolderInput.value = "";
    catExtsInput.value = "";
    catEnabledInput.checked = true;
    catDeleteBtn.style.display = "none";
  }

  categoryModal.classList.remove("hidden");
  catNameInput.focus();
}

function closeCategoryModal() {
  categoryModal.classList.add("hidden");
  editingCategoryId = null;
}

function saveCategory() {
  const name = catNameInput.value.trim();
  const icon = catIconInput.value.trim() || "📁";
  const folder = catFolderInput.value.trim() || name;
  const extsRaw = catExtsInput.value
    .split(/[,\s]+/)
    .map((e) => e.trim().toLowerCase().replace(/^\./, ""))
    .filter(Boolean);

  if (!name) {
    showToast("Please enter a category name");
    return;
  }

  if (editingCategoryId) {
    const cat = categories.find((c) => c.id === editingCategoryId);
    if (cat) {
      cat.name = name;
      cat.icon = icon;
      cat.target_folder = folder;
      cat.extensions = extsRaw;
      cat.enabled = catEnabledInput.checked;
    }
  } else {
    categories.push({
      id: `custom-cat-${Date.now()}`,
      name,
      icon,
      target_folder: folder,
      extensions: extsRaw,
      enabled: catEnabledInput.checked,
    });
  }

  closeCategoryModal();
  renderCategoryCards();
  triggerScan();
  showToast(`Saved category "${name}"`);
}

function deleteCategory() {
  if (!editingCategoryId) return;
  categories = categories.filter((c) => c.id !== editingCategoryId);
  closeCategoryModal();
  renderCategoryCards();
  triggerScan();
  showToast("Category removed");
}

// Custom Extension Tags
function renderCustomTags() {
  customTagsContainer.innerHTML = "";
  customExtensions.forEach((ext) => {
    const tag = document.createElement("span");
    tag.className = "custom-tag-chip";
    const label = document.createElement("span");
    label.textContent = `.${ext}`;
    const removeButton = document.createElement("button");
    removeButton.className = "custom-tag-remove";
    removeButton.dataset.ext = ext;
    removeButton.title = "Remove";
    removeButton.textContent = "×";
    removeButton.addEventListener("click", () => {
      customExtensions = customExtensions.filter((e) => e !== ext);
      renderCustomTags();
      triggerScan();
    });
    tag.append(label, removeButton);
    customTagsContainer.appendChild(tag);
  });
}

// Core Scanner
async function triggerScan() {
  if (!currentPath || !currentPath.trim()) {
    allPreviews = [];
    selectedFileIds = new Set();
    renderCategoryCards();
    renderTable();
    updateDockSummary();
    return;
  }

  try {
    if (isTauri()) {
      const scanRules: ScanRules = {
        categories,
        custom_extensions: customExtensions,
        include_subfolders: isSubfoldersEnabled,
        include_hidden: !ignoreHidden,
        custom_target_dir: null,
      };

      const result = await invoke<ScanResult>("scan_directory", {
        sourceDir: currentPath,
        rules: scanRules,
      });
      allPreviews = result.previews;
      if (result.skipped_paths.length > 0) {
        console.warn("Scan skipped paths:", result.skipped_paths);
        showToast(`Scan completed with ${result.skipped_paths.length} skipped path(s): ${result.skipped_paths[0]}`);
      }
    } else {
      allPreviews = [];
    }

    selectedFileIds = new Set(allPreviews.map((p) => p.id));
    renderCategoryCards();
    renderTable();
    updateDockSummary();
  } catch (err) {
    console.error(err);
    allPreviews = [];
    selectedFileIds = new Set();
    renderCategoryCards();
    renderTable();
    updateDockSummary();
    showToast(`Scan error: ${err}`);
  }
}

// Filter and sort items
function getFilteredFiles(): FilePreview[] {
  let list = allPreviews;

  if (activeCategoryFilter !== "all") {
    list = list.filter((p) => p.category.toLowerCase() === activeCategoryFilter.toLowerCase());
  }

  if (searchQuery) {
    list = list.filter(
      (p) =>
        p.file_name.toLowerCase().includes(searchQuery) ||
        p.extension.toLowerCase().includes(searchQuery) ||
        p.destination_path.toLowerCase().includes(searchQuery)
    );
  }

  list = [...list].sort((a, b) => {
    let comp = 0;
    if (sortColumn === "name") {
      comp = a.file_name.localeCompare(b.file_name);
    } else if (sortColumn === "category") {
      comp = a.category.localeCompare(b.category);
    } else if (sortColumn === "size") {
      comp = a.size_bytes - b.size_bytes;
    }
    return sortDirection === "asc" ? comp : -comp;
  });

  return list;
}

// Render Preview Table (Clean table with box around type, soft muted category badge, and compact paths)
function renderTable() {
  const files = getFilteredFiles();
  const totalVolume = allPreviews.reduce((acc, p) => acc + p.size_bytes, 0);

  stageSummaryText.textContent = `${allPreviews.length} files (${formatBytes(totalVolume)})`;

  if (files.length === 0) {
    previewBody.innerHTML = "";
    tableEmptyState.classList.add("active");
    const emptyTitle = document.getElementById("empty-state-title");
    const emptySubtitle = document.getElementById("empty-state-subtitle");
    if (emptyTitle && emptySubtitle) {
      if (!currentPath || !currentPath.trim()) {
        emptyTitle.textContent = "No folder selected";
        emptySubtitle.textContent = "Click Browse to choose a folder to organize.";
      } else {
        emptyTitle.textContent = "No files matching active rules";
        emptySubtitle.textContent = "Select another folder or adjust your categories on the left.";
      }
    }
  } else {
    tableEmptyState.classList.remove("active");
    const actionVerb = operationMode === "MOVE" ? "Move" : "Copy";
    const fragment = document.createDocumentFragment();
    files.forEach((file) => {
      const isSelected = selectedFileIds.has(file.id);
      const actionText = file.conflict_detected ? "Rename on conflict" : actionVerb;
      const actionClass = file.conflict_detected ? "action-status rename" : "action-status ready";
      const srcDisplay = formatCompactPath(file.relative_path || file.source_path);
      const destDisplay = formatCompactPath(file.destination_path);
      const row = document.createElement("tr");
      row.className = isSelected ? "selected" : "";
      row.dataset.id = file.id;
      const checkboxCell = document.createElement("td");
      checkboxCell.className = "col-checkbox";
      const checkbox = document.createElement("input");
      checkbox.type = "checkbox";
      checkbox.className = "row-checkbox";
      checkbox.dataset.id = file.id;
      checkbox.checked = isSelected;
      checkboxCell.appendChild(checkbox);
      const makeCell = (className: string, text: string, title?: string) => {
        const cell = document.createElement("td");
        cell.className = className;
        cell.textContent = text;
        if (title) cell.title = title;
        return cell;
      };
      const typeCell = document.createElement("td");
      typeCell.className = "col-type";
      const typeBadge = document.createElement("span");
      typeBadge.className = "type-badge";
      typeBadge.textContent = file.extension;
      typeCell.appendChild(typeBadge);
      const categoryCell = document.createElement("td");
      categoryCell.className = "col-category";
      const categoryBadge = document.createElement("span");
      categoryBadge.className = `cat-badge ${getCategoryBadgeClass(file.category)}`;
      categoryBadge.textContent = file.category;
      categoryCell.appendChild(categoryBadge);
      const actionCell = document.createElement("td");
      actionCell.className = "col-action";
      const actionStatus = document.createElement("span");
      actionStatus.className = actionClass;
      actionStatus.textContent = actionText;
      actionCell.appendChild(actionStatus);
      row.append(
        checkboxCell,
        makeCell("col-name", file.file_name, file.file_name),
        typeCell,
        categoryCell,
        makeCell("col-size", formatBytes(file.size_bytes)),
        makeCell("col-source", srcDisplay, file.source_path),
        makeCell("col-destination", destDisplay, file.destination_path),
        actionCell,
      );
      checkbox.addEventListener("change", (event) => {
        if (checkbox.checked) {
          selectedFileIds.add(file.id);
          row.classList.add("selected");
        } else {
          selectedFileIds.delete(file.id);
          row.classList.remove("selected");
        }
        updateDockSummary();
        event.stopPropagation();
      });
      row.addEventListener("click", (event) => {
        if ((event.target as HTMLElement).tagName === "INPUT") return;
        checkbox.checked = !checkbox.checked;
        checkbox.dispatchEvent(new Event("change"));
      });
      fragment.appendChild(row);
    });
    previewBody.replaceChildren(fragment);
  }
}

// Update Action Footer Dock
function updateDockSummary() {
  const count = selectedFileIds.size;
  const actionWord = operationMode === "MOVE" ? "moved" : "copied";

  if (count === 0) {
    executeBtn.disabled = true;
    if (executeBtnText) {
      executeBtnText.textContent = "Organize files";
    }
    dockStatusMessage.textContent = "No files selected";
  } else {
    executeBtn.disabled = false;
    if (executeBtnText) {
      executeBtnText.textContent = `Organize ${count} files`;
    }
    dockStatusMessage.textContent = `${count} files ready to be ${actionWord}. Existing files will not be overwritten.`;
  }
}

// FastCopy-style High-Speed Execution Handler
async function handleExecute() {
  if (isExecuting) return;
  const selectedItems = allPreviews.filter((p) => selectedFileIds.has(p.id));
  if (selectedItems.length === 0) return;
  isExecuting = true;

  const total = selectedItems.length;
  const actionWord = operationMode === "MOVE" ? "Moving" : "Copying";

  // Open Modal
  executionModal.classList.remove("hidden");
  document.getElementById("exec-modal-title")!.textContent = `${actionWord} ${total} Files`;
  document.getElementById("exec-modal-subtitle")!.textContent = "High-speed buffered I/O execution active...";
  execDoneBtn.style.display = "none";
  execLogConsole.innerHTML = "";

  execPercentage.textContent = "0%";
  execRatio.textContent = `0 / ${total}`;
  execBarFill.style.width = "0%";
  execTimeVal.textContent = "--";
  execSpeedVal.textContent = "--";
  execSuccessVal.textContent = "--";
  execFailedVal.textContent = "0";
  executeBtn.disabled = true;

  const actions: FileAction[] = selectedItems.map((p) => ({
    id: p.id,
    source_path: p.source_path,
    destination_path: p.destination_path,
    category: p.category,
    file_name: p.file_name,
  }));

  const startTime = performance.now();

  if (isTauri()) {
    try {
      const summary = await invoke<ExecutionSummary>("execute_organization", {
        items: actions,
        mode: operationMode,
      });

      const elapsed = summary.time_taken_ms || Math.round(performance.now() - startTime);
      const speed = Math.round(summary.successful / Math.max(elapsed / 1000, 0.001));
      const completedDestinations = new Map(summary.completed.map((item) => [item.id, item.destination_path]));
      allPreviews = allPreviews.map((preview) => ({
        ...preview,
        destination_path: completedDestinations.get(preview.id) || preview.destination_path,
      }));
      finishExecution(summary.successful, summary.failed, elapsed, speed, summary.errors);
    } catch (err) {
      console.error(err);
      finishExecution(0, total, Math.round(performance.now() - startTime), 0, [`Execution failed: ${formatError(err)}`]);
    } finally {
      isExecuting = false;
      updateDockSummary();
    }
  } else {
    // Ultra-fast simulated transfer with rapid progress chunks
    let processed = 0;
    const batchSize = Math.max(1, Math.floor(total / 8));

    const interval = setInterval(() => {
      processed += batchSize;
      if (processed >= total) {
        processed = total;
        clearInterval(interval);

        const elapsed = Math.max(12, Math.round(performance.now() - startTime));
        const speed = Math.round(total / Math.max(elapsed / 1000, 0.001));

        finishExecution(total, 0, elapsed, speed, []);
        isExecuting = false;
        updateDockSummary();
      } else {
        const pct = Math.round((processed / total) * 100);
        execPercentage.textContent = `${pct}%`;
        execRatio.textContent = `${processed} / ${total}`;
        execBarFill.style.width = `${pct}%`;

        const cur = selectedItems[processed - 1];
        if (cur) {
          appendLog(`[OK] ${cur.file_name} -> ${cur.destination_path}`);
        }
      }
    }, 25);
  }
}

function formatError(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}

function appendLog(msg: string) {
  const line = document.createElement("div");
  line.className = "log-line info";
  line.textContent = msg;
  execLogConsole.appendChild(line);
  execLogConsole.scrollTop = execLogConsole.scrollHeight;
}

function finishExecution(successful: number, failed: number, elapsedMs: number, speed: number, errors: string[]) {
  execPercentage.textContent = "100%";
  execRatio.textContent = `${successful} / ${successful + failed}`;
  execBarFill.style.width = "100%";

  const completedWithoutErrors = failed === 0 && errors.length === 0;
  document.getElementById("exec-modal-title")!.textContent = completedWithoutErrors
    ? "Organization Completed"
    : successful > 0
      ? "Organization Completed With Errors"
      : "Organization Failed";
  document.getElementById("exec-modal-subtitle")!.textContent = completedWithoutErrors
    ? `Organized ${successful} files in ${elapsedMs}ms (${speed} files/sec)`
    : `${successful} succeeded, ${failed} failed or were not confirmed. Review the log for details.`;

  execTimeVal.textContent = `${elapsedMs} ms`;
  execSpeedVal.textContent = `${speed} /s`;
  execSuccessVal.textContent = `${successful}`;
  execFailedVal.textContent = `${failed}`;

  appendLog(completedWithoutErrors ? `Completed: ${successful} files processed successfully.` : `Completed with errors: ${successful} succeeded, ${failed} failed.`);
  if (errors.length > 0) {
    errors.forEach((e) => appendLog(`Warning: ${e}`));
  }

  execDoneBtn.style.display = "inline-flex";

  try {
    if (completedWithoutErrors) {
      confetti({
        particleCount: 60,
        spread: 60,
        origin: { y: 0.6 },
        colors: ["#0F172A", "#2563EB", "#059669"],
      });
    }
  } catch {
    // ignore
  }

  showToast(completedWithoutErrors ? `Organized ${successful} files` : `Organization finished with ${failed} error(s)`);
}
