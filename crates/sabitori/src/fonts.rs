//! **実行時にフォントを足す** ([#75](https://github.com/Mutafika/sabitori/issues/75) の 13)。
//!
//! [`DeclarativeApp::fonts`](crate::DeclarativeApp::fonts) は起動時に 1 回しか
//! 呼ばれず、中身はビルド時の `include_bytes!` に限られていた。wasm には
//! システムフォントが無いので、**組み込みに無いもの (太字・絵文字・別の等幅) は
//! 二度と出せない**。アプリは絵文字を図形で描き直し、太字は諦めていた。
//!
//! ここに積んだフォントは、次のフレームでランタイムの組版に入る。積んだ時点で
//! 測り直しも走るので、**すでに画面に出ている文字も新しい face で組み直される**。
//!
//! # 使い方 (wasm で取ってきて足す)
//!
//! ```ignore
//! // 起動直後、太字と絵文字を後から取る
//! self.tasks.spawn(
//!     async { http::get("/fonts/NotoEmoji.ttf").send().await?.bytes().await },
//!     |_app, res| {
//!         if let Ok(bytes) = res {
//!             sabitori::fonts::add(bytes);   // 次のフレームから出る
//!         }
//!     },
//! );
//! ```
//!
//! native では実行時にユーザーが選んだフォント (設定画面) を読み込むのに使える。
//!
//! # 分かっていること
//!
//! - **後に積んだほうが後ろ**。優先順は積んだ順で、組み込みフォントより後ろに
//!   入る。「いま出ている face を差し替える」のではなく「穴を埋める」ための口。
//! - 積むと**測り直しが走る**ので、大きなフォントを毎フレーム積むと重い。
//!   起動時に数本、が想定。
//! - ランタイムがまだ立っていなければ、立つまで**積んだまま待つ** (捨てない)。

use std::sync::Mutex;

/// 積まれたフォント。**捨てずに持つ** — 画面外に描く ([`crate::offscreen`]) ときも
/// 同じ face で組む必要があるので、ランタイムが受け取ったら消える形にはできない。
static FONTS: Mutex<Vec<Vec<u8>>> = Mutex::new(Vec::new());

/// フォント (TTF / OTF のバイト列) を足す。次のフレームから組版に入る。
///
/// 同じバイト列を 2 回積んでも害は無いが、そのぶん測り直しが走る。
pub fn add(data: Vec<u8>) {
    if data.is_empty() {
        return;
    }
    if let Ok(mut fonts) = FONTS.lock() {
        fonts.push(data);
    }
    // 既定の `lazy_render` は入力が無ければ描かない。積んだだけでは
    // 誰も引き取らないので、1 フレーム起こす (web の橋と同じ穴)。
    #[cfg(target_arch = "wasm32")]
    crate::web_wake::wake();
}

/// これまでに積まれた本数。
pub fn count() -> usize {
    FONTS.lock().map(|f| f.len()).unwrap_or(0)
}

/// 積まれたフォントを全部返す。画面外に描くときなど、**ランタイムとは別に
/// 組版を立てる側**が使う。
pub fn all() -> Vec<Vec<u8>> {
    FONTS.lock().map(|f| f.clone()).unwrap_or_default()
}

/// `applied` 本目以降に積まれたぶんを返す (ランタイム用)。
pub(crate) fn since(applied: usize) -> Vec<Vec<u8>> {
    match FONTS.lock() {
        Ok(fonts) if fonts.len() > applied => fonts[applied..].to_vec(),
        _ => Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 積む先はプロセスで 1 つなので、テスト同士が混ざる。**本数ではなく
    /// 中身で確かめる** (数えると、並走した別のテストが足したぶんでずれる)。
    #[test]
    fn empty_data_is_ignored() {
        add(Vec::new());
        assert!(all().iter().all(|f| !f.is_empty()), "空のフォントが積まれている");
    }

    /// **受け取っても消えない。** 消える形にすると、画面外に描くほうが
    /// 組み込みフォントだけで組んでしまい、画面と帳票で字が変わる。
    #[test]
    fn added_fonts_stay_available_to_everyone() {
        let marker = vec![7u8, 7, 7, 7];
        let before = count();
        add(marker.clone());

        // ランタイムが受け取っても、一覧からは消えない。
        let handed = since(before);
        assert!(handed.iter().any(|f| f == &marker), "渡されていない");
        assert!(all().iter().any(|f| f == &marker), "渡したら消えている");

        // 受け取ったところから先は空 (同じものを 2 度渡さない)。
        assert!(since(count()).is_empty());
    }

    /// **積んだバイト列が実際に組版で使える。**
    ///
    /// 「受け皿はあるが、渡したフォントでは 1 文字も出せない」が一番困る形
    /// なので、システムフォントを**使わない**条件で確かめる — wasm と同じ条件
    /// (ブラウザにはシステムフォントが無い)。
    #[test]
    fn a_font_added_at_runtime_can_actually_shape_text() {
        use sabitori_core::build::TextShape;

        const JP_FONT: &[u8] =
            include_bytes!("../../sabitori-text/assets/HackGen-Regular.ttf");

        const LATIN_FONT: &[u8] =
            include_bytes!("../../sabitori-text/assets/Hack-Regular.ttf");

        // Latin だけのフォントでは日本語が描けない (wasm の既定がこの状態)。
        let mut latin_only =
            sabitori_text::TextShaper::with_fonts_only("ja", &[LATIN_FONT.to_vec()]);
        assert!(
            !latin_only.missing_glyphs("予約", TextShape::new(14.0)).is_empty(),
            "Latin だけのフォントで日本語が描けている (テストの前提が壊れている)"
        );

        add(JP_FONT.to_vec());
        let mut with_runtime_font =
            sabitori_text::TextShaper::with_fonts_only("ja", &all());
        assert!(
            with_runtime_font.missing_glyphs("予約", TextShape::new(14.0)).is_empty(),
            "実行時に積んだフォントで描けていない"
        );
    }
}
