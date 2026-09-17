//! Web の文字入力の橋渡し (隠し `<textarea>`)。
//!
//! wasm のランタイムは canvas 1 枚で、DOM の入力要素を持たない。そのため
//! **winit の web 実装が拾える範囲しか届かない**。実際に headless Chrome で
//! 確かめた結果 ([#73](https://github.com/Mutafika/sabitori/issues/73)):
//!
//! | 操作 | 届くか |
//! |---|---|
//! | 半角英字の打鍵 | 届く (winit の keydown) |
//! | IME の変換 (`compositionupdate` / `compositionend`) | **届かない** |
//! | `insertText` (ソフトキーボード・音声入力・貼り付けの一部) | **届かない** |
//!
//! winit 0.30 の web 実装は `set_ime_allowed` / `set_ime_cursor_area` が何も
//! しない。そして iPadOS / Android は**入力要素にフォーカスしないとソフト
//! キーボードを出さない**ので、タブレットではテキスト欄に 1 文字も打てない。
//! 氏名・住所・備考を打つ業務アプリでは致命的。
//!
//! ここは iOS の [`crate::ios_keyboard`] と同じ形 — 見えない本物の入力要素に
//! 実際のテキスト入力セッションを張らせ、そこで起きたことを [`InputEvent`] に
//! 直してランタイムが毎フレーム汲む。違いは、web では変換中の文字列
//! (`compositionupdate`) がそのまま取れるので preedit を作れること。
//!
//! # 焦点は「ユーザー操作の中で」当てる
//!
//! iOS Safari は、**ユーザー操作のイベントハンドラの中で** `focus()` しないと
//! ソフトキーボードを出さない。ランタイムの次の tick からでは遅い。なので
//! canvas の `pointerup` を直接拾い、押した点がテキスト欄の矩形の上なら
//! **そのイベントの中で** `textarea.focus()` する。矩形は毎フレーム
//! [`set_fields`] で渡しておく。
//!
//! # なぜランタイム側に置くか
//!
//! アプリ側でも同じものは書ける (この実装は実際に業務アプリで動いていた
//! ものを移したもの) が、その場合「今の画面のテキスト欄を全部列挙する関数」を
//! 手で保守することになる。ランタイムは `register_managed` で欄を知っている
//! ので、列挙が要らない。

#![cfg(target_arch = "wasm32")]

use std::cell::RefCell;

use sabitori_core::Rect;
use sabitori_input::{InputEvent, Key, Modifiers};
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;

thread_local! {
    /// DOM のイベントから作った入力。ランタイムが毎フレーム [`drain`] する。
    static QUEUE: RefCell<Vec<InputEvent>> = const { RefCell::new(Vec::new()) };
    /// 張り付けた橋渡し (初回の [`ensure_attached`] で 1 つだけ作る)。
    static BRIDGE: RefCell<Option<Bridge>> = const { RefCell::new(None) };
    /// 今のフレームの、管理テキスト欄の画面矩形 (論理 px)。
    static FIELDS: RefCell<Vec<Rect>> = const { RefCell::new(Vec::new()) };
    /// 今の選択文字列と、それを切り取れるか。ランタイムが毎フレーム更新する。
    ///
    /// `copy` / `cut` の DOM イベントは**同期で**答えないといけないので、
    /// Rust 側へ問い合わせる余裕が無い。前フレームの値を置いておく。
    /// ⌘A の直後 1 フレーム (16ms) だけ古い値になるが、人の指では作れない間隔。
    static SELECTION: RefCell<Option<(String, bool)>> = const { RefCell::new(None) };
    /// `cut` イベントでクリップボードに**書けた**という合図。ランタイムが
    /// 汲んで、そこで初めて本文を消す (「書けてから消す」issue #33)。
    static CUT_WRITTEN: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

struct Bridge {
    textarea: web_sys::HtmlTextAreaElement,
    /// canvas の `pointerup` を張れたか。canvas はランタイムの初期化中に
    /// body へ足されるので、最初の呼び出しではまだ居ないことがある。
    /// 張れるまで毎フレーム試す — ここを 1 回で諦めると、**押しても
    /// キーボードが出ない**状態が一生続く。
    canvas_hooked: bool,
    /// 張ったままにするためだけに持つ。drop すると listener が外れる。
    _keep: Vec<Closure<dyn FnMut(web_sys::Event)>>,
}

fn push(event: InputEvent) {
    QUEUE.with(|q| q.borrow_mut().push(event));
    // **積んだら起こす。** 汲むのは描画フレームの中で、そのフレームは
    // `lazy_render` のせいで「何か起きた」ときしか回らない。ソフトキーボードで
    // 打っているあいだ canvas には winit のイベントが 1 つも来ないので、
    // ここで起こさないと次に画面を触るまで文字が出ない (crate::web_wake)。
    crate::web_wake::wake();
}

/// ランタイムが毎フレーム汲む。
pub fn drain() -> Vec<InputEvent> {
    QUEUE.with(|q| std::mem::take(&mut *q.borrow_mut()))
}

/// 今のフレームで文字を受け取れる欄の矩形を渡す (論理 px、canvas 左上が原点)。
///
/// `pointerup` の中で「押した点が欄の上か」を判定するのに使う。Rust 側へ
/// 問い合わせる余裕は無い (同期でないと iOS がキーボードを出さない) ので、
/// 前フレームの矩形を置いておく。
pub fn set_fields(rects: Vec<Rect>) {
    FIELDS.with(|f| *f.borrow_mut() = rects);
}

/// 今の選択文字列を渡す (`copy` / `cut` が同期で読む)。
///
/// `cuttable` が `false` なら画面の視覚選択 = 読み取り専用なので、⌘X は
/// 何もしない (native と同じ規則。[`AppState::clipboard_selection`] を参照)。
pub fn set_selection(selection: Option<(String, bool)>) {
    SELECTION.with(|s| *s.borrow_mut() = selection);
}

/// `cut` でクリップボードへ書けたか。汲むと下りる。
pub fn take_cut_written() -> bool {
    CUT_WRITTEN.with(|c| c.replace(false))
}

/// 隠し `<textarea>` を body に置き、listener を張る。2 回目以降は何もしない。
pub fn ensure_attached() {
    BRIDGE.with(|b| {
        let mut guard = b.borrow_mut();
        if guard.is_none() {
            match build() {
                Some(bridge) => *guard = Some(bridge),
                None => {
                    log::warn!("web_ime: textarea を張れなかった (document が無い?)");
                    return;
                }
            }
        }
        if let Some(bridge) = guard.as_mut() {
            if !bridge.canvas_hooked {
                bridge.canvas_hooked = hook_canvas(&bridge.textarea, &mut bridge._keep);
            }
        }
    });
}

/// 欄に焦点があるかを反映する。`caret` は変換候補を出す位置 (論理 px)。
///
/// 焦点が外れたら `blur()` する — 付けっぱなしだと、ブラウザがページの
/// どこかにキャレットがあると思い続ける。
pub fn set_active(active: bool, caret: Option<(f32, f32, f32, f32)>) {
    BRIDGE.with(|b| {
        let guard = b.borrow();
        let Some(bridge) = guard.as_ref() else { return };
        let el: &web_sys::HtmlElement = bridge.textarea.as_ref();
        if active {
            // 変換候補ウィンドウは**入力要素の位置**に出るので、キャレットへ
            // 動かす。見えない要素だが、位置は候補窓に効く。
            if let Some((x, y, _w, h)) = caret {
                let style = el.style();
                let _ = style.set_property("left", &format!("{x}px"));
                let _ = style.set_property("top", &format!("{y}px"));
                let _ = style.set_property("height", &format!("{}px", h.max(1.0)));
            }
            // **焦点を毎フレーム確かめ直す。**
            //
            // `pointerup` の中で当てるのは iOS のため (ユーザー操作の中で
            // focus しないとキーボードが出ない) だが、それだけに頼ると
            // winit 側が canvas に焦点を戻す順番と競って、**押したのに打てない**
            // 回が出る (実際に headless Chrome で再現した)。欄に焦点がある間は
            // ここで取り返す。デスクトップのブラウザは操作の外でも focus できる
            // ので、これが効く。
            if !is_focused(&bridge.textarea) {
                let _ = el.focus();
            }
        } else if is_focused(&bridge.textarea) {
            let _ = el.blur();
        }
    });
}

fn is_focused(textarea: &web_sys::HtmlTextAreaElement) -> bool {
    web_sys::window()
        .and_then(|w| w.document())
        .and_then(|d| d.active_element())
        .is_some_and(|a| a.is_same_node(Some(textarea.as_ref())))
}

fn build() -> Option<Bridge> {
    let document = web_sys::window()?.document()?;
    let body = document.body()?;

    let textarea: web_sys::HtmlTextAreaElement =
        document.create_element("textarea").ok()?.dyn_into().ok()?;
    textarea.set_id("sabitori-ime");
    // `opacity: 0` で隠す (`display: none` や `visibility: hidden` だと
    // フォーカスできない = キーボードが出ない)。`font-size: 16px` は iOS が
    // それ未満の入力要素にフォーカスするとページを拡大するため。
    // `pointer-events: none` で、canvas のドラッグを邪魔しない。
    textarea
        .set_attribute(
            "style",
            "position: fixed; left: 0; top: 0; width: 1px; height: 1px; \
             opacity: 0; pointer-events: none; border: 0; padding: 0; \
             margin: 0; resize: none; z-index: 2147483647; font-size: 16px;",
        )
        .ok()?;
    // 補完・自動修正はこちらの文字列を勝手に書き換えるので全部切る。
    for (k, v) in [
        ("autocapitalize", "off"),
        ("autocomplete", "off"),
        ("autocorrect", "off"),
        ("spellcheck", "false"),
        ("aria-hidden", "true"),
        ("tabindex", "-1"),
    ] {
        let _ = textarea.set_attribute(k, v);
    }
    body.append_child(&textarea).ok()?;

    let mut keep: Vec<Closure<dyn FnMut(web_sys::Event)>> = Vec::new();
    let target: &web_sys::EventTarget = textarea.as_ref();

    // --- 変換中 ---------------------------------------------------------
    keep.push(listen(target, "compositionupdate", |e| {
        let Some(ce) = e.dyn_ref::<web_sys::CompositionEvent>() else { return };
        let text = ce.data().unwrap_or_default();
        let end = text.len();
        push(InputEvent::ImePreedit { text, cursor: Some((end, end)) });
    }));

    // --- 確定 -----------------------------------------------------------
    keep.push(listen(target, "compositionend", |e| {
        let Some(ce) = e.dyn_ref::<web_sys::CompositionEvent>() else { return };
        // 先に preedit を畳んでから確定を流す。順序が逆だと、確定文字の上に
        // 変換中の文字列が残る。
        push(InputEvent::ImePreedit { text: String::new(), cursor: None });
        let text = ce.data().unwrap_or_default();
        if !text.is_empty() {
            push(InputEvent::ImeCommit { text });
        }
        clear_textarea();
    }));

    // --- 変換を経ない入力 (ソフトキーボード・音声・貼り付け) ------------
    keep.push(listen(target, "input", |e| {
        let Some(ie) = e.dyn_ref::<web_sys::InputEvent>() else { return };
        if ie.is_composing() {
            return; // 変換中は composition 側で見る。
        }
        let data = ie.data().unwrap_or_default();
        match ie.input_type().as_str() {
            "insertText" => {
                for ch in data.chars() {
                    if !ch.is_control() {
                        push(InputEvent::CharInput(ch));
                    }
                }
            }
            // 貼り付けはここに来る (`paste` イベントより後で、確実に本文が入る)。
            "insertFromPaste" | "insertFromDrop" | "insertReplacementText" => {
                let text = if data.is_empty() { textarea_value() } else { data };
                if !text.is_empty() {
                    push(InputEvent::Paste { text });
                }
            }
            "insertLineBreak" | "insertParagraph" => {
                push(key_event(Key::Enter, Modifiers::default()));
            }
            // deleteContentBackward などは keydown 側で拾っているので無視。
            _ => {}
        }
        clear_textarea();
    }));

    // --- クリップボード (#76) -------------------------------------------
    keep.push(listen(target, "copy", |e| {
        write_clipboard(&e);
        clear_textarea();
    }));
    keep.push(listen(target, "cut", |e| {
        crate::web_wake::wake();
        if write_clipboard(&e) {
            // **書けたときだけ**合図を立てる。順序が逆だと、書けなかった環境で
            // 「切り取ったのにどこにも残らない」が復活する (issue #33)。
            CUT_WRITTEN.with(|c| c.set(true));
        }
        clear_textarea();
    }));

    // --- 編集キー -------------------------------------------------------
    keep.push(listen(target, "keydown", |e| {
        let Some(ke) = e.dyn_ref::<web_sys::KeyboardEvent>() else { return };
        // 変換中のキーはブラウザ / IME のもの。keyCode 229 は「IME が処理中」の
        // 古くからの合図で、is_composing が立たない実装でもこれは来る。
        if ke.is_composing() || ke.key_code() == 229 {
            return;
        }
        let mods = Modifiers {
            shift: ke.shift_key(),
            ctrl: ke.ctrl_key(),
            alt: ke.alt_key(),
            meta: ke.meta_key(),
        };
        // **文字を入れるキーはブラウザに任せる** (#81)。
        //
        // ここで `prevent_default()` すると、ブラウザは textarea に文字を
        // 入れず `input` も出さない。流れるのは `KeyInput` だけだが、
        // **`InputEvent` で文字を運ぶのは `ImeCommit` / `Paste` だけ**なので、
        // 打った英数字がどこにも届かない (v0.12.0 の web は、変換を通した
        // 日本語だけ入ってパスワードが 1 文字も打てなかった)。native は
        // `keymap::char_inputs` が打鍵から文字を作って渡しているが、web に
        // それに当たるものは無い — Shift や配列 (JIS の `@`、EU の記号) を
        // 考えると `Key::A` から復元することもできない。
        //
        // ブラウザに入れさせれば `input` (`insertText`) から `CharInput` として
        // 受け取れる。そちらの経路は既にある (ソフトキーボード・音声入力用)。
        //
        // 修飾キー付き (⌘A / Ctrl+Z) は文字ではなく命令なので、今までどおり
        // 止めて `KeyInput` を流す。`KeyboardEvent.key` は印字可能なら
        // **その文字そのもの**、そうでなければ `"Enter"` のような名前になる。
        if !mods.meta && !mods.ctrl && crate::web_keys::is_printable(&ke.key()) {
            return;
        }

        let Some(key) = map_key(&ke.key()) else { return };

        // ⌘V / Ctrl+V は流さない — ブラウザが `paste` → `input` を出すので、
        // そちらで本文ごと受け取る。両方流すと二重に貼られる。
        if matches!(key, Key::V) && (mods.meta || mods.ctrl) {
            return;
        }

        // ⌘C / ⌘X もブラウザのイベントに委ねる (#76)。
        //
        // `navigator.clipboard` は Promise を返すので同期の `write_text` に
        // 載らず、しかも安全なコンテキスト (https / localhost) でしか使えない
        // — 社内 LAN の http で配る業務アプリでは使えない。DOM の `copy` /
        // `cut` なら、イベントの中で同期に読み書きできて、許可ダイアログも
        // 出ない。
        //
        // ただし **`copy` / `cut` は「選択が無いと出ない」**。隠し textarea は
        // 常に空なので、ここで選択文字列を入れて `select()` してから既定動作に
        // 委ねる。そうして初めてイベントが飛ぶ。
        if matches!(key, Key::C | Key::X) && (mods.meta || mods.ctrl) {
            let sel = SELECTION.with(|s| s.borrow().clone());
            let Some((text, cuttable)) = sel else { return };
            if text.is_empty() || (matches!(key, Key::X) && !cuttable) {
                return;
            }
            select_in_textarea(&text);
            return;
        }

        // 既定動作を止める: 矢印でページが動く、Tab で canvas から焦点が
        // 外れる、Backspace で戻る (古いブラウザ) を防ぐ。修飾キー付きの
        // ⌘C / ⌘X はブラウザの copy / cut を出させたいので止めない。
        if !mods.meta && !mods.ctrl {
            e.prevent_default();
        }
        push(key_event(key, mods));
    }));

    Some(Bridge { textarea, canvas_hooked: false, _keep: keep })
}

/// canvas を押したらフォーカスを当てる listener を張る。張れたら `true`。
///
/// **このイベントの中で `focus()` すること。** iOS はユーザー操作の中で
/// focus しないとソフトキーボードを出さない。ランタイムの次の tick からでは
/// 遅い = タブレットで 1 文字も打てない。
fn hook_canvas(
    textarea: &web_sys::HtmlTextAreaElement,
    keep: &mut Vec<Closure<dyn FnMut(web_sys::Event)>>,
) -> bool {
    let Some(canvas) = web_sys::window()
        .and_then(|w| w.document())
        .and_then(|d| d.get_element_by_id("sabitori-canvas"))
    else {
        return false;
    };

    let ta = textarea.clone();
    let c = Closure::<dyn FnMut(web_sys::Event)>::new(move |e: web_sys::Event| {
        let Some(pe) = e.dyn_ref::<web_sys::PointerEvent>() else { return };
        let (x, y) = (pe.offset_x() as f32, pe.offset_y() as f32);
        let over_field = FIELDS.with(|f| {
            f.borrow()
                .iter()
                .any(|r| r.contains(sabitori_core::Point::new(x, y)))
        });
        let el: &web_sys::HtmlElement = ta.as_ref();
        if over_field {
            let _ = el.focus();
        } else {
            let _ = el.blur();
        }
    });
    let ok = canvas
        .add_event_listener_with_callback("pointerup", c.as_ref().unchecked_ref())
        .is_ok();
    keep.push(c);
    ok
}

fn listen(
    target: &web_sys::EventTarget,
    name: &str,
    f: impl FnMut(web_sys::Event) + 'static,
) -> Closure<dyn FnMut(web_sys::Event)> {
    let c = Closure::<dyn FnMut(web_sys::Event)>::new(f);
    let _ = target.add_event_listener_with_callback(name, c.as_ref().unchecked_ref());
    c
}

fn key_event(key: Key, modifiers: Modifiers) -> InputEvent {
    InputEvent::KeyInput { key, pressed: true, modifiers }
}

/// textarea は**常に空**に保つ。溜めると、次の入力の `input` イベントで
/// 前の文字列まで巻き込む。
fn clear_textarea() {
    BRIDGE.with(|b| {
        if let Some(bridge) = b.borrow().as_ref() {
            bridge.textarea.set_value("");
        }
    });
}

/// `copy` / `cut` の中で、今の選択をクリップボードへ書く。書けたら `true`。
fn write_clipboard(e: &web_sys::Event) -> bool {
    let Some(ce) = e.dyn_ref::<web_sys::ClipboardEvent>() else { return false };
    let Some(data) = ce.clipboard_data() else { return false };
    let sel = SELECTION.with(|s| s.borrow().clone());
    let Some((text, _)) = sel else { return false };
    if text.is_empty() {
        return false;
    }
    if data.set_data("text/plain", &text).is_err() {
        return false;
    }
    // 自分で入れたので、ブラウザの既定動作 (textarea の選択をコピー) は止める。
    e.prevent_default();
    true
}

/// textarea に文字列を入れて全選択する。`copy` / `cut` を発火させるため。
fn select_in_textarea(text: &str) {
    BRIDGE.with(|b| {
        if let Some(bridge) = b.borrow().as_ref() {
            bridge.textarea.set_value(text);
            let _ = bridge.textarea.set_selection_range(0, text.len() as u32);
        }
    });
}

fn textarea_value() -> String {
    BRIDGE.with(|b| {
        b.borrow()
            .as_ref()
            .map(|bridge| bridge.textarea.value())
            .unwrap_or_default()
    })
}

/// DOM の `KeyboardEvent.key` → sabitori の [`Key`]。
///
/// 文字キーは `CharInput` (composition / input 側) が持つので、ここでは
/// **編集に効くキーだけ**返す。修飾キー付きの英字は `⌘A` などのために通す。
fn map_key(key: &str) -> Option<Key> {
    Some(match key {
        "Backspace" => Key::Backspace,
        "Delete" => Key::Delete,
        "ArrowLeft" => Key::Left,
        "ArrowRight" => Key::Right,
        "ArrowUp" => Key::Up,
        "ArrowDown" => Key::Down,
        "Home" => Key::Home,
        "End" => Key::End,
        "Enter" | "NumpadEnter" => Key::Enter,
        "Tab" => Key::Tab,
        "Escape" => Key::Escape,
        "PageUp" => Key::PageUp,
        "PageDown" => Key::PageDown,
        "Insert" => Key::Insert,
        other => {
            let mut chars = other.chars();
            let (Some(c), None) = (chars.next(), chars.next()) else {
                return None;
            };
            match c.to_ascii_lowercase() {
                'a' => Key::A,
                'b' => Key::B,
                'c' => Key::C,
                'd' => Key::D,
                'e' => Key::E,
                'f' => Key::F,
                'g' => Key::G,
                'h' => Key::H,
                'i' => Key::I,
                'j' => Key::J,
                'k' => Key::K,
                'l' => Key::L,
                'm' => Key::M,
                'n' => Key::N,
                'o' => Key::O,
                'p' => Key::P,
                'q' => Key::Q,
                'r' => Key::R,
                's' => Key::S,
                't' => Key::T,
                'u' => Key::U,
                'v' => Key::V,
                'w' => Key::W,
                'x' => Key::X,
                'y' => Key::Y,
                'z' => Key::Z,
                _ => return None,
            }
        }
    })
}

