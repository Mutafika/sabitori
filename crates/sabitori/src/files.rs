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
//! # プラットフォーム
//!
//! | | 選ぶ | 保存する |
//! |---|---|---|
//! | macOS | `NSOpenPanel` | `NSSavePanel` |
//! | web | `<input type=file>` | Blob + `<a download>` |
//! | Windows / Linux | **未対応** ([`PickResult::Unsupported`]) | **未対応** (`false`) |
//!
//! Windows / Linux は、入れるなら `rfd` を足すことになる。Linux の既定
//! バックエンドが GTK3 で、**使わない利用者にも GTK の開発ヘッダを要求する**
//! ため、依存を足すかは別の判断として残してある。**黙って何も起きない形には
//! していない** — `Unsupported` が返るので、アプリは「この環境では選べません」
//! と出せる。

use std::cell::RefCell;

/// 選ばれたファイル 1 つ。
#[derive(Clone, Debug, PartialEq)]
pub struct PickedFile {
    /// 表示用の名前 (パスではない — web ではパスが取れない)。
    pub name: String,
    /// 中身。
    pub bytes: Vec<u8>,
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
/// macOS は保存先を選ばせ、web はダウンロードさせる。
pub fn save(name: &str, bytes: &[u8]) -> bool {
    #[cfg(all(target_os = "macos", not(target_arch = "wasm32")))]
    {
        macos::save(name, bytes)
    }
    #[cfg(target_arch = "wasm32")]
    {
        web::save(name, bytes)
    }
    #[cfg(not(any(target_os = "macos", target_arch = "wasm32")))]
    {
        let _ = (name, bytes);
        tracing::warn!("files::save はこの環境では未対応 (#77)");
        false
    }
}

// ---------------------------------------------------------------------------
// macOS
// ---------------------------------------------------------------------------

#[cfg(all(target_os = "macos", not(target_arch = "wasm32")))]
mod macos {
    use super::{PickOptions, PickResult, PickedFile};
    use objc2::MainThreadMarker;
    use objc2_app_kit::{NSOpenPanel, NSSavePanel};
    use objc2_foundation::{NSArray, NSString};

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
        panel.setCanChooseFiles(true);
        panel.setCanChooseDirectories(false);
        if !options.accept.is_empty() {
            // 点を落として拡張子だけにする (`.db` → `db`)。
            let exts: Vec<_> = options
                .accept
                .iter()
                .map(|e| NSString::from_str(e.trim_start_matches('.')))
                .collect();
            let refs: Vec<&NSString> = exts.iter().map(|s| &**s).collect();
            #[allow(deprecated)]
            unsafe {
                panel.setAllowedFileTypes(Some(&NSArray::from_slice(&refs)))
            };
        }
        // 1 = NSModalResponseOK
        if unsafe { panel.runModal() } != 1 {
            return PickResult::Cancelled;
        }
        let urls = unsafe { panel.URLs() };
        let mut out = Vec::new();
        for url in urls.iter() {
            let Some(path) = (unsafe { url.path() }) else { continue };
            let path = std::path::PathBuf::from(path.to_string());
            match std::fs::read(&path) {
                Ok(bytes) => out.push(PickedFile {
                    name: path
                        .file_name()
                        .map(|n| n.to_string_lossy().into_owned())
                        .unwrap_or_default(),
                    bytes,
                }),
                Err(e) => tracing::warn!("選ばれたファイルを読めない: {e}"),
            }
        }
        if out.is_empty() {
            PickResult::Cancelled
        } else {
            PickResult::Picked(out)
        }
    }

    pub fn save(name: &str, bytes: &[u8]) -> bool {
        let Some(mtm) = main_thread() else {
            tracing::warn!("files::save をメインスレッド以外から呼んでいる");
            return false;
        };
        let panel = NSSavePanel::savePanel(mtm);
        panel.setNameFieldStringValue(&NSString::from_str(name));
        if unsafe { panel.runModal() } != 1 {
            return false;
        }
        let Some(url) = (unsafe { panel.URL() }) else { return false };
        let Some(path) = (unsafe { url.path() }) else { return false };
        match std::fs::write(path.to_string(), bytes) {
            Ok(()) => true,
            Err(e) => {
                tracing::warn!("保存に失敗: {e}");
                false
            }
        }
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
                            out.push(PickedFile { name, bytes });
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
    }

    /// 差し込んだ結果が 1 回だけ汲まれること (2 回汲めると二重復元になる)。
    #[test]
    fn a_delivered_result_is_drained_once() {
        test_deliver("restore", PickResult::Cancelled);
        assert_eq!(take_results().len(), 1);
        assert!(take_results().is_empty());
    }
}
