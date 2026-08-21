/// Sabitori File Manager — built entirely with the declarative Element API.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::process::Stdio;

use sabitori::*;
use sabitori::element::Px;
use sabitori::file_browser::{self, FileEntry, SortBy, SortOrder};

const ROW_H: f32 = 32.0;
/// ファイル一覧のスクロールコンテナ id。 `.scroll(FILE_SCROLL_ID)` を付けると、
/// ホイール・慣性・位置の保持をランタイムが持つ。 アプリ側は `ctx.scroll_info()`
/// で読むだけ (issue #14 の「所有者を 1 つに決める」)。
const FILE_SCROLL_ID: &str = "file-list";
const HEADER_H: f32 = 45.0;
const TOOLBAR_H: f32 = 35.0;
const TAB_BAR_H: f32 = 32.0;
const STATUS_H: f32 = 32.0;
const COL_HEADER_H: f32 = 32.0;
const SETTINGS_W: f32 = 320.0;
const MIN_SIDEBAR_W: f32 = 120.0;
const MAX_SIDEBAR_W: f32 = 400.0;

fn c(hex: &str) -> Color { Color::from_hex(hex) }

// ---------------------------------------------------------------------------
// Theme
// ---------------------------------------------------------------------------

#[derive(Clone)]
struct FilerTheme {
    name: &'static str,
    bg: &'static str,
    surface: &'static str,
    elevated: &'static str,
    border: &'static str,
    primary: &'static str,
    text_pri: &'static str,
    text_sec: &'static str,
    hover_bg: &'static str,
    select_bg: &'static str,
    bg_opacity: f32,
}

const THEMES: &[FilerTheme] = &[
    FilerTheme {
        name: "Midnight",
        bg: "#12121e", surface: "#1a1a2e", elevated: "#22223a", border: "#2a2a40",
        primary: "#6c63ff", text_pri: "#e8e8f0", text_sec: "#9090a8",
        hover_bg: "#ffffff08", select_bg: "#6c63ff30", bg_opacity: 1.0,
    },
    FilerTheme {
        name: "Tokyo Night",
        bg: "#1a1b26", surface: "#24283b", elevated: "#292e42", border: "#3b4261",
        primary: "#7aa2f7", text_pri: "#c0caf5", text_sec: "#565f89",
        hover_bg: "#ffffff08", select_bg: "#7aa2f730", bg_opacity: 1.0,
    },
    FilerTheme {
        name: "Catppuccin",
        bg: "#1e1e2e", surface: "#28283e", elevated: "#313244", border: "#45475a",
        primary: "#cba6f7", text_pri: "#cdd6f4", text_sec: "#6c7086",
        hover_bg: "#ffffff08", select_bg: "#cba6f730", bg_opacity: 1.0,
    },
    FilerTheme {
        name: "Rose Pine",
        bg: "#191724", surface: "#1f1d2e", elevated: "#26233a", border: "#403d52",
        primary: "#c4a7e7", text_pri: "#e0def4", text_sec: "#6e6a86",
        hover_bg: "#ffffff08", select_bg: "#c4a7e730", bg_opacity: 1.0,
    },
    FilerTheme {
        name: "Nord",
        bg: "#2e3440", surface: "#3b4252", elevated: "#434c5e", border: "#4c566a",
        primary: "#88c0d0", text_pri: "#eceff4", text_sec: "#7b88a1",
        hover_bg: "#ffffff08", select_bg: "#88c0d030", bg_opacity: 1.0,
    },
    FilerTheme {
        name: "Dracula",
        bg: "#282a36", surface: "#2d303e", elevated: "#343746", border: "#44475a",
        primary: "#bd93f9", text_pri: "#f8f8f2", text_sec: "#6272a4",
        hover_bg: "#ffffff08", select_bg: "#bd93f930", bg_opacity: 1.0,
    },
    FilerTheme {
        name: "Emerald",
        bg: "#0d1117", surface: "#161b22", elevated: "#1c2129", border: "#30363d",
        primary: "#3fb950", text_pri: "#e6edf3", text_sec: "#7d8590",
        hover_bg: "#ffffff08", select_bg: "#3fb95030", bg_opacity: 1.0,
    },
    FilerTheme {
        name: "Sunset",
        bg: "#1a1420", surface: "#221c2a", elevated: "#2a2234", border: "#3d3248",
        primary: "#ff7b72", text_pri: "#f0e6f6", text_sec: "#8b7f99",
        hover_bg: "#ffffff08", select_bg: "#ff7b7230", bg_opacity: 1.0,
    },
    FilerTheme {
        name: "Glass",
        bg: "#0a0a14", surface: "#12121e", elevated: "#1a1a2e", border: "#ffffff18",
        primary: "#7aa2f7", text_pri: "#e8e8f0", text_sec: "#9090a8",
        hover_bg: "#ffffff0c", select_bg: "#7aa2f730", bg_opacity: 0.75,
    },
    FilerTheme {
        name: "Frosted",
        bg: "#1a1a2e", surface: "#22223a", elevated: "#2a2a42", border: "#ffffff15",
        primary: "#c4a7e7", text_pri: "#e0def4", text_sec: "#8b7f99",
        hover_bg: "#ffffff0c", select_bg: "#c4a7e730", bg_opacity: 0.65,
    },
];

// ---------------------------------------------------------------------------
// File type icons (Nerd Font)
// ---------------------------------------------------------------------------

fn file_type_info(file: &FileEntry) -> (&'static str, &'static str) {
    if file.is_dir { return ("#5b7bd5", ""); }
    let ext = file.path.extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase();
    match ext.as_str() {
        "rs"                    => ("#ff6e40", "\u{e7a8}"),
        "ts" | "tsx"            => ("#3178c6", "\u{e628}"),
        "js" | "jsx"            => ("#f0db4f", "\u{e781}"),
        "py"                    => ("#3776ab", "\u{e73c}"),
        "go"                    => ("#00add8", "\u{e627}"),
        "rb"                    => ("#cc342d", "\u{e739}"),
        "swift"                 => ("#fa7343", "\u{e755}"),
        "kt" | "kts"            => ("#7f52ff", "\u{e634}"),
        "java"                  => ("#ed8b00", "\u{e738}"),
        "c"                     => ("#6e6e6e", "\u{e61e}"),
        "cpp" | "cc" | "cxx"    => ("#00599c", "\u{e61d}"),
        "h" | "hpp"             => ("#8a7090", "\u{e61e}"),
        "html" | "htm"          => ("#e44d26", "\u{e736}"),
        "css"                   => ("#264de4", "\u{e749}"),
        "scss" | "sass"         => ("#cf649a", "\u{e749}"),
        "vue"                   => ("#42b883", "\u{e6a0}"),
        "svelte"                => ("#ff3e00", "\u{e697}"),
        "json"                  => ("#5b9a4e", "\u{e60b}"),
        "yaml" | "yml"          => ("#cb171e", "\u{e6a8}"),
        "toml"                  => ("#9c4221", "\u{e6b2}"),
        "xml"                   => ("#e44d26", "\u{f05c0}"),
        "md" | "mdx"            => ("#519aba", "\u{e73e}"),
        "txt" | "text"          => ("#8a8a8a", "\u{f0219}"),
        "sh" | "bash" | "zsh" | "fish" => ("#4eaa25", "\u{e795}"),
        "png" | "jpg" | "jpeg" | "gif" | "webp" | "bmp" | "ico"
                                => ("#4caf50", "\u{f1c5}"),
        "svg"                   => ("#ffb300", "\u{f1c5}"),
        "mp4" | "mov" | "avi" | "mkv" | "webm"
                                => ("#9c27b0", "\u{f1c8}"),
        "mp3" | "wav" | "flac" | "ogg" | "aac"
                                => ("#e91e63", "\u{f1c7}"),
        "pdf"                   => ("#f44336", "\u{f1c1}"),
        "doc" | "docx"          => ("#2b579a", "\u{f1c2}"),
        "xls" | "xlsx"          => ("#217346", "\u{f1c3}"),
        "ppt" | "pptx"          => ("#d24726", "\u{f1c4}"),
        "csv"                   => ("#217346", "\u{f1c3}"),
        "zip" | "tar" | "gz" | "bz2" | "xz" | "7z" | "rar"
                                => ("#ff9800", "\u{f1c6}"),
        "lock"                  => ("#666666", "\u{f023}"),
        "wgsl" | "glsl" | "hlsl" => ("#8bc34a", "\u{f1b2}"),
        "wasm"                  => ("#654ff0", "\u{e6a1}"),
        "app"                   => ("#888888", "\u{f108}"),
        "dmg" | "pkg"           => ("#888888", "\u{f0a0}"),
        "log"                   => ("#777777", "\u{f15c}"),
        "env"                   => ("#ecd53f", "\u{f013}"),
        "sql"                   => ("#e38c00", "\u{f1c0}"),
        _                       => ("#888888", "\u{f15b}"),
    }
}

fn file_icon(file: &FileEntry, primary: Color) -> Element {
    if file.is_dir {
        return div().w(Px(22.0)).h(Px(18.0)).shrink(0.0)
            .flex_col()
            .children([
                div().w(Px(10.0)).h(Px(4.0)).shrink(0.0)
                    .bg(primary.with_alpha(0.6))
                    .corner_radius(Corners { top_left: 2.0, top_right: 2.0, bottom_left: 0.0, bottom_right: 0.0 }),
                div().w(Px(22.0)).h(Px(14.0)).shrink(0.0)
                    .bg(primary.with_alpha(0.3))
                    .corner_radius(Corners { top_left: 0.0, top_right: 2.0, bottom_left: 2.0, bottom_right: 2.0 }),
            ]);
    }
    let (color_hex, icon) = file_type_info(file);
    div().w(Px(22.0)).h(Px(18.0)).shrink(0.0)
        .flex_row().items_center().justify_center()
        .children([text(icon).font_size(16.0).color(c(color_hex))])
}

// ---------------------------------------------------------------------------
// Tab
// ---------------------------------------------------------------------------

struct Tab {
    path: PathBuf,
    files: Vec<FileEntry>,
    filtered: Vec<usize>,
    selected: BTreeSet<usize>,
    last_selected: Option<usize>,
    /// 次のフレームでスクロールしたい位置。 ランタイムへ `scroll_intents()` で
    /// 渡して消す。 **スクロール位置そのものは持たない** — それは `.scroll(id)`
    /// を付けた時点でランタイムの持ち物になる (issue #14)。
    pending_scroll: Option<f32>,
    history: Vec<PathBuf>,
    history_pos: usize, // points to current entry in history
}

impl Tab {
    fn new(path: PathBuf, show_hidden: bool, sort_by: SortBy, sort_order: SortOrder) -> Self {
        let files = file_browser::read_directory(&path, show_hidden, sort_by, sort_order);
        let filtered: Vec<usize> = (0..files.len()).collect();
        let history = vec![path.clone()];
        Self {
            path,
            files,
            filtered,
            selected: BTreeSet::new(),
            last_selected: None,
            pending_scroll: None,
            history,
            history_pos: 0,
        }
    }

    fn refresh(&mut self, show_hidden: bool, sort_by: SortBy, sort_order: SortOrder) {
        self.files = file_browser::read_directory(&self.path, show_hidden, sort_by, sort_order);
        self.filtered = (0..self.files.len()).collect();
    }

    fn apply_filter(&mut self, query: &str) {
        if query.is_empty() {
            self.filtered = (0..self.files.len()).collect();
        } else {
            let q = query.to_lowercase();
            self.filtered = self.files.iter().enumerate()
                .filter(|(_, f)| f.name.to_lowercase().contains(&q))
                .map(|(i, _)| i)
                .collect();
        }
    }

    fn display_name(&self) -> &str {
        self.path.file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("/")
    }
}

// ---------------------------------------------------------------------------
// App
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, PartialEq)]
enum ViewMode { List, Grid }

struct CtxMenu {
    x: f32,
    y: f32,
    on_file: bool,
}

#[derive(Clone)]
struct FileConflict {
    source: PathBuf,
    dest_dir: PathBuf,
    name: String,
    is_cut: bool, // move vs copy
}

struct ConflictModal {
    conflicts: Vec<FileConflict>,
    current: usize,
    apply_all: Option<ConflictChoice>,
}

#[derive(Clone, Copy, PartialEq)]
enum ConflictChoice { Replace, KeepBoth, Skip }

struct FilerApp {
    tabs: Vec<Tab>,
    active_tab: usize,
    bookmarks: Vec<(String, PathBuf)>,
    show_hidden: bool,
    sort_by: SortBy,
    sort_order: SortOrder,
    view_mode: ViewMode,
    sidebar_width: f32,
    sidebar_dragging: bool,
    search_active: bool,
    search_query: String,
    theme_idx: usize,
    show_settings: bool,
    bg_opacity: f32,
    opacity_dragging: bool,
    last_width: std::cell::Cell<f32>,
    /// 前フレームの `(scroll_y, viewport_height)`。 キーボード選択の auto-scroll は
    /// `&mut self` から呼ばれて `ctx` を持たないので、 `view()` で控えておく。
    last_scroll: std::cell::Cell<(f32, f32)>,
    last_shift: std::cell::Cell<bool>,
    last_cmd: std::cell::Cell<bool>,
    ql_open: bool,
    // Context menu
    ctx_menu: Option<CtxMenu>,
    // File clipboard
    clipboard: Vec<PathBuf>,
    clipboard_cut: bool,
    // Rename
    renaming: Option<(usize, String)>, // (real file index, current text)
    // Double-click tracking
    last_click: Option<(usize, std::time::Instant)>, // (filtered_idx, time)
    // Drag & drop
    drag: Option<DragState>,
    last_hovered: std::cell::RefCell<Option<String>>,
    last_mouse: std::cell::Cell<(f32, f32)>,
    // Toast notification
    toast: Option<(String, std::time::Instant)>,
    // Window handle for OS drag
    window: Option<std::sync::Arc<winit::window::Window>>,
    // External file hover (drag from another window/app)
    hover_files: Vec<PathBuf>,
    /// `tick` が絵を変えたか。 `poll_dirty` で汲んで下ろす。
    tick_changed: bool,
    // Conflict resolution modal
    conflict_modal: Option<ConflictModal>,
    hover_mouse: (f32, f32),
}

struct DragState {
    file_indices: Vec<usize>,
    start_x: f32,
    start_y: f32,
    active: bool,
    created: std::time::Instant,
}

impl FilerApp {
    fn new() -> Self {
        let home = std::env::var("SABITORI_START_DIR").ok()
            .map(PathBuf::from)
            .filter(|p| p.is_dir())
            .unwrap_or_else(|| file_browser::home_dir());
        let tab = Tab::new(home, false, SortBy::Name, SortOrder::Ascending);
        Self {
            tabs: vec![tab],
            active_tab: 0,
            bookmarks: file_browser::default_bookmarks(),
            show_hidden: false,
            sort_by: SortBy::Name,
            sort_order: SortOrder::Ascending,
            view_mode: ViewMode::List,
            sidebar_width: 200.0,
            sidebar_dragging: false,
            search_active: false,
            search_query: String::new(),
            theme_idx: 0,
            show_settings: false,
            bg_opacity: 1.0,
            opacity_dragging: false,
            last_width: std::cell::Cell::new(1100.0),
            last_scroll: std::cell::Cell::new((0.0, 0.0)),
            last_shift: std::cell::Cell::new(false),
            last_cmd: std::cell::Cell::new(false),
            ql_open: false,
            ctx_menu: None,
            clipboard: Vec::new(),
            clipboard_cut: false,
            renaming: None,
            last_click: None,
            drag: None,
            last_hovered: std::cell::RefCell::new(None),
            last_mouse: std::cell::Cell::new((0.0, 0.0)),
            toast: None,
            window: None,
            hover_files: Vec::new(),
            tick_changed: false,
            conflict_modal: None,
            hover_mouse: (0.0, 0.0),
        }
    }

    fn theme(&self) -> &FilerTheme { &THEMES[self.theme_idx] }
    fn tab(&self) -> &Tab { &self.tabs[self.active_tab] }
    fn tab_mut(&mut self) -> &mut Tab { &mut self.tabs[self.active_tab] }

    fn navigate_to(&mut self, path: PathBuf) {
        if path.is_dir() {
            let (sh, sb, so) = (self.show_hidden, self.sort_by, self.sort_order);
            let tab = self.tab_mut();
            // Push to history (truncate forward history)
            tab.history.truncate(tab.history_pos + 1);
            tab.history.push(path.clone());
            tab.history_pos = tab.history.len() - 1;
            tab.path = path;
            tab.refresh(sh, sb, so);
            tab.selected.clear();
            tab.last_selected = None;
            tab.pending_scroll = Some(0.0);
            self.search_query.clear();
            self.search_active = false;
            self.ql_close();
        }
    }

    fn go_back(&mut self) {
        let tab = self.tab();
        if tab.history_pos == 0 { return; }
        let new_pos = tab.history_pos - 1;
        let path = tab.history[new_pos].clone();
        let (sh, sb, so) = (self.show_hidden, self.sort_by, self.sort_order);
        let tab = self.tab_mut();
        tab.history_pos = new_pos;
        tab.path = path;
        tab.refresh(sh, sb, so);
        tab.selected.clear();
        tab.last_selected = None;
        tab.pending_scroll = Some(0.0);
    }

    fn go_forward(&mut self) {
        let tab = self.tab();
        if tab.history_pos + 1 >= tab.history.len() { return; }
        let new_pos = tab.history_pos + 1;
        let path = tab.history[new_pos].clone();
        let (sh, sb, so) = (self.show_hidden, self.sort_by, self.sort_order);
        let tab = self.tab_mut();
        tab.history_pos = new_pos;
        tab.path = path;
        tab.refresh(sh, sb, so);
        tab.selected.clear();
        tab.last_selected = None;
        tab.pending_scroll = Some(0.0);
    }

    fn refresh_all_tabs(&mut self) {
        let (sh, sb, so) = (self.show_hidden, self.sort_by, self.sort_order);
        for tab in &mut self.tabs {
            tab.refresh(sh, sb, so);
            tab.apply_filter("");
        }
    }

    fn new_tab(&mut self) {
        let path = self.tab().path.clone();
        let tab = Tab::new(path, self.show_hidden, self.sort_by, self.sort_order);
        self.tabs.push(tab);
        self.active_tab = self.tabs.len() - 1;
    }

    fn close_tab(&mut self, idx: usize) {
        if self.tabs.len() <= 1 { return; }
        self.tabs.remove(idx);
        if self.active_tab >= self.tabs.len() {
            self.active_tab = self.tabs.len() - 1;
        }
    }

    fn select_file(&mut self, filtered_idx: usize, shift: bool, cmd: bool) {
        let tab = self.tab_mut();
        let real_idx = tab.filtered[filtered_idx];

        if shift {
            // Range select
            if let Some(anchor) = tab.last_selected {
                let anchor_filtered = tab.filtered.iter().position(|&i| i == anchor).unwrap_or(0);
                let (lo, hi) = if filtered_idx < anchor_filtered {
                    (filtered_idx, anchor_filtered)
                } else {
                    (anchor_filtered, filtered_idx)
                };
                if !cmd { tab.selected.clear(); }
                for fi in lo..=hi {
                    tab.selected.insert(tab.filtered[fi]);
                }
            }
        } else if cmd {
            // Toggle
            if tab.selected.contains(&real_idx) {
                tab.selected.remove(&real_idx);
            } else {
                tab.selected.insert(real_idx);
            }
            tab.last_selected = Some(real_idx);
        } else {
            // Single
            tab.selected.clear();
            tab.selected.insert(real_idx);
            tab.last_selected = Some(real_idx);
        }
    }

    fn selected_file(&self) -> Option<&FileEntry> {
        let tab = self.tab();
        tab.last_selected.and_then(|i| tab.files.get(i))
    }

    // Quick Look
    fn ql_kill(&self) {
        let _ = std::process::Command::new("killall").arg("qlmanage")
            .stdout(Stdio::null()).stderr(Stdio::null()).status();
    }
    fn ql_show(&mut self, path: &Path) {
        self.ql_kill();
        let _ = std::process::Command::new("qlmanage")
            .arg("-p").arg(path)
            .stdout(Stdio::null()).stderr(Stdio::null())
            .spawn();
        self.ql_open = true;
    }
    fn ql_close(&mut self) {
        self.ql_kill();
        self.ql_open = false;
    }
    fn ql_toggle(&mut self) {
        if self.ql_open {
            self.ql_close();
        } else if let Some(f) = self.selected_file() {
            let p = f.path.clone(); self.ql_show(&p);
        }
    }
    fn ql_update_if_open(&mut self) {
        if self.ql_open {
            if let Some(f) = self.selected_file() {
                let p = f.path.clone(); self.ql_show(&p);
            }
        }
    }

    // ── File operations ──
    fn copy_selected(&mut self) {
        let tab = self.tab();
        self.clipboard = tab.selected.iter()
            .filter_map(|&i| tab.files.get(i).map(|f| f.path.clone()))
            .collect();
        self.clipboard_cut = false;
    }

    fn cut_selected(&mut self) {
        self.copy_selected();
        self.clipboard_cut = true;
    }

    fn paste_files(&mut self) {
        if self.clipboard.is_empty() { return; }
        let dest = self.tab().path.clone();
        let sources = self.clipboard.clone();
        let is_cut = self.clipboard_cut;
        if is_cut { self.clipboard.clear(); }
        self.transfer_files(sources, dest, is_cut);
    }

    fn trash_selected(&mut self) {
        let tab = self.tab();
        let paths: Vec<PathBuf> = tab.selected.iter()
            .filter_map(|&i| tab.files.get(i).map(|f| f.path.clone()))
            .collect();
        for path in &paths {
            // macOS: use osascript to move to Trash (safer than rm)
            let _ = std::process::Command::new("osascript")
                .arg("-e")
                .arg(&format!(
                    "tell application \"Finder\" to delete POSIX file \"{}\"",
                    path.display()
                ))
                .stdout(Stdio::null()).stderr(Stdio::null())
                .status();
        }
        let (sh, sb, so) = (self.show_hidden, self.sort_by, self.sort_order);
        let tab = self.tab_mut();
        tab.selected.clear();
        tab.last_selected = None;
        tab.refresh(sh, sb, so);
    }

    fn new_folder(&mut self) {
        let base = self.tab().path.join("untitled folder");
        let mut target = base.clone();
        let mut n = 1;
        while target.exists() {
            target = self.tab().path.join(format!("untitled folder {n}"));
            n += 1;
        }
        let _ = std::fs::create_dir(&target);
        let (sh, sb, so) = (self.show_hidden, self.sort_by, self.sort_order);
        self.tab_mut().refresh(sh, sb, so);
        // Find and select the new folder, start rename
        let idx = self.tab().files.iter().position(|f| f.path == target);
        if let Some(i) = idx {
            let tab = self.tab_mut();
            tab.selected.clear();
            tab.selected.insert(i);
            tab.last_selected = Some(i);
            let name = target.file_name().unwrap_or_default().to_string_lossy().to_string();
            self.renaming = Some((i, name));
        }
    }

    fn start_rename(&mut self) {
        if let Some(&real_idx) = self.tab().selected.iter().next() {
            if self.tab().selected.len() == 1 {
                let name = self.tab().files[real_idx].name.clone();
                self.renaming = Some((real_idx, name));
            }
        }
    }

    fn confirm_rename(&mut self) {
        if let Some((real_idx, ref new_name)) = self.renaming {
            if let Some(file) = self.tab().files.get(real_idx) {
                if !new_name.is_empty() && new_name != &file.name {
                    let new_path = file.path.parent().unwrap().join(new_name);
                    let _ = std::fs::rename(&file.path, &new_path);
                    let (sh, sb, so) = (self.show_hidden, self.sort_by, self.sort_order);
                    self.tab_mut().refresh(sh, sb, so);
                }
            }
        }
        self.renaming = None;
    }

    fn cancel_rename(&mut self) {
        self.renaming = None;
    }

    // ── Context menu rendering ──
    fn context_menu(&self, ctx: &ViewContext) -> Element {
        let Some(ref menu) = self.ctx_menu else { return div(); };
        let t = self.theme();
        let text_pri = c(t.text_pri);
        let text_sec = c(t.text_sec);
        let primary = c(t.primary);
        let menu_bg = c(t.elevated);
        let has_sel = !self.tab().selected.is_empty();
        let has_clip = !self.clipboard.is_empty();

        let item = |id: &str, label: &str, shortcut: &str, enabled: bool| -> Element {
            let hovered = ctx.hovered.as_deref() == Some(id);
            let fg = if !enabled { text_sec.with_alpha(0.3) }
                else if hovered { text_pri }
                else { text_sec };
            let bg = if hovered && enabled { c(t.hover_bg) } else { Color::TRANSPARENT };
            div().id(if enabled { id } else { "" })
                .h(Px(28.0)).shrink(0.0).bg(bg).rounded_px(4.0)
                .flex_row().items_center().justify_between().px_pad(Px(12.0))
                .children([
                    text(label).font_size(12.0).color(fg),
                    text(shortcut).font_size(10.0).color(text_sec.with_alpha(0.4)),
                ])
        };
        let sep = || -> Element {
            div().h(Px(1.0)).shrink(0.0).bg(c(t.border)).mx(Px(8.0)).my(Px(2.0))
        };

        let mut items: Vec<Element> = Vec::new();

        if menu.on_file && has_sel {
            items.push(item("ctx-open", "Open", "\u{21a9}", true));
            items.push(item("ctx-open-new", "Open in New Window", "\u{2318}N", true));
            items.push(item("ctx-rename", "Rename", "Enter", true));
            items.push(sep());
            items.push(item("ctx-copy", "Copy", "\u{2318}C", true));
            items.push(item("ctx-cut", "Cut", "\u{2318}X", true));
            if has_clip {
                items.push(item("ctx-paste", "Paste", "\u{2318}V", true));
            }
            items.push(sep());
            items.push(item("ctx-trash", "Move to Trash", "\u{2318}\u{232b}", true));
        } else {
            // Empty area
            items.push(item("ctx-newfolder", "New Folder", "\u{2318}\u{21e7}N", true));
            if has_clip {
                items.push(item("ctx-paste", "Paste", "\u{2318}V", true));
            }
            items.push(sep());
            items.push(item("ctx-selectall", "Select All", "\u{2318}A", true));
        }

        let menu_h = items.len() as f32 * 28.0 + 16.0;
        let menu_w = 220.0;
        // Clamp position to stay in viewport
        let mx = menu.x.min(ctx.width - menu_w - 8.0);
        let my = menu.y.min(ctx.height - menu_h - 8.0);

        // Backdrop: full viewport, transparent, captures clicks
        // Menu: positioned at click coordinates via padding
        div()
            .id("ctx-backdrop")
            .w(Px(ctx.width)).h(Px(ctx.height))
            .pt(Px(my)).pl(Px(mx))
            .children([
                div()
                    .w(Px(menu_w)).h(Px(menu_h))
                    .bg(menu_bg)
                    .border(1.0, c(t.border))
                    .rounded_px(8.0)
                    .shadow_md(Color::new(0.0, 0.0, 0.0, 0.5))
                    .p_px(4.0)
                    .flex_col()
                    .children(items),
            ])
    }

    fn safe_rename(dest: &Path, name: &str) -> PathBuf {
        let (stem, ext) = if let Some(dot) = name.rfind('.') {
            (&name[..dot], Some(&name[dot..]))
        } else {
            (name, None)
        };
        for n in 2..100 {
            let new_name = match ext {
                Some(ext) => format!("{stem} {n}{ext}"),
                None => format!("{stem} {n}"),
            };
            let candidate = dest.join(&new_name);
            if !candidate.exists() { return candidate; }
        }
        dest.join(format!("{name} copy"))
    }

    /// Execute file transfer: checks for conflicts, shows modal if needed.
    fn transfer_files(&mut self, sources: Vec<PathBuf>, dest: PathBuf, is_cut: bool) {
        let mut conflicts = Vec::new();
        let mut no_conflict = Vec::new();

        for src in sources {
            let name = src.file_name().unwrap_or_default().to_string_lossy().to_string();
            let target = dest.join(&name);
            if target.exists() && target != src {
                conflicts.push(FileConflict {
                    source: src, dest_dir: dest.clone(), name, is_cut,
                });
            } else if target != src {
                no_conflict.push((src, target, is_cut));
            }
        }

        // Execute non-conflicting immediately
        for (src, target, cut) in no_conflict {
            Self::do_transfer(&src, &target, cut);
        }

        // Show modal for conflicts
        if !conflicts.is_empty() {
            self.conflict_modal = Some(ConflictModal {
                conflicts,
                current: 0,
                apply_all: None,
            });
        }

        let (sh, sb, so) = (self.show_hidden, self.sort_by, self.sort_order);
        self.tab_mut().refresh(sh, sb, so);
    }

    fn do_transfer(src: &Path, target: &Path, is_cut: bool) {
        if is_cut {
            let _ = std::fs::rename(src, target);
        } else if src.is_dir() {
            let _ = std::process::Command::new("cp").arg("-r").arg(src).arg(target).status();
        } else {
            let _ = std::fs::copy(src, target);
        }
    }

    fn resolve_conflict(&mut self, choice: ConflictChoice) {
        let Some(ref mut modal) = self.conflict_modal else { return; };
        let conflict = modal.conflicts[modal.current].clone();

        match choice {
            ConflictChoice::Replace => {
                let target = conflict.dest_dir.join(&conflict.name);
                if target.is_dir() {
                    let _ = std::fs::remove_dir_all(&target);
                } else {
                    let _ = std::fs::remove_file(&target);
                }
                Self::do_transfer(&conflict.source, &target, conflict.is_cut);
            }
            ConflictChoice::KeepBoth => {
                let target = Self::safe_rename(&conflict.dest_dir, &conflict.name);
                Self::do_transfer(&conflict.source, &target, conflict.is_cut);
            }
            ConflictChoice::Skip => {}
        }

        modal.current += 1;

        // Check if there are more conflicts with apply_all
        if let Some(apply_choice) = modal.apply_all {
            while modal.current < modal.conflicts.len() {
                let c = modal.conflicts[modal.current].clone();
                match apply_choice {
                    ConflictChoice::Replace => {
                        let t = c.dest_dir.join(&c.name);
                        if t.is_dir() { let _ = std::fs::remove_dir_all(&t); }
                        else { let _ = std::fs::remove_file(&t); }
                        Self::do_transfer(&c.source, &t, c.is_cut);
                    }
                    ConflictChoice::KeepBoth => {
                        let t = Self::safe_rename(&c.dest_dir, &c.name);
                        Self::do_transfer(&c.source, &t, c.is_cut);
                    }
                    ConflictChoice::Skip => {}
                }
                modal.current += 1;
            }
        }

        if modal.current >= modal.conflicts.len() {
            self.conflict_modal = None;
            let (sh, sb, so) = (self.show_hidden, self.sort_by, self.sort_order);
            self.tab_mut().refresh(sh, sb, so);
        }
    }

    fn handle_drop(&mut self, source_indices: &[usize]) -> bool {
        // Determine drop target from last hovered element
        let hovered = self.last_hovered.borrow().clone();
        let target_dir = if let Some(ref id) = hovered {
            if let Some(idx_str) = id.strip_prefix("f-") {
                // Dropping on a file/folder in the list
                if let Ok(fi) = idx_str.parse::<usize>() {
                    let tab = self.tab();
                    if fi < tab.filtered.len() {
                        let real_idx = tab.filtered[fi];
                        if let Some(file) = tab.files.get(real_idx) {
                            if file.is_dir { Some(file.path.clone()) } else { None }
                        } else { None }
                    } else { None }
                } else { None }
            } else if let Some(idx_str) = id.strip_prefix("sb-") {
                // Dropping on a sidebar bookmark
                if let Ok(idx) = idx_str.parse::<usize>() {
                    self.bookmarks.get(idx).map(|(_, p)| p.clone())
                } else { None }
            } else { None }
        } else { None };

        if let Some(dest) = target_dir {
            let tab = self.tab();
            let paths: Vec<PathBuf> = source_indices.iter()
                .filter_map(|&i| tab.files.get(i).map(|f| f.path.clone()))
                .filter(|p| p.parent() != Some(dest.as_path()))
                .collect();
            if paths.is_empty() { return false; }
            self.transfer_files(paths, dest, true); // D&D = move
            true
        } else {
            false
        }
    }

    fn opacity_from_mouse(&self, mouse_x: f32, window_w: f32) -> f32 {
        let panel_x = window_w - SETTINGS_W;
        let track_left = panel_x + 12.0;
        let track_w = SETTINGS_W - 36.0;
        ((mouse_x - track_left - 6.0) / track_w).clamp(0.3, 1.0)
    }

    fn list_height(&self, ctx: &ViewContext) -> f32 {
        let extra = if self.tabs.len() > 1 { TAB_BAR_H } else { 0.0 };
        (ctx.height - HEADER_H - TOOLBAR_H - extra - STATUS_H - COL_HEADER_H).max(0.0)
    }

    fn sort_indicator(&self, col: SortBy) -> &'static str {
        if self.sort_by == col {
            if self.sort_order == SortOrder::Ascending { " \u{25b4}" } else { " \u{25be}" }
        } else { "" }
    }

    // ── Settings panel (unchanged logic, just references self.bg_opacity) ──
    fn settings_panel(&self, ctx: &ViewContext) -> Element {
        let t = self.theme();
        let op = self.bg_opacity;
        let primary = c(t.primary);
        let text_pri = c(t.text_pri);
        let text_sec = c(t.text_sec);
        let surface = c(t.surface).with_alpha(op);

        let mut theme_cards: Vec<Element> = Vec::new();
        for (i, theme) in THEMES.iter().enumerate() {
            let id = format!("theme-{i}");
            let is_active = i == self.theme_idx;
            let is_hovered = ctx.hovered.as_deref() == Some(id.as_str());
            let card_bg = if is_active { c(theme.primary).with_alpha(0.12) }
                else if is_hovered { c(t.hover_bg) }
                else { Color::TRANSPARENT };
            let card_border = if is_active { c(theme.primary).with_alpha(0.5) } else { c(t.border) };

            let swatches = div().flex_row().gap(4.0).children([
                div().w(Px(16.0)).h(Px(16.0)).shrink(0.0).rounded_px(4.0).bg(c(theme.bg)),
                div().w(Px(16.0)).h(Px(16.0)).shrink(0.0).rounded_px(4.0).bg(c(theme.surface)),
                div().w(Px(16.0)).h(Px(16.0)).shrink(0.0).rounded_px(4.0).bg(c(theme.primary)),
                div().w(Px(16.0)).h(Px(16.0)).shrink(0.0).rounded_px(4.0).bg(c(theme.text_pri)),
            ]);
            let preview = div().flex_1().flex_col().gap(2.0)
                .rounded_px(4.0).bg(c(theme.bg)).p_px(6.0)
                .children([
                    div().h(Px(4.0)).shrink(0.0).bg(c(theme.primary).with_alpha(0.6)).rounded_px(2.0),
                    div().h(Px(4.0)).shrink(0.0).bg(c(theme.text_sec).with_alpha(0.3)).rounded_px(2.0),
                ]);

            theme_cards.push(
                div().id(id).h(Px(64.0)).shrink(0.0)
                    .bg(card_bg).rounded_px(8.0).border(1.0, card_border)
                    .flex_row().items_center().p_px(12.0).gap(12.0)
                    .children([
                        div().flex_col().gap(6.0).children([
                            div().flex_row().items_center().gap(8.0).children([
                                text(theme.name).font_size(13.0)
                                    .color(if is_active { c(theme.primary) } else { text_pri }).bold(),
                                if is_active { text("Active").font_size(10.0).color(c(theme.primary).with_alpha(0.7)) }
                                else { div() },
                            ]),
                            swatches,
                        ]),
                        preview,
                    ]),
            );
        }

        // Opacity slider
        let track_w = SETTINGS_W - 24.0;
        let thumb_x = self.bg_opacity * (track_w - 12.0);
        let is_track_hovered = ctx.hovered.as_deref() == Some("opacity-track");
        let opacity_slider = div().flex_col().gap(6.0).shrink(0.0).children([
            div().flex_row().justify_between().children([
                text("Opacity").font_size(12.0).color(text_sec),
                text(&format!("{:.0}%", self.bg_opacity * 100.0)).font_size(12.0).color(primary),
            ]),
            div().id("opacity-track").h(Px(24.0)).shrink(0.0)
                .flex_row().items_center()
                .children([
                    div().h(Px(4.0)).w(Px(thumb_x + 6.0)).shrink(0.0).bg(primary)
                        .corner_radius(Corners { top_left: 2.0, bottom_left: 2.0, top_right: 0.0, bottom_right: 0.0 }),
                    div().w(Px(12.0)).h(Px(12.0)).shrink(0.0)
                        .bg(if is_track_hovered || self.opacity_dragging { primary } else { text_pri })
                        .rounded_px(6.0),
                    div().h(Px(4.0)).flex_1().bg(c(t.border))
                        .corner_radius(Corners { top_left: 0.0, bottom_left: 0.0, top_right: 2.0, bottom_right: 2.0 }),
                ]),
        ]);

        div().w(Px(SETTINGS_W)).shrink(0.0).bg(surface).flex_col().children([
            div().h(Px(44.0)).shrink(0.0).flex_row().items_center().justify_between().px_pad(Px(16.0)).children([
                text("Settings").font_size(14.0).color(text_pri).bold(),
                div().id("settings-close").w(Px(28.0)).h(Px(28.0)).shrink(0.0)
                    .bg(if ctx.hovered.as_deref() == Some("settings-close") { c(t.hover_bg) } else { Color::TRANSPARENT })
                    .rounded_px(6.0).flex_row().items_center().justify_center()
                    .children([text("\u{2715}").font_size(14.0).color(text_sec)]),
            ]),
            div().h(Px(1.0)).shrink(0.0).bg(c(t.border)),
            div().flex_1().flex_col().p_px(12.0).gap(16.0).children([
                opacity_slider,
                div().flex_col().gap(6.0).children([
                    text("Theme").font_size(12.0).color(text_sec),
                    div().flex_col().gap(6.0).children(theme_cards),
                ]),
            ]),
        ])
    }
}

// ---------------------------------------------------------------------------
// DeclarativeApp
// ---------------------------------------------------------------------------

impl DeclarativeApp for FilerApp {
    fn title(&self) -> &str { "Sabitori File Manager" }
    fn size(&self) -> (f32, f32) { (1100.0, 700.0) }
    fn transparent(&self) -> bool { true }
    fn decorations(&self) -> bool { false }
    fn fonts(&self) -> Vec<Vec<u8>> {
        vec![include_bytes!("../assets/fonts/SymbolsNerdFontMono-Regular.ttf").to_vec()]
    }

    fn overlay_view(&self, ctx: &ViewContext) -> Option<Element> {
        // Conflict resolution modal
        if let Some(ref modal) = self.conflict_modal {
            if modal.current < modal.conflicts.len() {
                let t = self.theme();
                let primary = c(t.primary);
                let text_pri = c(t.text_pri);
                let text_sec = c(t.text_sec);
                let conflict = &modal.conflicts[modal.current];
                let remaining = modal.conflicts.len() - modal.current;

                let btn = |id: &str, label: &str, bg_color: Color| -> Element {
                    let hovered = ctx.hovered.as_deref() == Some(id);
                    div().id(id).h(Px(32.0)).shrink(0.0)
                        .bg(if hovered { bg_color.lighten(0.15) } else { bg_color })
                        .rounded_px(6.0)
                        .flex_row().items_center().justify_center()
                        .px_pad(Px(16.0))
                        .children([text(label).font_size(12.0).color(Color::WHITE).bold()])
                };

                let panel = div()
                    .w(Px(420.0))
                    .bg(c(t.elevated))
                    .border(1.0, c(t.border))
                    .rounded_px(12.0)
                    .shadow_md(Color::new(0.0, 0.0, 0.0, 0.5))
                    .p_px(20.0)
                    .flex_col().gap(16.0)
                    .children([
                        // Title
                        text(&format!("\"{} \" already exists", conflict.name))
                            .font_size(14.0).color(text_pri).bold(),
                        // Description
                        text(&format!(
                            "A file with the same name exists in \"{}\".\nWhat would you like to do?",
                            conflict.dest_dir.file_name().unwrap_or_default().to_string_lossy()
                        )).font_size(12.0).color(text_sec),
                        // Remaining count
                        if remaining > 1 {
                            text(&format!("{remaining} conflicts remaining"))
                                .font_size(11.0).color(text_sec.with_alpha(0.6))
                        } else { div() },
                        // Buttons
                        div().flex_row().gap(8.0).justify_end().children([
                            btn("conflict-skip", "Skip", c("#555555")),
                            btn("conflict-keep", "Keep Both", c("#3b7dd8")),
                            btn("conflict-replace", "Replace", c("#d94040")),
                        ]),
                        // Apply to all checkbox
                        if remaining > 1 {
                            div().id("conflict-apply-all")
                                .flex_row().items_center().gap(6.0)
                                .children([
                                    div().w(Px(14.0)).h(Px(14.0)).shrink(0.0)
                                        .bg(if modal.apply_all.is_some() { primary } else { Color::TRANSPARENT })
                                        .border(1.0, if modal.apply_all.is_some() { primary } else { text_sec.with_alpha(0.4) })
                                        .rounded_px(3.0)
                                        .flex_row().items_center().justify_center()
                                        .children([
                                            if modal.apply_all.is_some() {
                                                text("\u{2713}").font_size(10.0).color(Color::WHITE)
                                            } else { div() },
                                        ]),
                                    text("Apply to all").font_size(11.0).color(text_sec),
                                ])
                        } else { div() },
                    ]);

                return Some(
                    div().id("conflict-backdrop")
                        .w(Px(ctx.width)).h(Px(ctx.height))
                        .bg(Color::new(0.0, 0.0, 0.0, 0.5))
                        .flex_col().items_center().justify_center()
                        .children([panel])
                );
            }
        }

        // Context menu
        if self.ctx_menu.is_some() {
            return Some(self.context_menu(ctx));
        }
        // Drag ghost
        if let Some(ref drag) = self.drag {
            if drag.active {
                let t = self.theme();
                let primary = c(t.primary);
                let text_pri = c(t.text_pri);
                let tab = self.tab();
                let count = drag.file_indices.len();

                // Build ghost rows (max 4 visible + count badge)
                let mut ghost_rows: Vec<Element> = Vec::new();
                let show_max = count.min(4);
                for (i, &real_idx) in drag.file_indices.iter().take(show_max).enumerate() {
                    if let Some(file) = tab.files.get(real_idx) {
                        let offset = i as f32 * 2.0; // slight stacking offset
                        ghost_rows.push(
                            div()
                                .h(Px(ROW_H)).shrink(0.0)
                                .bg(c(t.elevated).with_alpha(0.9))
                                .rounded_px(6.0)
                                .border(1.0, c(t.border))
                                .shadow_sm(Color::new(0.0, 0.0, 0.0, 0.3))
                                .flex_row().items_center().px_pad(Px(8.0)).gap(6.0)
                                .ml(Px(offset))
                                .children([
                                    file_icon(file, primary),
                                    text(&file.name).font_size(12.0).color(text_pri),
                                ]),
                        );
                    }
                }

                // Count badge if more than shown
                if count > show_max {
                    ghost_rows.push(
                        div()
                            .h(Px(20.0)).w(Px(20.0)).shrink(0.0)
                            .bg(primary)
                            .rounded_px(10.0)
                            .flex_row().items_center().justify_center()
                            .ml(Px(show_max as f32 * 2.0))
                            .children([
                                text(&format!("+{}", count - show_max))
                                    .font_size(10.0).color(Color::WHITE).bold(),
                            ]),
                    );
                }

                let ghost_w = 260.0;
                let ghost_h = show_max as f32 * (ROW_H + 2.0) + if count > show_max { 24.0 } else { 0.0 };
                let gx = (ctx.mouse_x - ghost_w * 0.5).clamp(0.0, ctx.width - ghost_w);
                let gy = (ctx.mouse_y - ROW_H * 0.5).clamp(0.0, ctx.height - ghost_h);

                return Some(
                    div()
                        .w(Px(ctx.width)).h(Px(ctx.height))
                        .pt(Px(gy)).pl(Px(gx))
                        .children([
                            div()
                                .w(Px(ghost_w))
                                .opacity(0.85)
                                .flex_col().gap(2.0)
                                .children(ghost_rows),
                        ]),
                );
            }
        }
        // External file hover: show ghost for incoming drag
        if !self.hover_files.is_empty() {
            let t = self.theme();
            let primary = c(t.primary);
            let text_pri = c(t.text_pri);

            let count = self.hover_files.len();
            let mut ghost_rows: Vec<Element> = Vec::new();
            for (i, path) in self.hover_files.iter().take(4).enumerate() {
                let name = path.file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_else(|| path.to_string_lossy().to_string());
                // Create a temp FileEntry so we get the same icon as internal drag
                let entry = FileEntry {
                    name: name.clone(),
                    path: path.clone(),
                    is_dir: path.is_dir(),
                    is_hidden: name.starts_with('.'),
                    is_symlink: path.symlink_metadata().map_or(false, |m| m.is_symlink()),
                    size: path.metadata().map_or(0, |m| m.len()),
                    modified: path.metadata().ok().and_then(|m| m.modified().ok()),
                };
                let offset = i as f32 * 2.0;
                ghost_rows.push(
                    div().h(Px(ROW_H)).shrink(0.0)
                        .bg(c(t.elevated).with_alpha(0.9))
                        .rounded_px(6.0)
                        .border(1.0, primary.with_alpha(0.4))
                        .shadow_sm(Color::new(0.0, 0.0, 0.0, 0.3))
                        .flex_row().items_center().px_pad(Px(8.0)).gap(6.0)
                        .ml(Px(offset))
                        .children([
                            file_icon(&entry, primary),
                            text(&name).font_size(12.0).color(text_pri),
                        ]),
                );
            }
            if count > 4 {
                ghost_rows.push(
                    div().h(Px(20.0)).w(Px(20.0)).shrink(0.0)
                        .bg(primary).rounded_px(10.0)
                        .flex_row().items_center().justify_center()
                        .ml(Px(8.0))
                        .children([text(&format!("+{}", count - 4)).font_size(10.0).color(Color::WHITE).bold()]),
                );
            }

            let (mx, my) = self.hover_mouse;
            let gx = (mx - 130.0).clamp(0.0, ctx.width - 260.0);
            let gy = (my - ROW_H * 0.5).clamp(0.0, ctx.height - 100.0);

            return Some(
                div().w(Px(ctx.width)).h(Px(ctx.height))
                    .pt(Px(gy)).pl(Px(gx))
                    .children([
                        div().w(Px(260.0)).opacity(0.85).flex_col().gap(2.0)
                            .children(ghost_rows),
                    ]),
            );
        }

        // Toast notification
        if let Some((ref msg, _)) = self.toast {
            let t = self.theme();
            return Some(
                div()
                    .w(Px(ctx.width)).h(Px(ctx.height))
                    .flex_col().items_center()
                    .pt(Px(ctx.height - 80.0))
                    .children([
                        div()
                            .h(Px(36.0))
                            .bg(c(t.elevated))
                            .border(1.0, c(t.border))
                            .rounded_px(8.0)
                            .shadow_md(Color::new(0.0, 0.0, 0.0, 0.4))
                            .flex_row().items_center().px_pad(Px(16.0))
                            .children([
                                text(msg).font_size(12.0).color(c(t.text_pri)),
                            ]),
                    ]),
            );
        }
        None
    }

    /// 外部ファイルをドラッグで持ってきている間だけ、 毎フレーム描き直す。
    /// OS のドラッグ中は `CursorMoved` が来ないので `tick` でマウスを追って
    /// おり、 入力イベントが 1 つも無いまま絵が動く — 既定の `lazy_render`
    /// はそれを知る手立てが無いので、 ここで名乗る。
    fn is_animating(&self) -> bool {
        !self.hover_files.is_empty()
    }

    /// toast と古いドラッグの掃除は「時計が来たら 1 回」。 連続した動きでは
    /// ないので `is_animating` ではなくこちら。
    fn poll_dirty(&mut self) -> bool {
        std::mem::take(&mut self.tick_changed)
    }

    fn tick(&mut self, _dt: f32) {
        // スクロールのばねはランタイムが回す (`.scroll(id)` に付けた時点で
        // 位置も速度もランタイムの持ち物)。 ここで tick する必要は無い。
        // Clear toast after 2 seconds
        if let Some((_, time)) = &self.toast {
            if time.elapsed().as_secs_f32() > 2.0 {
                self.toast = None;
                self.tick_changed = true;
            }
        }
        // Clear stale drag after 5 seconds
        if let Some(ref drag) = self.drag {
            if drag.created.elapsed().as_secs() > 5 {
                self.drag = None;
                self.tick_changed = true;
            }
        }
        // Poll mouse position during external file hover (OS drag doesn't send CursorMoved)
        #[cfg(target_os = "macos")]
        if !self.hover_files.is_empty() {
            if let Some(ref window) = self.window {
                if let Some(pos) = sabitori::macos_drag::get_mouse_position(window) {
                    self.hover_mouse = pos;
                }
            }
        }
    }

    fn view(&self, ctx: &ViewContext) -> Element {
        self.last_width.set(ctx.width);
        *self.last_hovered.borrow_mut() = ctx.hovered.clone();
        self.last_mouse.set((ctx.mouse_x, ctx.mouse_y));
        self.last_shift.set(ctx.shift_held);
        self.last_cmd.set(ctx.cmd_held);

        let t = self.theme();
        let op = self.bg_opacity;
        let primary = c(t.primary);
        let text_pri = c(t.text_pri);
        let text_sec = c(t.text_sec);
        let bg = c(t.bg).with_alpha(op);
        let surface = c(t.surface).with_alpha(op);
        let elevated = c(t.elevated).with_alpha(op);
        let tab = self.tab();

        // ── Title bar ──
        let close_hovered = ctx.hovered.as_deref() == Some("win-close");
        let mini_hovered = ctx.hovered.as_deref() == Some("win-mini");
        let zoom_hovered = ctx.hovered.as_deref() == Some("win-zoom");

        let header = div()
            .id("title-bar")
            .h(Px(38.0)).shrink(0.0).bg(elevated)
            .flex_col().children([
                div().flex_1().flex_row().items_center().px_pad(Px(14.0)).children([
                    // Traffic lights
                    div().flex_row().items_center().gap(8.0).shrink(0.0).children([
                        div().id("win-close").w(Px(12.0)).h(Px(12.0)).shrink(0.0)
                            .bg(if close_hovered { Color::from_hex("#ff5f57") } else { Color::from_hex("#ff5f5760") })
                            .rounded_px(6.0),
                        div().id("win-mini").w(Px(12.0)).h(Px(12.0)).shrink(0.0)
                            .bg(if mini_hovered { Color::from_hex("#febc2e") } else { Color::from_hex("#febc2e60") })
                            .rounded_px(6.0),
                        div().id("win-zoom").w(Px(12.0)).h(Px(12.0)).shrink(0.0)
                            .bg(if zoom_hovered { Color::from_hex("#28c840") } else { Color::from_hex("#28c84060") })
                            .rounded_px(6.0),
                    ]),
                    // Center: app name
                    div().flex_1().flex_row().justify_center().children([
                        text("Sabitori").font_size(12.0).color(text_sec.with_alpha(0.5)),
                    ]),
                    div().w(Px(56.0)).shrink(0.0),
                ]),
            ]);

        // ── Toolbar: nav buttons + breadcrumb + controls ──
        let file_count = tab.filtered.len();
        let sel_count = tab.selected.len();
        let can_back = tab.history_pos > 0;
        let can_fwd = tab.history_pos + 1 < tab.history.len();

        let nav_btn = |id: &str, label: &str, enabled: bool| -> Element {
            let hovered = ctx.hovered.as_deref() == Some(id);
            let fg = if !enabled { text_sec.with_alpha(0.2) }
                else if hovered { text_pri }
                else { text_sec };
            div().id(if enabled { id } else { "" })
                .w(Px(24.0)).h(Px(22.0)).shrink(0.0)
                .bg(if hovered && enabled { c(t.hover_bg) } else { Color::TRANSPARENT })
                .rounded_px(5.0)
                .flex_row().items_center().justify_center()
                .children([text(label).font_size(14.0).color(fg)])
        };

        // Breadcrumb path (each component clickable)
        let breadcrumb = if self.search_active {
            div().flex_1().h(Px(26.0))
                .bg(c(t.border).with_alpha(0.3)).rounded_px(6.0).border(1.0, primary.with_alpha(0.4))
                .flex_row().items_center().px_pad(Px(10.0)).gap(4.0)
                .children([
                    text("Find:").font_size(11.0).color(primary),
                    text(if self.search_query.is_empty() { "type to filter..." } else { &self.search_query })
                        .font_size(12.0).color(if self.search_query.is_empty() { text_sec.with_alpha(0.4) } else { text_pri }),
                ])
        } else {
            // Build clickable breadcrumb segments
            let path = &tab.path;
            let home = file_browser::home_dir();
            let mut crumbs: Vec<Element> = Vec::new();
            let components: Vec<_> = if let Ok(rel) = path.strip_prefix(&home) {
                let mut v = vec![("~", home.clone())];
                let mut acc = home.clone();
                for comp in rel.components() {
                    acc = acc.join(comp);
                    v.push((comp.as_os_str().to_str().unwrap_or("?"), acc.clone()));
                }
                v
            } else {
                let mut v = vec![("/", PathBuf::from("/"))];
                let mut acc = PathBuf::from("/");
                for comp in path.components().skip(1) {
                    acc = acc.join(comp);
                    v.push((comp.as_os_str().to_str().unwrap_or("?"), acc.clone()));
                }
                v
            };
            for (i, (name, _)) in components.iter().enumerate() {
                if i > 0 {
                    crumbs.push(text("/").font_size(11.0).color(text_sec.with_alpha(0.3)));
                }
                let crumb_id = format!("crumb-{i}");
                let is_last = i == components.len() - 1;
                let is_hovered = ctx.hovered.as_deref() == Some(crumb_id.as_str());
                crumbs.push(
                    div().id(crumb_id).shrink(0.0).px_pad(Px(3.0)).h(Px(20.0))
                        .bg(if is_hovered { c(t.hover_bg) } else { Color::TRANSPARENT })
                        .rounded_px(4.0)
                        .flex_row().items_center()
                        .children([
                            text(*name).font_size(12.0).color(if is_last { text_pri } else if is_hovered { text_pri } else { text_sec }),
                        ]),
                );
            }
            crumbs.push(text(&format!("  {file_count}")).font_size(10.0).color(text_sec.with_alpha(0.3)));

            div().flex_1().flex_row().items_center().gap(1.0).children(crumbs)
        };

        // Segmented view mode
        let seg = |id: &str, label: &str, active: bool| -> Element {
            let hovered = ctx.hovered.as_deref() == Some(id);
            div().id(id).h(Px(22.0)).shrink(0.0).px_pad(Px(8.0))
                .bg(if active { primary } else if hovered { c(t.hover_bg) } else { Color::TRANSPARENT })
                .rounded_px(5.0)
                .flex_row().items_center().justify_center()
                .children([text(label).font_size(10.0).color(if active { Color::WHITE } else { text_sec })])
        };
        // Text toggles
        let txt_btn = |id: &str, label: &str, active: bool| -> Element {
            let hovered = ctx.hovered.as_deref() == Some(id);
            div().id(id).h(Px(22.0)).shrink(0.0).px_pad(Px(4.0))
                .flex_row().items_center()
                .children([text(label).font_size(10.0)
                    .color(if active { primary } else if hovered { text_pri } else { text_sec.with_alpha(0.5) })])
        };

        let toolbar = div().h(Px(TOOLBAR_H)).shrink(0.0).bg(elevated).flex_col().children([
            div().flex_1().flex_row().items_center().px_pad(Px(12.0)).gap(6.0).children([
                nav_btn("nav-back", "<", can_back),
                nav_btn("nav-fwd", ">", can_fwd),
                breadcrumb,
                div().flex_row().h(Px(22.0)).shrink(0.0).bg(c(t.border).with_alpha(0.3)).rounded_px(5.0).children([
                    seg("view-list", "List", self.view_mode == ViewMode::List),
                    seg("view-grid", "Grid", self.view_mode == ViewMode::Grid),
                ]),
                txt_btn("toggle-hidden", ".*", self.show_hidden),
                txt_btn("search-btn", "Find", self.search_active),
                txt_btn("settings-btn", "Set", self.show_settings),
            ]),
            div().h(Px(1.0)).shrink(0.0).bg(c(t.border)),
        ]);

        // ── Tab bar (only if >1 tab) ──
        let tab_bar = if self.tabs.len() > 1 {
            let mut tab_items: Vec<Element> = Vec::new();
            for (i, t_tab) in self.tabs.iter().enumerate() {
                let id = format!("tab-{i}");
                let close_id = format!("tab-close-{i}");
                let is_active = i == self.active_tab;
                let is_hovered = ctx.hovered.as_deref() == Some(id.as_str());
                let close_hovered = ctx.hovered.as_deref() == Some(close_id.as_str());
                let tab_bg = if is_active { elevated } else if is_hovered { c(t.hover_bg) } else { Color::TRANSPARENT };
                tab_items.push(
                    div().id(id).h(Px(28.0)).shrink(0.0)
                        .bg(tab_bg).rounded_px(6.0)
                        .flex_row().items_center().px_pad(Px(10.0)).gap(6.0)
                        .children([
                            text(t_tab.display_name()).font_size(11.0)
                                .color(if is_active { text_pri } else { text_sec }),
                            div().id(close_id).w(Px(16.0)).h(Px(16.0)).shrink(0.0)
                                .bg(if close_hovered { c(t.hover_bg) } else { Color::TRANSPARENT })
                                .rounded_px(4.0).flex_row().items_center().justify_center()
                                .children([text("\u{2715}").font_size(9.0).color(text_sec)]),
                        ]),
                );
            }
            // "+" button
            tab_items.push(
                div().id("tab-new").w(Px(24.0)).h(Px(24.0)).shrink(0.0)
                    .bg(if ctx.hovered.as_deref() == Some("tab-new") { c(t.hover_bg) } else { Color::TRANSPARENT })
                    .rounded_px(6.0).flex_row().items_center().justify_center()
                    .children([text("+").font_size(14.0).color(text_sec)]),
            );

            div().h(Px(TAB_BAR_H)).shrink(0.0).bg(surface).flex_col().children([
                div().flex_1().flex_row().items_center().px_pad(Px(8.0)).gap(2.0).children(tab_items),
                div().h(Px(1.0)).shrink(0.0).bg(c(t.border)),
            ])
        } else { div() };

        // ── Sidebar with sections ──
        let mut sidebar_items: Vec<Element> = Vec::new();

        // Section label
        sidebar_items.push(
            text("Favorites").font_size(10.0).color(text_sec.with_alpha(0.4)).bold()
        );

        for (i, (label, path)) in self.bookmarks.iter().enumerate() {
            let id = format!("sb-{i}");
            let is_active = *path == tab.path;
            let is_hovered = ctx.hovered.as_deref() == Some(id.as_str());
            let row_bg = if is_active { primary.with_alpha(0.12) } else if is_hovered { c(t.hover_bg) } else { Color::TRANSPARENT };
            let label_color = if is_active { text_pri } else if is_hovered { text_pri.with_alpha(0.9) } else { text_sec };

            sidebar_items.push(
                div().id(id).h(Px(26.0)).shrink(0.0)
                    .bg(row_bg).rounded_px(6.0)
                    .flex_row().items_center().px_pad(Px(8.0)).gap(6.0)
                    .children([
                        // Folder icon for sidebar
                        div().w(Px(16.0)).h(Px(12.0)).shrink(0.0).flex_col().children([
                            div().w(Px(7.0)).h(Px(3.0)).shrink(0.0)
                                .bg(if is_active { primary.with_alpha(0.7) } else { text_sec.with_alpha(0.3) })
                                .corner_radius(Corners { top_left: 1.5, top_right: 1.5, bottom_left: 0.0, bottom_right: 0.0 }),
                            div().w(Px(16.0)).h(Px(9.0)).shrink(0.0)
                                .bg(if is_active { primary.with_alpha(0.4) } else { text_sec.with_alpha(0.15) })
                                .corner_radius(Corners { top_left: 0.0, top_right: 1.5, bottom_left: 1.5, bottom_right: 1.5 }),
                        ]),
                        text(label).font_size(11.0).color(label_color),
                    ]),
            );
        }

        let sidebar = div().w(Px(self.sidebar_width)).shrink(0.0).bg(surface).flex_col()
            .pt(Px(8.0)).px_pad(Px(6.0)).gap(2.0)
            .children(sidebar_items);

        // Sidebar drag handle
        let divider_hovered = ctx.hovered.as_deref() == Some("sidebar-drag");
        let divider = div().id("sidebar-drag").w(Px(5.0)).shrink(0.0)
            .bg(if divider_hovered || self.sidebar_dragging { primary.with_alpha(0.4) } else { c(t.border) })
            .flex_row().items_center().justify_center();

        // ── File area ──
        let list_h = self.list_height(ctx);
        // スクロール位置と viewport はランタイムの計測値から読む。 初回フレームは
        // まだ測れていないので、 chrome 高さから引いた概算 (`list_h`) で代用する。
        let sinfo = ctx.scroll_info(FILE_SCROLL_ID).unwrap_or_default();
        let scroll_y = sinfo.scroll_y;
        let viewport_h = if sinfo.viewport_height > 0.0 { sinfo.viewport_height } else { list_h };
        self.last_scroll.set((scroll_y, viewport_h));
        // スクロールバーの位置と長さ。 中身が収まっていれば出さない。
        let scrollbar_for = |content_h: f32| -> Element {
            if content_h <= viewport_h || content_h <= 0.0 {
                return div();
            }
            let ratio = viewport_h / content_h;
            let max_scroll = (content_h - viewport_h).max(1.0);
            let pos = (scroll_y / max_scroll).clamp(0.0, 1.0) * (1.0 - ratio);
            div().w(Px(6.0)).shrink(0.0).flex_col().children([
                div().h(Px(pos * viewport_h)).shrink(0.0),
                div().w(Px(4.0)).h(Px((ratio * viewport_h).max(20.0))).shrink(0.0)
                    .bg(Color::from_hex("#ffffff20")).rounded_px(2.0),
            ])
        };
        let file_area = if self.view_mode == ViewMode::List {
            // Sort indicator
            let sn = self.sort_indicator(SortBy::Name);
            let ss = self.sort_indicator(SortBy::Size);
            let sm = self.sort_indicator(SortBy::Modified);
            let col_header = div().h(Px(COL_HEADER_H)).shrink(0.0).bg(elevated).flex_col().children([
                div().flex_1().flex_row().items_center().px_pad(Px(12.0)).children([
                    div().id("sort-name").flex_1().flex_row().items_center().children([
                        text(&format!("Name{sn}")).font_size(12.0)
                            .color(if self.sort_by == SortBy::Name { primary } else { text_sec }),
                    ]),
                    div().id("sort-size").w(Px(80.0)).shrink(0.0).flex_row().items_center().children([
                        text(&format!("Size{ss}")).font_size(12.0)
                            .color(if self.sort_by == SortBy::Size { primary } else { text_sec }),
                    ]),
                    div().id("sort-mod").w(Px(130.0)).shrink(0.0).flex_row().items_center().children([
                        text(&format!("Modified{sm}")).font_size(12.0)
                            .color(if self.sort_by == SortBy::Modified { primary } else { text_sec }),
                    ]),
                ]),
                div().h(Px(1.0)).shrink(0.0).bg(c(t.border)),
            ]);

            // 可視範囲はランタイムの計測から。 上下に spacer を積むので、
            // スクロール量が実データの長さと一致する。
            let (first, count) = ctx.visible_range(FILE_SCROLL_ID, ROW_H);
            let last = (first + count).min(tab.filtered.len());
            let first = first.min(last);

            let mut file_rows: Vec<Element> = Vec::new();
            for fi in first..last {
                let real_idx = tab.filtered[fi];
                let file = &tab.files[real_idx];
                let id = format!("f-{fi}");
                let is_selected = tab.selected.contains(&real_idx);
                let is_hovered = ctx.hovered.as_deref() == Some(id.as_str());
                let is_drag_active = self.drag.as_ref().map_or(false, |d| d.active);
                let is_drop_target = is_drag_active && is_hovered && file.is_dir;
                let is_being_dragged = is_drag_active && is_selected;

                let is_odd = fi % 2 == 1;
                let stripe_bg = if is_odd { c(t.surface).with_alpha(0.15) } else { Color::TRANSPARENT };
                let row_bg = if is_drop_target { primary.with_alpha(0.3) }
                    else if is_being_dragged { c(t.select_bg).with_alpha(0.15) }
                    else if is_selected { c(t.select_bg) }
                    else if is_hovered { c(t.hover_bg) }
                    else { stripe_bg };
                let row_border = if is_drop_target { primary.with_alpha(0.6) } else { Color::TRANSPARENT };
                let row_shadow = if is_selected && !is_being_dragged {
                    Some(sabitori::element::BoxShadow {
                        color: primary.with_alpha(0.15),
                        offset: sabitori::Point::new(0.0, 0.0),
                        blur: 8.0, spread: 0.0,
                    })
                } else { None };
                let name_color = if is_selected { text_pri }
                    else if file.is_dir { primary }
                    else if file.is_hidden { text_sec.with_alpha(0.4) }
                    else { text_pri };
                let name_color = if is_being_dragged { name_color.with_alpha(0.4) } else { name_color };
                let meta_color = if is_selected { text_sec.lighten(0.3) } else { text_sec };
                let size_str = if file.is_dir { "\u{2014}".to_string() } else { file_browser::format_size(file.size) };
                let mod_str = file_browser::format_modified(file.modified);

                let mut row = div().id(id).h(Px(ROW_H)).shrink(0.0)
                    .bg(row_bg).rounded_px(6.0).mx(Px(4.0))
                    .border(if is_drop_target { 1.0 } else { 0.0 }, row_border);
                if let Some(s) = row_shadow { row = row.shadow(s); }
                file_rows.push(
                    row
                        .flex_row().items_center().px_pad(Px(8.0)).gap(6.0)
                        .children([
                            file_icon(file, primary),
                            text(&file.name).font_size(12.0).color(name_color).flex_1(),
                            text(&size_str).font_size(12.0).color(meta_color).w(Px(80.0)).shrink(0.0),
                            text(&mod_str).font_size(12.0).color(meta_color).w(Px(130.0)).shrink(0.0),
                        ]),
                );
            }

            let content_h = tab.filtered.len() as f32 * ROW_H;
            let mut rows: Vec<Element> = Vec::with_capacity(file_rows.len() + 2);
            if first > 0 {
                rows.push(div().h(Px(first as f32 * ROW_H)).shrink(0.0));
            }
            rows.extend(file_rows);
            let tail = tab.filtered.len().saturating_sub(last);
            if tail > 0 {
                rows.push(div().h(Px(tail as f32 * ROW_H)).shrink(0.0));
            }

            div().flex_1().flex_col().bg(bg).children([
                col_header,
                div().flex_1().flex_row().children([
                    // ここが唯一のスクロール宣言。 ホイールも慣性も位置の保持も
                    // ランタイムが持つ。
                    div().scroll(FILE_SCROLL_ID).flex_1().flex_col().children(rows),
                    scrollbar_for(content_h),
                ]),
            ])
        } else {
            // Grid view
            const GW: f32 = 90.0;
            const GH: f32 = 90.0;
            const GIS: f32 = 44.0;
            const GG: f32 = 4.0;
            const GP: f32 = 12.0;
            let avail_w = ctx.width - self.sidebar_width - 7.0 - GP * 2.0
                - if self.show_settings { SETTINGS_W + 1.0 } else { 0.0 };
            let cols = ((avail_w + GG) / (GW + GG)).floor().max(1.0) as usize;
            let total_rows = (tab.filtered.len() + cols - 1) / cols;
            let row_h = GH + GG;
            let (first_row, visible_rows) = ctx.visible_range(FILE_SCROLL_ID, row_h);

            let mut grid_rows: Vec<Element> = Vec::new();
            for row in first_row..(first_row + visible_rows).min(total_rows) {
                let mut items: Vec<Element> = Vec::new();
                for col in 0..cols {
                    let fi = row * cols + col;
                    if fi >= tab.filtered.len() { break; }
                    let real_idx = tab.filtered[fi];
                    let file = &tab.files[real_idx];
                    let id = format!("f-{fi}");
                    let is_selected = tab.selected.contains(&real_idx);
                    let is_hovered = ctx.hovered.as_deref() == Some(id.as_str());
                    let card_bg = if is_selected { c(t.select_bg) } else if is_hovered { c(t.hover_bg) } else { Color::TRANSPARENT };
                    let name_color = if is_selected { text_pri }
                        else if file.is_dir { primary }
                        else if file.is_hidden { text_sec.with_alpha(0.4) }
                        else { text_pri };

                    let large_icon = if file.is_dir {
                        div().w(Px(GIS)).h(Px(GIS * 0.75)).shrink(0.0).flex_col().children([
                            div().w(Px(GIS * 0.4)).h(Px(7.0)).shrink(0.0).bg(primary.with_alpha(0.6))
                                .corner_radius(Corners { top_left: 4.0, top_right: 4.0, bottom_left: 0.0, bottom_right: 0.0 }),
                            div().w(Px(GIS)).flex_1().bg(primary.with_alpha(0.25))
                                .corner_radius(Corners { top_left: 0.0, top_right: 4.0, bottom_left: 4.0, bottom_right: 4.0 }),
                        ])
                    } else {
                        let (ch, ic) = file_type_info(file);
                        text(ic).font_size(GIS * 0.7).color(c(ch))
                    };

                    let dn = if file.name.len() > 12 { format!("{}...", &file.name[..9]) } else { file.name.clone() };
                    items.push(
                        div().id(id).w(Px(GW)).h(Px(GH)).shrink(0.0)
                            .bg(card_bg).rounded_px(8.0)
                            .flex_col().items_center().justify_center().gap(4.0)
                            .children([large_icon, text(&dn).font_size(10.0).color(name_color)]),
                    );
                }
                grid_rows.push(div().h(Px(GH)).shrink(0.0).flex_row().gap(GG).px_pad(Px(GP)).children(items));
            }

            let last_row = (first_row + visible_rows).min(total_rows);
            let content_h = total_rows as f32 * row_h + GP;
            let mut rows: Vec<Element> = Vec::with_capacity(grid_rows.len() + 2);
            if first_row > 0 {
                rows.push(div().h(Px(first_row as f32 * row_h)).shrink(0.0));
            }
            rows.extend(grid_rows);
            let tail_rows = total_rows.saturating_sub(last_row);
            if tail_rows > 0 {
                rows.push(div().h(Px(tail_rows as f32 * row_h)).shrink(0.0));
            }

            div().flex_1().flex_col().bg(bg).children([
                div().flex_1().flex_row().children([
                    div().scroll(FILE_SCROLL_ID).flex_1().flex_col().pt(Px(GP)).gap(GG).children(rows),
                    scrollbar_for(content_h),
                ]),
            ])
        };

        // ── Status bar ──
        let status_bar = div().h(Px(STATUS_H)).shrink(0.0).bg(elevated).flex_col().children([
            div().h(Px(1.0)).shrink(0.0).bg(c(t.border)),
            div().flex_1().flex_row().items_center().justify_between().px_pad(Px(16.0)).children([
                text(&if sel_count > 0 { format!("{sel_count} selected") } else { String::new() })
                    .font_size(11.0).color(text_sec),
                text(self.theme().name).font_size(11.0).color(text_sec.with_alpha(0.4)),
            ]),
        ]);

        // ── Assemble ──
        let mut content: Vec<Element> = vec![sidebar, divider, file_area];
        if self.show_settings {
            content.push(div().w(Px(1.0)).shrink(0.0).bg(c(t.border)));
            content.push(self.settings_panel(ctx));
        }

        let mut main_children: Vec<Element> = vec![header, toolbar];
        if self.tabs.len() > 1 { main_children.push(tab_bar); }
        main_children.push(div().flex_1().flex_row().children(content));
        main_children.push(status_bar);

        div().w(Px(ctx.width)).h(Px(ctx.height)).bg(bg)
            .rounded_px(10.0).overflow_hidden()
            .flex_col().children(main_children)
    }

    fn on_click(&mut self, id: &str) {
        // Window controls
        if id == "win-close" {
            std::process::exit(0);
        }
        if id == "win-mini" {
            if let Some(ref w) = self.window { w.set_minimized(true); }
            return;
        }
        if id == "win-zoom" {
            if let Some(ref w) = self.window {
                w.set_maximized(!w.is_maximized());
            }
            return;
        }
        if id == "title-bar" {
            if let Some(ref w) = self.window { let _ = w.drag_window(); }
            return;
        }

        // Clear any stale drag state on new click
        self.drag = None;

        // Conflict modal
        if self.conflict_modal.is_some() {
            match id {
                "conflict-replace" => {
                    let apply_all = self.conflict_modal.as_ref().and_then(|m| m.apply_all);
                    if apply_all.is_some() {
                        if let Some(ref mut m) = self.conflict_modal { m.apply_all = Some(ConflictChoice::Replace); }
                    }
                    self.resolve_conflict(ConflictChoice::Replace);
                }
                "conflict-keep" => {
                    let apply_all = self.conflict_modal.as_ref().and_then(|m| m.apply_all);
                    if apply_all.is_some() {
                        if let Some(ref mut m) = self.conflict_modal { m.apply_all = Some(ConflictChoice::KeepBoth); }
                    }
                    self.resolve_conflict(ConflictChoice::KeepBoth);
                }
                "conflict-skip" => {
                    let apply_all = self.conflict_modal.as_ref().and_then(|m| m.apply_all);
                    if apply_all.is_some() {
                        if let Some(ref mut m) = self.conflict_modal { m.apply_all = Some(ConflictChoice::Skip); }
                    }
                    self.resolve_conflict(ConflictChoice::Skip);
                }
                "conflict-apply-all" => {
                    if let Some(ref mut modal) = self.conflict_modal {
                        // Toggle: if already set, clear it. If not, pre-set to Skip (any next click sets the actual choice)
                        modal.apply_all = if modal.apply_all.is_some() { None } else { Some(ConflictChoice::Skip) };
                    }
                }
                "conflict-backdrop" => { self.conflict_modal = None; } // dismiss
                _ => {}
            }
            return;
        }

        // Context menu actions
        if self.ctx_menu.is_some() {
            self.ctx_menu = None;
            match id {
                "ctx-open" => {
                    if let Some(f) = self.selected_file() {
                        if f.is_dir {
                            let p = f.path.clone(); self.navigate_to(p);
                        } else {
                            let _ = std::process::Command::new("open").arg(&f.path).spawn();
                        }
                    }
                }
                "ctx-open-new" => {
                    if let Some(f) = self.selected_file() {
                        let target = if f.is_dir { f.path.clone() } else { f.path.parent().unwrap_or(f.path.as_path()).to_path_buf() };
                        let exe = std::env::current_exe().unwrap_or_default();
                        let _ = std::process::Command::new(exe)
                            .env("SABITORI_START_DIR", &target)
                            .spawn();
                    }
                }
                "ctx-copy" => self.copy_selected(),
                "ctx-cut" => self.cut_selected(),
                "ctx-paste" => self.paste_files(),
                "ctx-trash" => self.trash_selected(),
                "ctx-rename" => self.start_rename(),
                "ctx-newfolder" => self.new_folder(),
                "ctx-selectall" => {
                    let tab = self.tab_mut();
                    tab.selected = tab.filtered.iter().copied().collect();
                }
                _ => {} // backdrop or unknown → just close
            }
            return;
        }

        // Rename mode
        if self.renaming.is_some() {
            // Clicking outside cancels rename
            self.cancel_rename();
        }

        let shift = self.last_shift.get();
        let cmd = self.last_cmd.get();

        // Settings
        if id == "settings-btn" { self.show_settings = !self.show_settings; return; }
        if id == "settings-close" { self.show_settings = false; return; }
        if id == "opacity-track" { self.opacity_dragging = true; return; }

        // View mode / hidden / search
        if id == "view-list" { self.view_mode = ViewMode::List; self.update_grid_scroll(); return; }
        if id == "view-grid" { self.view_mode = ViewMode::Grid; self.update_grid_scroll(); return; }
        if id == "toggle-hidden" {
            self.show_hidden = !self.show_hidden;
            self.refresh_all_tabs();
            return;
        }
        if id == "search-btn" {
            self.search_active = !self.search_active;
            if !self.search_active { self.search_query.clear(); self.tab_mut().apply_filter(""); }
            return;
        }

        // Sidebar drag
        if id == "sidebar-drag" { self.sidebar_dragging = true; return; }

        // Sort
        if id == "sort-name" { self.toggle_sort(SortBy::Name); return; }
        if id == "sort-size" { self.toggle_sort(SortBy::Size); return; }
        if id == "sort-mod" { self.toggle_sort(SortBy::Modified); return; }

        // Tabs
        if id == "tab-new" { self.new_tab(); return; }
        if let Some(idx_str) = id.strip_prefix("tab-close-") {
            if let Ok(idx) = idx_str.parse::<usize>() { self.close_tab(idx); }
            return;
        }
        if let Some(idx_str) = id.strip_prefix("tab-") {
            if let Ok(idx) = idx_str.parse::<usize>() {
                if idx < self.tabs.len() { self.active_tab = idx; }
            }
            return;
        }

        // Theme
        if let Some(idx_str) = id.strip_prefix("theme-") {
            if let Ok(idx) = idx_str.parse::<usize>() {
                if idx < THEMES.len() { self.theme_idx = idx; self.bg_opacity = THEMES[idx].bg_opacity; }
            }
            return;
        }

        // Back/Forward
        if id == "nav-back" { self.go_back(); return; }
        if id == "nav-fwd" { self.go_forward(); return; }

        // Breadcrumb navigation
        if let Some(idx_str) = id.strip_prefix("crumb-") {
            if let Ok(idx) = idx_str.parse::<usize>() {
                let tab = self.tab();
                let home = file_browser::home_dir();
                let path = &tab.path;
                let components: Vec<PathBuf> = if let Ok(rel) = path.strip_prefix(&home) {
                    let mut v = vec![home.clone()];
                    let mut acc = home.clone();
                    for comp in rel.components() {
                        acc = acc.join(comp);
                        v.push(acc.clone());
                    }
                    v
                } else {
                    let mut v = vec![PathBuf::from("/")];
                    let mut acc = PathBuf::from("/");
                    for comp in path.components().skip(1) {
                        acc = acc.join(comp);
                        v.push(acc.clone());
                    }
                    v
                };
                if let Some(target) = components.get(idx) {
                    let p = target.clone();
                    self.navigate_to(p);
                }
            }
            return;
        }

        // Sidebar bookmarks
        if let Some(idx_str) = id.strip_prefix("sb-") {
            if let Ok(idx) = idx_str.parse::<usize>() {
                if let Some((_, path)) = self.bookmarks.get(idx) {
                    let p = path.clone(); self.navigate_to(p);
                }
            }
            return;
        }

        // Files
        if let Some(idx_str) = id.strip_prefix("f-") {
            if let Ok(fi) = idx_str.parse::<usize>() {
                if fi >= self.tab().filtered.len() { return; }
                let real_idx = self.tab().filtered[fi];
                let now = std::time::Instant::now();

                let is_double = if let Some((prev_fi, prev_time)) = self.last_click {
                    prev_fi == fi && now.duration_since(prev_time).as_millis() < 400
                } else { false };

                if is_double && !shift && !cmd {
                    self.last_click = None;
                    let file = self.tab().files.get(real_idx).cloned();
                    if let Some(file) = file {
                        if file.is_dir {
                            self.navigate_to(file.path);
                        } else {
                            let _ = std::process::Command::new("open").arg(&file.path).spawn();
                        }
                    }
                } else {
                    self.last_click = Some((fi, now));
                    self.select_file(fi, shift, cmd);
                    // Start potential drag from click position
                    let (mx, my) = self.last_mouse.get();
                    let indices: Vec<usize> = self.tab().selected.iter().copied().collect();
                    self.drag = Some(DragState {
                        file_indices: indices,
                        start_x: mx,
                        start_y: my,
                        active: false,
                        created: std::time::Instant::now(),
                    });
                }
            }
        }
    }

    fn on_right_click(&mut self, id: &str, x: f32, y: f32) {
        // If right-clicking a file, select it first
        if let Some(idx_str) = id.strip_prefix("f-") {
            if let Ok(fi) = idx_str.parse::<usize>() {
                let tab = self.tab();
                if fi < tab.filtered.len() {
                    let real_idx = tab.filtered[fi];
                    if !tab.selected.contains(&real_idx) {
                        let tab = self.tab_mut();
                        tab.selected.clear();
                        tab.selected.insert(real_idx);
                        tab.last_selected = Some(real_idx);
                    }
                }
                self.ctx_menu = Some(CtxMenu { x, y, on_file: true });
                return;
            }
        }
        // Right-click on empty area
        self.ctx_menu = Some(CtxMenu { x, y, on_file: false });
    }

    fn on_pointer_move(&mut self, x: f32, y: f32) {
        if self.opacity_dragging {
            self.bg_opacity = self.opacity_from_mouse(x, self.last_width.get());
        }
        if self.sidebar_dragging {
            self.sidebar_width = x.clamp(MIN_SIDEBAR_W, MAX_SIDEBAR_W);
        }
        // Drag detection: activate after 3px threshold from click position
        if let Some(ref mut drag) = self.drag {
            if !drag.active {
                let dx = x - drag.start_x;
                let dy = y - drag.start_y;
                if (dx * dx + dy * dy) > 25.0 { // 5px threshold
                    drag.active = true;
                    // Copy file paths to system clipboard for cross-window paste
                    let tab = &self.tabs[self.active_tab];
                    let paths: Vec<&Path> = drag.file_indices.iter()
                        .filter_map(|&i| tab.files.get(i).map(|f| f.path.as_path()))
                        .collect();
                    sabitori::macos_drag::copy_paths_to_clipboard(&paths);
                }
            }
        }
    }

    fn on_pointer_up(&mut self) {
        self.opacity_dragging = false;
        self.sidebar_dragging = false;

        // Drop: if dragging, check target
        if let Some(drag) = self.drag.take() {
            if drag.active && !drag.file_indices.is_empty() {
                let dropped = self.handle_drop(&drag.file_indices);
                if !dropped {
                    // No valid drop target — show clipboard toast
                    let count = drag.file_indices.len();
                    let msg = format!("{count} file{} copied to clipboard — Cmd+V to paste",
                        if count > 1 { "s" } else { "" });
                    self.toast = Some((msg, std::time::Instant::now()));
                }
            }
        }
    }

    fn set_window(&mut self, window: std::sync::Arc<winit::window::Window>) {
        self.window = Some(window);
    }

    fn on_cursor_left(&mut self) {
        // If dragging files and cursor leaves the window → start OS-level drag
        if let Some(drag) = self.drag.take() {
            if drag.active {
                if let Some(ref window) = self.window {
                    let tab = self.tab();
                    let paths: Vec<PathBuf> = drag.file_indices.iter()
                        .filter_map(|&i| tab.files.get(i).map(|f| f.path.clone()))
                        .collect();
                    let path_refs: Vec<&Path> = paths.iter().map(|p| p.as_path()).collect();
                    sabitori::macos_drag::start_file_drag(window, &path_refs);
                }
            }
            // Always clear drag when cursor leaves
        }
    }

    fn on_file_hover(&mut self, path: PathBuf) {
        if !self.hover_files.contains(&path) {
            self.hover_files.push(path);
        }
    }

    fn on_file_hover_cancelled(&mut self) {
        self.hover_files.clear();
    }

    fn on_file_drop(&mut self, paths: Vec<PathBuf>) {
        self.hover_files.clear();
        let dest = self.tab().path.clone();
        self.transfer_files(paths, dest, false); // external drop = copy
    }

    // `on_scroll` は実装しない。 ホイールはランタイムがポインタ直下の
    // `.scroll(id)` コンテナへ配る。 自前で受けて足し込むと二重に動く。

    /// 溜めておいたスクロール要求をランタイムへ渡す。 `.scroll(id)` コンテナへの
    /// プログラム的なスクロールはこの口から行う (自分で位置を書き換えない)。
    fn scroll_intents(&mut self) -> Vec<(String, f32)> {
        self.tabs
            .iter_mut()
            .filter_map(|t| t.pending_scroll.take())
            .map(|y| (FILE_SCROLL_ID.to_string(), y))
            .collect()
    }

    fn on_navigate_back(&mut self) { self.go_back(); }
    fn on_navigate_forward(&mut self) { self.go_forward(); }

    fn on_input(&mut self, event: &InputEvent) -> bool {
        // Close context menu on any key
        if self.ctx_menu.is_some() {
            self.ctx_menu = None;
            return true;
        }

        // Rename mode
        if self.renaming.is_some() {
            match event {
                InputEvent::CharInput(ch) if !ch.is_control() => {
                    if let Some((_, ref mut name)) = self.renaming { name.push(*ch); }
                    return true;
                }
                InputEvent::KeyInput { key: Key::Backspace, pressed: true, .. } => {
                    if let Some((_, ref mut name)) = self.renaming { name.pop(); }
                    return true;
                }
                InputEvent::KeyInput { key: Key::Enter, pressed: true, .. } => {
                    self.confirm_rename();
                    return true;
                }
                InputEvent::KeyInput { key: Key::Escape, pressed: true, .. } => {
                    self.cancel_rename();
                    return true;
                }
                _ => return false,
            }
        }

        // Search text input
        if self.search_active {
            match event {
                InputEvent::CharInput(ch) if !ch.is_control() => {
                    self.search_query.push(*ch);
                    let q = self.search_query.clone();
                    self.tab_mut().apply_filter(&q);
                    return true;
                }
                InputEvent::KeyInput { key: Key::Backspace, pressed: true, .. } => {
                    self.search_query.pop();
                    let q = self.search_query.clone();
                    self.tab_mut().apply_filter(&q);
                    return true;
                }
                InputEvent::KeyInput { key: Key::Escape, pressed: true, .. } => {
                    self.search_active = false;
                    self.search_query.clear();
                    self.tab_mut().apply_filter("");
                    return true;
                }
                _ => {}
            }
        }

        if let InputEvent::KeyInput { key, pressed: true, modifiers, .. } = event {
            match key {
                Key::Backspace => {
                    if let Some(parent) = self.tab().path.parent().map(|p| p.to_path_buf()) {
                        self.navigate_to(parent);
                    }
                    return true;
                }
                Key::Enter => {
                    let tab = self.tab();
                    if let Some(&real_idx) = tab.selected.iter().next() {
                        if tab.selected.len() == 1 {
                            if let Some(file) = tab.files.get(real_idx) {
                                if file.is_dir {
                                    let p = file.path.clone(); self.navigate_to(p);
                                } else {
                                    let _ = std::process::Command::new("open").arg(&file.path).spawn();
                                }
                            }
                        }
                    }
                    return true;
                }
                Key::Down => {
                    self.move_selection(1, modifiers.shift);
                    self.ql_update_if_open();
                    return true;
                }
                Key::Up => {
                    self.move_selection(-1, modifiers.shift);
                    self.ql_update_if_open();
                    return true;
                }
                Key::Space => { self.ql_toggle(); return true; }
                Key::Escape => {
                    if self.ql_open { self.ql_close(); return true; }
                    if self.show_settings { self.show_settings = false; return true; }
                }
                Key::A if modifiers.meta => {
                    let tab = self.tab_mut();
                    tab.selected = tab.filtered.iter().copied().collect();
                    return true;
                }
                Key::Left if modifiers.meta => { self.go_back(); return true; }
                Key::Right if modifiers.meta => { self.go_forward(); return true; }
                Key::C if modifiers.meta => { self.copy_selected(); return true; }
                Key::X if modifiers.meta => { self.cut_selected(); return true; }
                Key::V if modifiers.meta => { self.paste_files(); return true; }
                Key::Backspace if modifiers.meta => { self.trash_selected(); return true; }
                _ => {}
            }
        }

        if let InputEvent::CharInput(ch) = event {
            let cmd = self.last_cmd.get();
            if cmd {
                match ch {
                    'n' | 'N' => {
                        // New window: spawn another filer process
                        let exe = std::env::current_exe().unwrap_or_default();
                        let _ = std::process::Command::new(exe).spawn();
                        return true;
                    }
                    't' | 'T' => { self.new_tab(); return true; }
                    'w' | 'W' => { self.close_tab(self.active_tab); return true; }
                    '[' => { self.go_back(); return true; }
                    ']' => { self.go_forward(); return true; }
                    _ => {}
                }
            }
            if *ch == '/' && !self.search_active {
                self.search_active = true;
                self.search_query.clear();
                return true;
            }
        }

        false
    }
}

impl FilerApp {
    fn toggle_sort(&mut self, col: SortBy) {
        if self.sort_by == col {
            self.sort_order = match self.sort_order {
                SortOrder::Ascending => SortOrder::Descending,
                SortOrder::Descending => SortOrder::Ascending,
            };
        } else {
            self.sort_by = col;
            self.sort_order = SortOrder::Ascending;
        }
        self.refresh_all_tabs();
    }

    /// 表示モードを切り替えたら先頭へ戻す。 中身の高さはレイアウトが測るので、
    /// アプリが content_height を計算して渡す必要は無くなった。
    fn update_grid_scroll(&mut self) {
        self.tab_mut().pending_scroll = Some(0.0);
    }

    fn move_selection(&mut self, delta: i32, extend: bool) {
        let last_scroll = self.last_scroll.get();
        let tab = self.tab_mut();
        if tab.filtered.is_empty() { return; }
        let max = tab.filtered.len() - 1;

        let current = tab.last_selected
            .and_then(|ri| tab.filtered.iter().position(|&i| i == ri))
            .unwrap_or(0);
        let new_fi = if delta > 0 { (current + delta as usize).min(max) }
            else { current.saturating_sub((-delta) as usize) };
        let new_real = tab.filtered[new_fi];

        if extend {
            tab.selected.insert(new_real);
        } else {
            tab.selected.clear();
            tab.selected.insert(new_real);
        }
        tab.last_selected = Some(new_real);

        // Auto-scroll: 選択が画面外なら見える位置を要求する。 いまのスクロール
        // 位置と viewport は前フレームの計測値 (`ScrollInfo`) から取る。
        let sel_y = new_fi as f32 * ROW_H;
        let (scroll_y, viewport) = last_scroll;
        if viewport > 0.0 {
            if sel_y + ROW_H > scroll_y + viewport {
                tab.pending_scroll = Some(sel_y + ROW_H - viewport);
            } else if sel_y < scroll_y {
                tab.pending_scroll = Some(sel_y);
            }
        }
    }
}

fn main() {
    sabitori::run_declarative(FilerApp::new());
}
