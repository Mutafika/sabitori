//! システムクリップボードの読み書き。
//!
//! 0.4.0 より前は、 コピーが **macOS 専用**（`pbcopy` サブプロセス）で、
//! **ペーストはどのプラットフォームにも実装が無かった** (issue #20)。
//! `sabitori-widgets` の `TextInputState` には `Key::V if is_cmd` の受け口だけあり、
//! 「実際のペーストテキストは CharInput か ImeCommit で届く」 というコメントが
//! 付いていたが、 **クリップボードを読むコードが repo 内に存在しなかった**ので
//! それは起こらなかった。
//!
//! ここは `arboard` に寄せて macOS / Windows / Linux を 1 本で扱う。
//! wasm は `navigator.clipboard` が非同期なので別扱いが要る（未対応）。

/// クリップボードのテキストを読む。 空・非テキスト・失敗はすべて `None`。
///
/// 呼ぶたびにハンドルを作る。 ペーストはユーザ操作の頻度なので、 ハンドルを
/// 持ち回してライフタイムを増やすより素直。
pub fn read_text() -> Option<String> {
    #[cfg(test)]
    {
        TEST_CLIPBOARD.with(|c| c.borrow().clone())
    }
    #[cfg(all(not(test), not(target_arch = "wasm32")))]
    {
        let mut cb = arboard::Clipboard::new().ok()?;
        let text = cb.get_text().ok()?;
        if text.is_empty() {
            None
        } else {
            Some(text)
        }
    }
    #[cfg(all(not(test), target_arch = "wasm32"))]
    {
        // navigator.clipboard.readText() は Promise を返すので、 この同期 API には
        // 載らない。 wasm でのペーストは別の形（イベント経由）が要る。
        None
    }
}

/// クリップボードにテキストを書く。 **書けたら `true`**。
///
/// エラーそのものは握りつぶす（コピーできないだけで、 アプリを止める理由には
/// ならない）が、 **成否は返す**。 切り取りが「書いてから消す」を守れるように
/// するため — 消してから書きに行くと、 書けなかったときに切り取った文字列が
/// どこにも残らない (issue #33 の症状そのもの)。 wasm はまだ書けないので
/// 常に `false`。
pub fn write_text(text: &str) -> bool {
    #[cfg(test)]
    {
        if !TEST_WRITABLE.with(|w| w.get()) {
            return false;
        }
        TEST_CLIPBOARD.with(|c| *c.borrow_mut() = Some(text.to_string()));
        true
    }
    #[cfg(all(not(test), not(target_arch = "wasm32")))]
    {
        match arboard::Clipboard::new() {
            Ok(mut cb) => cb.set_text(text.to_string()).is_ok(),
            Err(_) => false,
        }
    }
    #[cfg(all(not(test), target_arch = "wasm32"))]
    {
        let _ = text;
        false
    }
}

/// テスト中の「クリップボード」。 **実クリップボードには触らない。**
///
/// 2 つ理由がある。 `cargo test` を回しただけで開発者のクリップボードが黙って
/// 書き換わるのは事故だし、 CI (ヘッドレス) にはそもそもクリップボードが無い
/// ので、 実物を叩くテストは環境で結果が変わる。
///
/// この差し替えが効くのは **`sabitori` 自身の unit test だけ** (`cfg(test)` は
/// crate 単位)。 `tests/` の integration test からは実物が見えるので、
/// クリップボードの中身を assert するテストは unit test 側に置くこと。
#[cfg(test)]
thread_local! {
    static TEST_CLIPBOARD: std::cell::RefCell<Option<String>> =
        const { std::cell::RefCell::new(None) };
}

/// テスト用: 書き込みが成功するかどうか。 `false` にすると
/// [`write_text`] が「書けなかった」を返す (wasm や、 arboard がハンドルを
/// 開けない環境の再現)。
#[cfg(test)]
thread_local! {
    static TEST_WRITABLE: std::cell::Cell<bool> = const { std::cell::Cell::new(true) };
}

/// テスト用: 「クリップボード」を空にして、 書き込みを成功に戻す。 テストは
/// 同じスレッドで連続して走るので、 前のテストが書いた値と設定が残る。
#[cfg(test)]
pub(crate) fn test_clear() {
    TEST_CLIPBOARD.with(|c| *c.borrow_mut() = None);
    TEST_WRITABLE.with(|w| w.set(true));
}

/// テスト用: 以降の [`write_text`] を失敗させる / 戻す。
#[cfg(test)]
pub(crate) fn test_set_writable(writable: bool) {
    TEST_WRITABLE.with(|w| w.set(writable));
}

/// このキー入力がペーストの要求か。 macOS は Cmd、 他は Ctrl。
///
/// ⇧+Insert（X11 の慣習）は見ていない。 必要なら足すこと。
pub fn is_paste_shortcut(key: sabitori_input::Key, modifiers: sabitori_input::Modifiers) -> bool {
    let primary = if cfg!(target_os = "macos") {
        modifiers.meta
    } else {
        modifiers.ctrl
    };
    key == sabitori_input::Key::V && primary
}

/// このキー入力がコピーの要求か。
pub fn is_copy_shortcut(key: sabitori_input::Key, modifiers: sabitori_input::Modifiers) -> bool {
    let primary = if cfg!(target_os = "macos") {
        modifiers.meta
    } else {
        modifiers.ctrl
    };
    key == sabitori_input::Key::C && primary
}

/// このキー入力が切り取りの要求か。
///
/// ⇧+Delete（Windows の古い慣習）は見ていない。 必要なら足すこと。
pub fn is_cut_shortcut(key: sabitori_input::Key, modifiers: sabitori_input::Modifiers) -> bool {
    let primary = if cfg!(target_os = "macos") {
        modifiers.meta
    } else {
        modifiers.ctrl
    };
    key == sabitori_input::Key::X && primary
}

#[cfg(test)]
mod tests {
    use super::*;
    use sabitori_input::{Key, Modifiers};

    /// 修飾キー無しの V / C はショートカットではない。 打った文字が
    /// ペースト扱いされたら、 テキスト欄に "v" が入らなくなる。
    #[test]
    fn bare_letters_are_not_shortcuts() {
        let none = Modifiers::default();
        assert!(!is_paste_shortcut(Key::V, none));
        assert!(!is_copy_shortcut(Key::C, none));
        assert!(!is_cut_shortcut(Key::X, none));
    }

    /// プラットフォームの主修飾キーと組んだときだけ成立すること。
    #[test]
    fn primary_modifier_makes_them_shortcuts() {
        let primary = if cfg!(target_os = "macos") {
            Modifiers { meta: true, ..Default::default() }
        } else {
            Modifiers { ctrl: true, ..Default::default() }
        };
        assert!(is_paste_shortcut(Key::V, primary));
        assert!(is_copy_shortcut(Key::C, primary));
        assert!(is_cut_shortcut(Key::X, primary));
        // 別のキーは巻き込まない。
        assert!(!is_paste_shortcut(Key::B, primary));
    }

    /// 反対側の修飾キーでは成立しない（macOS で Ctrl+V が効いてしまう等）。
    #[test]
    fn the_other_modifier_does_not_trigger() {
        let other = if cfg!(target_os = "macos") {
            Modifiers { ctrl: true, ..Default::default() }
        } else {
            Modifiers { meta: true, ..Default::default() }
        };
        assert!(!is_paste_shortcut(Key::V, other));
        assert!(!is_cut_shortcut(Key::X, other));
    }
}
