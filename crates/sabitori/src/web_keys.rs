//! **web の打鍵の仕分け。** DOM を触らない部分だけを切り出してある。
//!
//! 中身は [`web_ime`](crate::web_ime) のものだが、あちらは wasm でしかコンパイル
//! されない = **CI のテストが 1 行も走らない**。判断を間違えると
//! 「web で英数字が 1 文字も入らない」([#81]) のような、native のテストでは
//! 絶対に見えない壊れ方になるので、判断そのものはここに置いて native から試す。
//!
//! [#81]: https://github.com/Mutafika/sabitori/issues/81

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

#[cfg(test)]
mod tests {
    use super::is_printable;

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
