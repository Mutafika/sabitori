//! **web の打鍵の仕分け。** DOM を触らない部分だけを切り出してある。
//!
//! 中身は [`web_ime`](crate::web_ime) のものだが、あちらは wasm でしかコンパイル
//! されない = **CI のテストが 1 行も走らない**。判断を間違えると
//! 「web で英数字が 1 文字も入らない」([#81]) のような、native のテストでは
//! 絶対に見えない壊れ方になるので、判断そのものはここに置いて native から試す。
//!
//! [#81]: https://github.com/Mutafika/sabitori/issues/81

// 呼ぶのは wasm の `web_ime` だけ。native ではテストからしか呼ばれない。
#![cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]

/// 打鍵が「文字を入れるキー」か。
///
/// DOM の `KeyboardEvent.key` は、印字可能なキーなら**その文字そのもの**
/// (`"a"` `"A"` `"@"` `" "` `"あ"`)、そうでなければ名前 (`"Enter"`
/// `"ArrowLeft"` `"Backspace"`) になる。この区別で、
/// 「ブラウザに入れさせる打鍵」と「こちらで処理する打鍵」を分ける。
///
/// 文字を入れるキーを**止めてはいけない** — 止めるとブラウザが隠し textarea に
/// 文字を入れず `input` も出さないので、打った文字がどこにも届かない。
/// `InputEvent` で文字を運ぶのは `ImeCommit` / `Paste` だけで、`KeyInput` は
/// 文字を持っていない。
pub(crate) fn is_printable(key: &str) -> bool {
    let mut chars = key.chars();
    chars.next().is_some() && chars.next().is_none()
}

/// 焦点が canvas にあるときの ⌘C / Ctrl+C を、隠し textarea へ回すか (#111)。
/// 回すなら、その textarea に入れて全選択する文字列を返す。
///
/// 画面の文字を選んだとき、焦点は canvas にある。canvas に届いた `keydown` は
/// winit が既定動作を止めるので、ブラウザの `copy` 自体が起きない。そこで窓の
/// 捕獲段階で先に拾い、textarea に焦点を移して選択文字列を入れる。そうすれば
/// ブラウザの `copy` が textarea で起き、既存の `write_clipboard` が書く。
///
/// - 欄 (textarea) に焦点があるなら何もしない — 欄の中の ⌘C は今までの道
/// - 選択が無い・空なら何もしない — クリップボードを空で上書きしない
/// - ⌘X は回さない — 画面の選択は読み取り専用 (native と同じ)
pub(crate) fn copy_to_route(
    key: &str,
    mods: CopyMods,
    focus: CopyFocus,
    selection: Option<&(String, bool)>,
) -> Option<String> {
    // コピーの打鍵だけ。⌘⇧C / Ctrl+Shift+C は Chrome の「要素を調べる」、
    // Ctrl+Alt+C は別の命令、mac の Ctrl+C はコピーではない — どれも `copy` が
    // 起きないので、回すと焦点だけが textarea に残る。
    let copy_key = if mods.mac { mods.meta && !mods.ctrl } else { mods.ctrl && !mods.meta };
    if !copy_key || mods.shift || mods.alt || !key.eq_ignore_ascii_case("c") {
        return None;
    }
    // 焦点が canvas (か、どこにも無い = body) のときだけ。欄 (textarea) の中は
    // 今までの道。ページに置かれた別の入力欄の ⌘C を横取りしない。
    if focus != CopyFocus::Canvas {
        return None;
    }
    selection.map(|(t, _)| t.clone()).filter(|t| !t.is_empty())
}

/// [`copy_to_route`] が見る修飾キー。`mac` = ⌘ がコピーの修飾キーの環境。
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct CopyMods {
    pub mac: bool,
    pub meta: bool,
    pub ctrl: bool,
    pub shift: bool,
    pub alt: bool,
}

/// ⌘C を押した時点の焦点。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum CopyFocus {
    /// canvas、またはどこにも無い (body)。
    Canvas,
    /// 隠し textarea (= 登録済みのテキスト欄) かページの別の要素。
    Elsewhere,
}

#[cfg(test)]
mod tests {
    use super::{copy_to_route, is_printable};

    /// 画面の選択の、コピーの打鍵だけを回す (#111)。
    #[test]
    fn only_a_copy_of_the_screen_selection_is_routed() {
        use super::{CopyFocus::*, CopyMods};
        let sel = ("C00001000".to_string(), false);
        let mac = |f: fn(&mut CopyMods)| {
            let mut m = CopyMods { mac: true, meta: true, ..Default::default() };
            f(&mut m);
            m
        };
        let pc = CopyMods { ctrl: true, ..Default::default() };
        let route = |k: &str, m: CopyMods, f| copy_to_route(k, m, f, Some(&sel));
        assert_eq!(route("c", mac(|_| {}), Canvas).as_deref(), Some("C00001000"), "mac の ⌘C");
        assert_eq!(route("c", pc, Canvas).as_deref(), Some("C00001000"), "Windows / Linux の Ctrl+C");
        assert_eq!(route("C", mac(|m| m.shift = true), Canvas), None, "⌘⇧C は要素を調べる");
        assert_eq!(route("c", CopyMods { shift: true, ..pc }, Canvas), None, "Ctrl+Shift+C も");
        assert_eq!(route("c", CopyMods { alt: true, ..pc }, Canvas), None);
        assert_eq!(route("c", mac(|m| { m.meta = false; m.ctrl = true }), Canvas), None, "mac の Ctrl+C はコピーではない");
        assert_eq!(route("c", CopyMods { meta: true, ..Default::default() }, Canvas), None, "mac 以外の ⌘");
        assert_eq!(route("c", mac(|_| {}), Elsewhere), None, "欄やページの入力欄の中は今までの道");
        assert_eq!(route("x", mac(|_| {}), Canvas), None, "⌘X は回さない");
        assert_eq!(route("c", CopyMods { mac: true, ..Default::default() }, Canvas), None, "修飾キー無しは文字");
        assert_eq!(copy_to_route("c", pc, Canvas, None), None, "選択が無ければ触らない");
        let empty = (String::new(), false);
        assert_eq!(copy_to_route("c", pc, Canvas, Some(&empty)), None);
    }

    /// **文字を入れるキーと、編集のキーを取り違えないこと。**
    ///
    /// 取り違えると、印字可能なキーを止めてしまって web で英数字が 1 文字も
    /// 入らない (#81) か、Enter / 矢印をブラウザに渡してページが動く。
    #[test]
    fn printable_keys_are_the_ones_that_carry_a_character() {
        for key in ["a", "A", "z", "0", "9", "@", "_", " ", "あ", "、", "é"] {
            assert!(is_printable(key), "{key:?} は文字を入れるキー");
        }
        for key in [
            "Enter", "Tab", "Backspace", "Delete", "Escape", "ArrowLeft", "ArrowRight",
            "ArrowUp", "ArrowDown", "Home", "End", "PageUp", "PageDown", "Shift",
            "Control", "Meta", "Alt", "CapsLock", "F1", "Dead", "Process", "Unidentified",
        ] {
            assert!(!is_printable(key), "{key:?} は文字を入れるキーではない");
        }
    }

    /// 空は文字ではない (古いブラウザで `key` が空になることがある)。
    #[test]
    fn an_empty_key_is_not_printable() {
        assert!(!is_printable(""));
    }
}
