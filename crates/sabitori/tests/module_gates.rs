//! **モジュールの `cfg` が、意図したモジュールに掛かっていること。**
//!
//! `lib.rs` の宣言は
//!
//! ```ignore
//! #[cfg(target_os = "macos")]
//! pub mod macos_drag;
//! ```
//!
//! という並びなので、**あいだに別の宣言を差し込むと cfg が移る**。実際
//! v0.13.0 の準備中に `mod a11y;` を `macos_drag` の直前に入れて、
//!
//! - `a11y` が macOS だけになり (native 全部のつもりだった)
//! - `macos_drag` が無条件になった
//!
//! 手元 (macOS) では両方通るので気づけず、**Linux の CI で初めて落ちた**
//! (`cannot find 'a11y' in 'crate'` が 8 件)。ここはその形を、押した瞬間に
//! 手元で見つけるためのもの。
//!
//! cfg を**意図的に**変えたときは、この表も直すこと。表を直さずに通らない、
//! が正しい動き。

/// `lib.rs` の各 `mod` 宣言に、直前に積まれた `#[cfg(..)]` を対応づける。
fn module_gates() -> Vec<(String, Vec<String>)> {
    let src = include_str!("../src/lib.rs");
    let mut out = Vec::new();
    let mut pending: Vec<String> = Vec::new();
    for line in src.lines() {
        let line = line.trim();
        if let Some(cfg) = line.strip_prefix("#[cfg(").and_then(|r| r.strip_suffix(")]")) {
            pending.push(cfg.replace(' ', ""));
            continue;
        }
        // 文書コメントと属性は cfg を持ち越す (あいだに挟まっても同じ宣言に掛かる)。
        if line.starts_with("///") || line.starts_with("//") || line.is_empty() {
            continue;
        }
        let decl = line.strip_prefix("pub ").unwrap_or(line);
        if let Some(rest) = decl.strip_prefix("mod ") {
            if let Some(name) = rest.strip_suffix(';') {
                out.push((name.to_string(), std::mem::take(&mut pending)));
                continue;
            }
        }
        pending.clear();
    }
    out
}

fn gates_of(name: &str) -> Vec<String> {
    module_gates()
        .into_iter()
        .find(|(m, _)| m == name)
        .unwrap_or_else(|| panic!("{name} モジュールの宣言が見つからない"))
        .1
}

/// 支援技術・画面外描画は **native 全部** (macOS だけではない)。
#[test]
fn native_only_modules_are_gated_on_not_wasm() {
    for m in ["a11y", "offscreen"] {
        assert_eq!(
            gates_of(m),
            vec!["not(target_arch=\"wasm32\")".to_string()],
            "{m} の cfg が native 全部になっていない"
        );
    }
}

/// プラットフォーム固有のモジュールが、そのプラットフォームに掛かっていること。
#[test]
fn platform_modules_keep_their_own_gate() {
    for (m, expected) in [
        ("macos_drag", "target_os=\"macos\""),
        ("macos_blur", "target_os=\"macos\""),
        ("ios_keyboard", "target_os=\"ios\""),
        ("web_ime", "target_arch=\"wasm32\""),
        ("web_history", "target_arch=\"wasm32\""),
        ("web_wake", "target_arch=\"wasm32\""),
    ] {
        assert_eq!(gates_of(m), vec![expected.to_string()], "{m} の cfg がずれている");
    }
}

/// どこでも使えるモジュールに cfg が付いていないこと。
#[test]
fn portable_modules_have_no_gate() {
    for m in ["fonts", "tasks", "files", "testing", "clipboard"] {
        assert!(gates_of(m).is_empty(), "{m} に cfg が付いている: {:?}", gates_of(m));
    }
}
