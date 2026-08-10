use std::path::{Path, PathBuf};
use std::time::SystemTime;

/// A file/directory entry with metadata.
#[derive(Clone, Debug)]
pub struct FileEntry {
    pub name: String,
    pub path: PathBuf,
    pub is_dir: bool,
    pub is_hidden: bool,
    pub is_symlink: bool,
    pub size: u64,
    pub modified: Option<SystemTime>,
}

/// Sort criteria.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SortBy {
    #[default]
    Name,
    Size,
    Modified,
    Kind,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SortOrder {
    #[default]
    Ascending,
    Descending,
}

/// Read directory contents. Directories are listed first.
pub fn read_directory(
    path: &Path,
    show_hidden: bool,
    sort_by: SortBy,
    sort_order: SortOrder,
) -> Vec<FileEntry> {
    let read = match std::fs::read_dir(path) {
        Ok(r) => r,
        Err(_) => return Vec::new(),
    };

    let mut entries: Vec<FileEntry> = read
        .filter_map(|e| e.ok())
        .filter_map(|entry| {
            let name = entry.file_name().to_string_lossy().to_string();
            let is_hidden = name.starts_with('.');
            if !show_hidden && is_hidden {
                return None;
            }
            let meta = entry.metadata().ok()?;
            let is_symlink = entry.file_type().ok().map_or(false, |ft| ft.is_symlink());
            Some(FileEntry {
                path: entry.path(),
                name,
                is_dir: meta.is_dir(),
                is_hidden,
                is_symlink,
                size: meta.len(),
                modified: meta.modified().ok(),
            })
        })
        .collect();

    // Sort: directories first, then by criteria
    entries.sort_by(|a, b| {
        // Dirs first
        let dir_cmp = b.is_dir.cmp(&a.is_dir);
        if dir_cmp != std::cmp::Ordering::Equal {
            return dir_cmp;
        }
        let ord = match sort_by {
            SortBy::Name => a.name.to_lowercase().cmp(&b.name.to_lowercase()),
            SortBy::Size => a.size.cmp(&b.size),
            SortBy::Modified => a.modified.cmp(&b.modified),
            SortBy::Kind => {
                let ext_a = a.path.extension().map(|e| e.to_string_lossy().to_string()).unwrap_or_default();
                let ext_b = b.path.extension().map(|e| e.to_string_lossy().to_string()).unwrap_or_default();
                ext_a.cmp(&ext_b)
            }
        };
        match sort_order {
            SortOrder::Ascending => ord,
            SortOrder::Descending => ord.reverse(),
        }
    });

    entries
}

/// Format file size as human-readable string.
pub fn format_size(bytes: u64) -> String {
    if bytes == 0 {
        return "—".to_string();
    }
    const UNITS: &[&str] = &["B", "KB", "MB", "GB", "TB"];
    let mut size = bytes as f64;
    let mut unit = 0;
    while size >= 1024.0 && unit < UNITS.len() - 1 {
        size /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{} B", bytes)
    } else {
        format!("{:.1} {}", size, UNITS[unit])
    }
}

/// Format modification time as absolute date/time.
pub fn format_modified(time: Option<SystemTime>) -> String {
    match time {
        None => "—".to_string(),
        Some(t) => {
            let duration = t.duration_since(SystemTime::UNIX_EPOCH).unwrap_or_default();
            let secs = duration.as_secs() as i64;

            // Local time: use libc to get timezone offset
            let local_secs = secs + local_utc_offset();
            let days = local_secs / 86400;
            let time_of_day = local_secs % 86400;
            let hours = time_of_day / 3600;
            let minutes = (time_of_day % 3600) / 60;

            // Days since 1970-01-01 to Y/M/D
            let (year, month, day) = days_to_ymd(days);

            format!("{year}/{month:02}/{day:02} {hours:02}:{minutes:02}")
        }
    }
}

/// Convert days since Unix epoch to (year, month, day).
fn days_to_ymd(days: i64) -> (i64, u32, u32) {
    let z = days + 719468;
    let era = if z >= 0 { z } else { z - 146096 } / 146097;
    let doe = (z - era * 146097) as u32;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    (y, m, d)
}

/// Get the local timezone offset from UTC in seconds.
///
/// `localtime_r` / `tm_gmtoff` are glibc/BSD extensions that do not exist on
/// Windows, so the two platforms need different calls. Both paths account for
/// DST because they ask the C runtime for the *current* local time.
#[cfg(all(not(target_arch = "wasm32"), unix))]
fn local_utc_offset() -> i64 {
    unsafe extern "C" {
        fn time(t: *mut i64) -> i64;
        fn localtime_r(t: *const i64, result: *mut libc_tm) -> *mut libc_tm;
    }
    #[repr(C)]
    struct libc_tm {
        tm_sec: i32, tm_min: i32, tm_hour: i32,
        tm_mday: i32, tm_mon: i32, tm_year: i32,
        tm_wday: i32, tm_yday: i32, tm_isdst: i32,
        /// glibc/BSD declare this as `long`, which is 32-bit on 32-bit targets.
        /// Hard-coding `i64` there would shift every following field and read
        /// garbage, so track the C type.
        tm_gmtoff: std::ffi::c_long,
        _tm_zone: *const u8,
    }
    unsafe {
        let mut now: i64 = 0;
        time(&mut now);
        let mut tm = std::mem::zeroed::<libc_tm>();
        localtime_r(&now, &mut tm);
        tm.tm_gmtoff as i64
    }
}

/// Windows: `struct tm` has no `tm_gmtoff`, so derive the offset by taking the
/// local broken-down time and re-interpreting it as UTC — the difference from
/// the real timestamp is the offset east of UTC.
///
/// Every name here carries its `64` suffix on purpose. The plain spellings
/// (`time`, `localtime_s`, `_mkgmtime`) are not functions the UCRT exports —
/// `<time.h>` defines them as `__inline` wrappers that pick the 32- or 64-bit
/// variant according to `_USE_32BIT_TIME_T`. C code links because the wrapper
/// is compiled into the caller; naming them from Rust asks the linker for a
/// symbol that was never emitted, and the build dies with
/// `LNK2019: unresolved external symbol localtime_s`. Suffixed names are the
/// real exports, and they also pin `time_t` to the `i64` used below.
#[cfg(all(not(target_arch = "wasm32"), windows))]
fn local_utc_offset() -> i64 {
    unsafe extern "C" {
        fn _time64(t: *mut i64) -> i64;
        /// MSVC: `errno_t _localtime64_s(struct tm* dest, const __time64_t* src)`
        /// (destination first — the argument order differs from C11 Annex K).
        fn _localtime64_s(result: *mut win_tm, t: *const i64) -> i32;
        /// Interprets the fields as UTC (the inverse of `gmtime`).
        fn _mkgmtime64(tm: *mut win_tm) -> i64;
    }
    #[repr(C)]
    struct win_tm {
        tm_sec: i32, tm_min: i32, tm_hour: i32,
        tm_mday: i32, tm_mon: i32, tm_year: i32,
        tm_wday: i32, tm_yday: i32, tm_isdst: i32,
    }
    unsafe {
        let mut now: i64 = 0;
        _time64(&mut now);
        let mut tm = std::mem::zeroed::<win_tm>();
        if _localtime64_s(&mut tm, &now) != 0 {
            return 0;
        }
        // `_localtime64_s` sets `tm_isdst > 0` during DST. We want these fields
        // read as a plain wall clock — UTC has no DST — and `_mkgmtime64`'s
        // handling of `tm_isdst` is not clearly specified. Zeroing it removes the
        // ambiguity: a no-op if the field is ignored, and it prevents a spurious
        // one-hour correction if it is not. Only observable in DST regions.
        tm.tm_isdst = 0;
        let as_utc = _mkgmtime64(&mut tm);
        if as_utc == -1 {
            return 0;
        }
        as_utc - now
    }
}

/// Neither unix nor Windows (wasm and friends): JST fallback, as before.
#[cfg(not(any(target_arch = "wasm32", unix, windows)))]
fn local_utc_offset() -> i64 {
    9 * 3600
}

#[cfg(target_arch = "wasm32")]
fn local_utc_offset() -> i64 {
    9 * 3600 // JST fallback
}

/// Build a TreeNode from a directory path (immediate children only).
pub fn dir_to_tree_node(path: &Path) -> crate::TreeNode {
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| path.to_string_lossy().to_string());

    let mut node = crate::TreeNode::new(name).with_icon("📁");

    // Read subdirectories only (for tree sidebar)
    if let Ok(entries) = std::fs::read_dir(path) {
        let mut children: Vec<crate::TreeNode> = entries
            .filter_map(|e| e.ok())
            .filter(|e| {
                let name = e.file_name().to_string_lossy().to_string();
                !name.starts_with('.') && e.metadata().ok().map_or(false, |m| m.is_dir())
            })
            .map(|e| {
                let child_name = e.file_name().to_string_lossy().to_string();
                // Don't recurse deep — just mark as having potential children
                let mut child = crate::TreeNode::new(child_name).with_icon("📁");
                // Check if this dir has subdirs (for expand arrow)
                if let Ok(sub) = std::fs::read_dir(e.path()) {
                    let has_subdirs = sub.filter_map(|s| s.ok()).any(|s| {
                        !s.file_name().to_string_lossy().starts_with('.')
                            && s.metadata().ok().map_or(false, |m| m.is_dir())
                    });
                    if has_subdirs {
                        // Add a placeholder child so expand arrow shows
                        child.children.push(crate::TreeNode::new("..."));
                    }
                }
                child
            })
            .collect();
        children.sort_by(|a, b| a.label.to_lowercase().cmp(&b.label.to_lowercase()));
        node = node.with_children(children);
    }

    node
}

/// Expand a tree node: replace placeholder children with real subdirectories.
pub fn expand_tree_node(node: &mut crate::TreeNode, base_path: &Path) {
    let dir_path = find_path_for_node(node, base_path);
    if let Some(path) = dir_path {
        let new_node = dir_to_tree_node(&path);
        node.children = new_node.children;
    }
}

fn find_path_for_node(node: &crate::TreeNode, base_path: &Path) -> Option<PathBuf> {
    let path = base_path.join(&node.label);
    if path.is_dir() { Some(path) } else { None }
}

/// Get default bookmark directories.
pub fn default_bookmarks() -> Vec<(String, PathBuf)> {
    let home = home_dir();
    vec![
        ("Home".to_string(), home.clone()),
        ("Desktop".to_string(), home.join("Desktop")),
        ("Documents".to_string(), home.join("Documents")),
        ("Downloads".to_string(), home.join("Downloads")),
    ]
}

/// Get home directory.
pub fn home_dir() -> PathBuf {
    std::env::var("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("/"))
}

/// Get a display-friendly path string (replace home with ~).
pub fn display_path(path: &Path) -> String {
    let home = home_dir();
    if let Ok(relative) = path.strip_prefix(&home) {
        format!("~/{}", relative.display())
    } else {
        path.to_string_lossy().to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `local_utc_offset` has a separate implementation per platform, and the
    /// Windows one derives the offset arithmetically — the easy mistake there is
    /// getting the sign backwards (`now - as_utc` instead of `as_utc - now`),
    /// which still produces a plausible-looking number. This runs on whichever
    /// implementation the host compiles, so CI covers each one in turn.
    #[test]
    fn local_utc_offset_is_a_real_timezone_offset() {
        let off = local_utc_offset();
        // Real zones span UTC-12:00 (Baker Island) .. UTC+14:00 (Kiritimati).
        // A flipped sign puts any non-UTC zone outside its own half of this
        // range; a units mix-up (minutes/ms) blows past it entirely.
        assert!(
            (-12 * 3600..=14 * 3600).contains(&off),
            "offset {off}s is outside the real UTC-12:00..=UTC+14:00 range"
        );
        // Every zone in the IANA database is a whole number of minutes.
        assert_eq!(off % 60, 0, "offset {off}s is not a whole number of minutes");
    }

    /// The Windows branch cannot be link-tested from any other host, and it
    /// fails at *link* time rather than compile time — so a Mac or Linux build
    /// stays green while Windows dies with
    /// `LNK2019: unresolved external symbol localtime_s`. The cause is that the
    /// UCRT exports `_time64` / `_localtime64_s` / `_mkgmtime64`; the plain
    /// spellings are `__inline` wrappers living in `<time.h>`, which C callers
    /// compile into themselves and Rust cannot. Reading our own source is the
    /// only check that runs on every host.
    #[test]
    fn the_windows_branch_names_only_exported_crt_symbols() {
        let src = include_str!("file_browser.rs");
        let start = src
            .find(r#"#[cfg(all(not(target_arch = "wasm32"), windows))]"#)
            .expect("the Windows local_utc_offset lost its cfg attribute");
        let rest = &src[start + 1..];
        let end = rest.find("#[cfg(").map(|i| start + 1 + i).unwrap_or(src.len());
        let block = &src[start..end];

        for exported in ["fn _time64(", "fn _localtime64_s(", "fn _mkgmtime64("] {
            assert!(block.contains(exported), "the Windows branch stopped declaring `{exported}`");
        }
        for inline_only in ["fn time(", "fn localtime_s(", "fn _mkgmtime("] {
            assert!(
                !block.contains(inline_only),
                "`{inline_only}` is a <time.h> inline, not a UCRT export — Windows will not link"
            );
        }
    }

    /// Guards the sign convention explicitly (east of UTC is positive), which is
    /// what `format_time` relies on when it adds the offset to a UTC timestamp.
    #[test]
    fn local_utc_offset_sign_matches_tz_env() {
        // Only assert when the host is running a zone we can name confidently.
        // Left permissive on purpose: CI machines are usually UTC.
        let off = local_utc_offset();
        if std::env::var("TZ").as_deref() == Ok("Asia/Tokyo") {
            assert_eq!(off, 9 * 3600, "Asia/Tokyo must be +9h, got {off}s");
        }
    }
}
