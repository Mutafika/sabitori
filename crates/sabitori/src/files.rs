//! ファイルを選ぶ・保存する ([#77](https://github.com/Mutafika/sabitori/issues/77))。
//!
//! 業務アプリで要るのは 2 つ:
//!
//! - **選ぶ** — バックアップ (.db / .zip) を選んでサーバーへ渡す
//! - **保存する** — CSV 出力、帳票のダウンロード
//!
//! ネイティブにはウィンドウへのドラッグ＆ドロップ (`on_file_drop`) しか無く、
//! 「ボタンを押して選ぶ」ができなかった。web には `on_file_drop` すら届かない。
//!
//! # 使い方
//!
//! どこからでも呼べる (クリック処理の中が普通)。結果は**後で**アプリに届く:
//!
//! ```ignore
//! button("リストア…").click(ctx, "restore", |_app: &mut App| {
//!     sabitori::files::pick("restore", PickOptions::new().accept([".db"]));
//! })
//!
//! // DeclarativeApp
//! fn on_files_picked(&mut self, key: &str, result: PickResult) {
//!     if key == "restore" {
//!         if let PickResult::Picked(files) = result {
//!             self.restore(&files[0].bytes);
//!         }
//!     }
//! }
//! ```
//!
//! 保存はその場で完結する:
//!
//! ```ignore
//! sabitori::files::save("vehicles_20260915.csv", &csv_bytes);
//! ```
//!
//! # 場所 (パス)
//!
//! native では選ばれたファイルの場所が [`PickedFile::path`] に入る。開いたファイルへ
//! ⌘S で書き戻す、次に開くときに同じフォルダから始める ([`PickOptions::start_in`])、
//! といったことができる。フォルダを選ばせるなら [`PickOptions::folders`]、大きな
//! ファイルの場所だけ欲しい (中身は読まない) なら [`PickOptions::paths_only`]。
//! 保存先を選ばせて、その場所を受け取るなら [`save_as`]:
//!
//! ```ignore
//! match files::save_as(&doc.name, &bytes, SaveOptions::new().start_in(&last_dir)) {
//!     SaveResult::Saved(Some(path)) => self.doc.path = Some(path), // 次の ⌘S はここへ
//!     SaveResult::Saved(None) => {}                                // web: ダウンロードした
//!     SaveResult::Failed(reason) => self.toast(format!("保存できませんでした: {reason}")),
//!     SaveResult::Cancelled | SaveResult::Unsupported => {}
//! }
//! ```
//!
//! # プラットフォーム
//!
//! | | 選ぶ | フォルダ | 場所 | 保存する |
//! |---|---|---|---|---|
//! | macOS | `NSOpenPanel` | ○ | ○ | `NSSavePanel` (場所を返す) |
//! | web | `<input type=file>` | **未対応** | 無し (ブラウザが渡さない) | Blob + `<a download>` |
//! | Windows / Linux | **未対応** ([`PickResult::Unsupported`]) | | | **未対応** |
//!
//! Windows / Linux は、入れるなら `rfd` を足すことになる (依存を足すかは別の判断として
//! 残してある)。**黙って何も起きない形にはしていない** — `Unsupported` が返るので、
//! アプリは「この環境では選べません」と出せる。

use std::cell::RefCell;
use std::path::{Path, PathBuf};

/// 選ばれたファイル (かフォルダ) 1 つ。
#[derive(Clone, Debug, PartialEq)]
pub struct PickedFile {
    /// 表示用の名前。
    pub name: String,
    /// 中身。フォルダを選ばせたとき ([`PickOptions::folders`]) は空。
    pub bytes: Vec<u8>,
    /// 場所。native だけ — web はブラウザがパスを渡さないので `None`。
    pub path: Option<PathBuf>,
}

impl PickedFile {
    /// 名前と中身から (場所は無し)。テストで結果を差し込むときに。
    pub fn new(name: impl Into<String>, bytes: Vec<u8>) -> Self {
        Self { name: name.into(), bytes, path: None }
    }

    /// 場所を付ける。
    pub fn with_path(mut self, path: impl Into<PathBuf>) -> Self {
        self.path = Some(path.into());
        self
    }
}

/// 選択の結果。
///
/// 「キャンセル」と「そもそもこの環境では選べない」を分ける。空の `Vec` に
/// まとめると、アプリが出すべき案内を決められない。
#[derive(Clone, Debug, PartialEq)]
pub enum PickResult {
    /// 選ばれた (1 つ以上)。
    Picked(Vec<PickedFile>),
    /// ユーザーが閉じた。
    Cancelled,
    /// この環境にファイル選択の口が無い。
    Unsupported,
}

/// 何を選ばせるか。
#[derive(Clone, Debug, Default, PartialEq)]
pub struct PickOptions {
    /// 受け付ける拡張子 (`".db"` のように点付き)。空なら制限しない。
    pub accept: Vec<String>,
    /// 複数選べるか。
    pub multiple: bool,
    /// ファイルではなくフォルダを選ばせる。
    pub folders: bool,
    /// 最初に開く場所。
    pub start_in: Option<PathBuf>,
    /// 中身を読まない ([`Self::paths_only`])。
    pub paths_only: bool,
}

impl PickOptions {
    pub fn new() -> Self {
        Self::default()
    }

    /// 受け付ける拡張子。
    pub fn accept<I, S>(mut self, exts: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.accept = exts.into_iter().map(Into::into).collect();
        self
    }

    /// 複数選択を許す。
    pub fn multiple(mut self) -> Self {
        self.multiple = true;
        self
    }

    /// ファイルではなくフォルダを選ばせる。中身は読まない (`bytes` は空、`path` に場所)。
    /// web は選べないので [`PickResult::Unsupported`]。
    pub fn folders(mut self) -> Self {
        self.folders = true;
        self
    }

    /// 最初に開く場所 (前に開いたフォルダなど)。web では効かない。
    pub fn start_in(mut self, dir: impl AsRef<Path>) -> Self {
        self.start_in = Some(dir.as_ref().to_path_buf());
        self
    }

    /// 中身を読まず、場所だけ受け取る (`bytes` は空、`path` に場所)。大きなファイル
    /// (動画など) を選ばせて自分で読む・場所だけ覚える、というときに — 付けないと
    /// 選ばれたファイルを丸ごとメモリに読む。web は場所を渡せないので効かない
    /// (今までどおり中身を読む)。
    pub fn paths_only(mut self) -> Self {
        self.paths_only = true;
        self
    }
}

/// 保存の結果 ([`save_as`])。
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SaveResult {
    /// 書いた。native は書いた場所、web は `None` (ダウンロードを始めた — 場所は
    /// ブラウザが決める)。
    Saved(Option<PathBuf>),
    /// ユーザーが閉じた。
    Cancelled,
    /// 保存先は選ばれたが書けなかった (権限が無い・空きが無いなど)。中身は理由 —
    /// 「保存できませんでした」と出す用。
    Failed(String),
    /// この環境に保存の口が無い。
    Unsupported,
}

/// 保存先の選び方 ([`save_as`])。
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SaveOptions {
    /// 最初に開く場所。
    pub start_in: Option<PathBuf>,
}

impl SaveOptions {
    pub fn new() -> Self {
        Self::default()
    }

    /// 最初に開く場所 (開いたファイルのフォルダなど)。web では効かない。
    pub fn start_in(mut self, dir: impl AsRef<Path>) -> Self {
        self.start_in = Some(dir.as_ref().to_path_buf());
        self
    }
}

thread_local! {
    /// 届いた結果。ランタイムが毎フレーム汲んで `on_files_picked` を呼ぶ。
    static RESULTS: RefCell<Vec<(String, PickResult)>> = const { RefCell::new(Vec::new()) };
}

/// 結果を積む。プラットフォーム実装と、テストの差し込み口が使う。
fn deliver(key: String, result: PickResult) {
    RESULTS.with(|r| r.borrow_mut().push((key, result)));
    #[cfg(target_arch = "wasm32")]
    crate::web_wake::wake();
}

/// ランタイムが毎フレーム汲む。
pub(crate) fn take_results() -> Vec<(String, PickResult)> {
    RESULTS.with(|r| std::mem::take(&mut *r.borrow_mut()))
}

/// **テストから「このファイルが選ばれた」を差し込む。**
///
/// ダイアログは OS のものなので、ヘッドレスのテストからは開けない。
/// `Harness` は結果だけを差し込んで、その先 (読み込み・復元・画面の更新) を
/// 確かめる。
pub fn test_deliver(key: impl Into<String>, result: PickResult) {
    deliver(key.into(), result);
}

/// ファイルを選ばせる。結果は [`DeclarativeApp::on_files_picked`] に届く。
///
/// `key` は「どの要求の答えか」を見分けるための札。アプリが決める。
///
/// [`DeclarativeApp::on_files_picked`]: crate::DeclarativeApp::on_files_picked
pub fn pick(key: impl Into<String>, options: PickOptions) {
    let key = key.into();
    #[cfg(all(target_os = "macos", not(target_arch = "wasm32")))]
    {
        deliver(key, macos::pick(&options));
    }
    #[cfg(target_arch = "wasm32")]
    {
        web::pick(key, options);
    }
    #[cfg(not(any(target_os = "macos", target_arch = "wasm32")))]
    {
        let _ = &options;
        tracing::warn!("files::pick はこの環境では未対応 (#77)");
        deliver(key, PickResult::Unsupported);
    }
}

/// ファイルを保存する。書き出しを始められたら `true`。
///
/// macOS は保存先を選ばせ、web はダウンロードさせる。書いた場所が要るなら [`save_as`]。
pub fn save(name: &str, bytes: &[u8]) -> bool {
    matches!(save_as(name, bytes, SaveOptions::new()), SaveResult::Saved(_))
}

/// 保存先を選ばせて書き、書いた場所を返す。`name` は最初に入っている名前。
///
/// 開いたファイルへ書き戻す (⌘S) なら、選ばせずに [`PickedFile::path`] へ
/// `std::fs::write` すればよい。これは「名前を付けて保存」の側。
pub fn save_as(name: &str, bytes: &[u8], options: SaveOptions) -> SaveResult {
    #[cfg(all(target_os = "macos", not(target_arch = "wasm32")))]
    {
        macos::save(name, bytes, &options)
    }
    #[cfg(target_arch = "wasm32")]
    {
        let _ = options;
        if web::save(name, bytes) {
            SaveResult::Saved(None)
        } else {
            SaveResult::Failed("ブラウザがダウンロードを始められなかった".into())
        }
    }
    #[cfg(not(any(target_os = "macos", target_arch = "wasm32")))]
    {
        let _ = (name, bytes, options);
        tracing::warn!("files::save はこの環境では未対応 (#77)");
        SaveResult::Unsupported
    }
}

/// 選ばれた場所へ書く。書けなかったら理由を付けて [`SaveResult::Failed`]。
#[cfg(not(target_arch = "wasm32"))]
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
fn write_to(path: PathBuf, bytes: &[u8]) -> SaveResult {
    match std::fs::write(&path, bytes) {
        Ok(()) => SaveResult::Saved(Some(path)),
        Err(e) => {
            tracing::warn!("保存に失敗: {e}");
            SaveResult::Failed(e.to_string())
        }
    }
}

// ---------------------------------------------------------------------------
// macOS
// ---------------------------------------------------------------------------

#[cfg(all(target_os = "macos", not(target_arch = "wasm32")))]
mod macos {
    use super::{write_to, PickOptions, PickResult, PickedFile, SaveOptions, SaveResult};
    use objc2::MainThreadMarker;
    use objc2_app_kit::{NSOpenPanel, NSSavePanel};
    use objc2_foundation::{NSArray, NSString, NSURL};

    /// 最初に開く場所を決める (NSOpenPanel も NSSavePanel の仲間)。
    fn start_in(panel: &NSSavePanel, dir: Option<&std::path::Path>) {
        if let Some(dir) = dir {
            let url = NSURL::fileURLWithPath_isDirectory(&NSString::from_str(&dir.to_string_lossy()), true);
            panel.setDirectoryURL(Some(&url));
        }
    }

    fn name_of(path: &std::path::Path) -> String {
        path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()
    }

    /// `runModal` は**メインスレッドでしか呼べない**。ランタイムのイベント
    /// ループがメインスレッドなので、クリック処理から呼ぶぶんには常に満たす。
    fn main_thread() -> Option<MainThreadMarker> {
        MainThreadMarker::new()
    }

    pub fn pick(options: &PickOptions) -> PickResult {
        let Some(mtm) = main_thread() else {
            tracing::warn!("files::pick をメインスレッド以外から呼んでいる");
            return PickResult::Unsupported;
        };
        let panel = NSOpenPanel::openPanel(mtm);
        panel.setAllowsMultipleSelection(options.multiple);
        panel.setCanChooseFiles(!options.folders);
        panel.setCanChooseDirectories(options.folders);
        start_in(&panel, options.start_in.as_deref());
        if !options.accept.is_empty() && !options.folders {
            // 点を落として拡張子だけにする (`.db` → `db`)。
            let exts: Vec<_> = options
                .accept
                .iter()
                .map(|e| NSString::from_str(e.trim_start_matches('.')))
                .collect();
            let refs: Vec<&NSString> = exts.iter().map(|s| &**s).collect();
            #[allow(deprecated)]
            panel.setAllowedFileTypes(Some(&NSArray::from_slice(&refs)));
        }
        // 1 = NSModalResponseOK
        if panel.runModal() != 1 {
            return PickResult::Cancelled;
        }
        let urls = panel.URLs();
        let mut out = Vec::new();
        for url in urls.iter() {
            let Some(path) = url.path() else { continue };
            let path = std::path::PathBuf::from(path.to_string());
            // フォルダと、場所だけ欲しいときは中身を読まない。
            if options.folders || options.paths_only {
                out.push(PickedFile { name: name_of(&path), bytes: Vec::new(), path: Some(path) });
                continue;
            }
            match std::fs::read(&path) {
                Ok(bytes) => out.push(PickedFile { name: name_of(&path), bytes, path: Some(path) }),
                Err(e) => tracing::warn!("選ばれたファイルを読めない: {e}"),
            }
        }
        if out.is_empty() {
            PickResult::Cancelled
        } else {
            PickResult::Picked(out)
        }
    }

    pub fn save(name: &str, bytes: &[u8], options: &SaveOptions) -> SaveResult {
        let Some(mtm) = main_thread() else {
            tracing::warn!("files::save をメインスレッド以外から呼んでいる");
            return SaveResult::Unsupported;
        };
        let panel = NSSavePanel::savePanel(mtm);
        panel.setNameFieldStringValue(&NSString::from_str(name));
        start_in(&panel, options.start_in.as_deref());
        if panel.runModal() != 1 {
            return SaveResult::Cancelled;
        }
        let Some(url) = panel.URL() else { return SaveResult::Cancelled };
        let Some(path) = url.path() else { return SaveResult::Cancelled };
        let path = std::path::PathBuf::from(path.to_string());
        write_to(path, bytes)
    }
}

// ---------------------------------------------------------------------------
// web
// ---------------------------------------------------------------------------

#[cfg(target_arch = "wasm32")]
mod web {
    use super::{deliver, PickOptions, PickResult, PickedFile};
    use wasm_bindgen::prelude::*;
    use wasm_bindgen::JsCast;

    /// 隠した `<input type=file>` を作ってその場で `click()` する。
    ///
    /// **ユーザー操作の余韻 (transient activation) に乗っている。** クリック処理は
    /// ブラウザのイベントの中ではなく次のフレームで走るので、厳密には
    /// 「操作の中」ではない。Chrome は数秒の猶予を持つので実際には開くが、
    /// Safari はより厳しい可能性がある (未確認)。
    pub fn pick(key: String, options: PickOptions) {
        // ブラウザはフォルダの場所を渡さない (`webkitdirectory` は中のファイルを並べるだけ)。
        if options.folders {
            log::warn!("files::pick: web ではフォルダを選べない");
            deliver(key, PickResult::Unsupported);
            return;
        }
        let Some(document) = web_sys::window().and_then(|w| w.document()) else {
            deliver(key, PickResult::Unsupported);
            return;
        };
        let Ok(el) = document.create_element("input") else {
            deliver(key, PickResult::Unsupported);
            return;
        };
        let Ok(input) = el.dyn_into::<web_sys::HtmlInputElement>() else {
            deliver(key, PickResult::Unsupported);
            return;
        };
        input.set_type("file");
        input.set_multiple(options.multiple);
        if !options.accept.is_empty() {
            input.set_accept(&options.accept.join(","));
        }
        let _ = input.style().set_property("display", "none");

        let input_for_cb = input.clone();
        let cb = Closure::<dyn FnMut(web_sys::Event)>::new(move |_: web_sys::Event| {
            let key = key.clone();
            let Some(list) = input_for_cb.files() else {
                deliver(key, PickResult::Cancelled);
                return;
            };
            if list.length() == 0 {
                deliver(key, PickResult::Cancelled);
                return;
            }
            let files: Vec<web_sys::File> = (0..list.length()).filter_map(|i| list.get(i)).collect();
            // `File::array_buffer()` は Promise なので、読み終わってから届ける。
            wasm_bindgen_futures::spawn_local(async move {
                let mut out = Vec::new();
                for f in files {
                    let name = f.name();
                    match wasm_bindgen_futures::JsFuture::from(f.array_buffer()).await {
                        Ok(buf) => {
                            let bytes = js_sys::Uint8Array::new(&buf).to_vec();
                            out.push(PickedFile::new(name, bytes));
                        }
                        Err(e) => log::warn!("ファイルを読めない: {e:?}"),
                    }
                }
                if out.is_empty() {
                    deliver(key, PickResult::Cancelled);
                } else {
                    deliver(key, PickResult::Picked(out));
                }
            });
        });
        let _ = input.add_event_listener_with_callback("change", cb.as_ref().unchecked_ref());
        // input は change まで生かす必要がある。閉じ込めごと leak する
        // (ダイアログ 1 回ぶんの小さな要素)。
        cb.forget();
        input.click();
    }

    pub fn save(name: &str, bytes: &[u8]) -> bool {
        let Some(document) = web_sys::window().and_then(|w| w.document()) else {
            return false;
        };
        let array = js_sys::Uint8Array::from(bytes);
        let parts = js_sys::Array::new();
        parts.push(&array.buffer());
        let Ok(blob) = web_sys::Blob::new_with_u8_array_sequence(&parts) else {
            return false;
        };
        let Ok(url) = web_sys::Url::create_object_url_with_blob(&blob) else {
            return false;
        };
        let Ok(el) = document.create_element("a") else { return false };
        let Ok(anchor) = el.dyn_into::<web_sys::HtmlAnchorElement>() else {
            return false;
        };
        anchor.set_href(&url);
        anchor.set_download(name);
        anchor.click();
        // Blob の URL は放っておくとページが死ぬまで残る。
        let _ = web_sys::Url::revoke_object_url(&url);
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn options_read_like_the_call_site() {
        let o = PickOptions::new().accept([".db", ".zip"]).multiple();
        assert_eq!(o.accept, vec![".db".to_string(), ".zip".to_string()]);
        assert!(o.multiple);
        let o = PickOptions::new().folders().start_in("/tmp/projects");
        assert!(o.folders);
        assert!(PickOptions::new().paths_only().paths_only);
        assert!(!PickOptions::new().paths_only, "既定は今までどおり中身を読む");
        assert_eq!(o.start_in.as_deref(), Some(Path::new("/tmp/projects")));
        assert_eq!(SaveOptions::new().start_in("/tmp").start_in.as_deref(), Some(Path::new("/tmp")));
    }

    #[test]
    fn a_picked_file_can_carry_where_it_came_from() {
        let f = PickedFile::new("見積.csv", b"a,b".to_vec());
        assert_eq!(f.path, None, "web と同じく、場所は無しから");
        let f = f.with_path("/Users/me/見積.csv");
        assert_eq!(f.path.as_deref(), Some(Path::new("/Users/me/見積.csv")));
    }

    /// 書けなかったら、閉じたのとは別に理由付きで返る (「保存できませんでした」と出せる)。
    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn a_write_that_fails_is_not_reported_as_cancelled() {
        let dir = std::env::temp_dir().join(format!("sabitori-files-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let ok = dir.join("out.csv");
        assert_eq!(write_to(ok.clone(), b"a,b"), SaveResult::Saved(Some(ok.clone())));
        assert_eq!(std::fs::read(&ok).unwrap(), b"a,b");
        // フォルダの無い場所には書けない。
        let bad = dir.join("no-such-dir").join("out.csv");
        assert!(matches!(write_to(bad, b"a,b"), SaveResult::Failed(reason) if !reason.is_empty()));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    /// 差し込んだ結果が 1 回だけ汲まれること (2 回汲めると二重復元になる)。
    #[test]
    fn a_delivered_result_is_drained_once() {
        test_deliver("restore", PickResult::Cancelled);
        assert_eq!(take_results().len(), 1);
        assert!(take_results().is_empty());
    }
}
