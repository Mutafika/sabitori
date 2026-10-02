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
//!
//! # 宣言だけで済ませる ([`FontAsset`])
//!
//! 上の手書きの fetch は、 たいていのアプリでは要らない。
//! [`DeclarativeApp::font_assets`](crate::DeclarativeApp::font_assets) に
//! [`font_asset!`](crate::font_asset) を並べれば、 **native はビルド時に埋め込み、
//! wasm は起動時に取ってきてここへ積む**。 大きな日本語フォントを wasm に焼き込まず
//! に済むので、 wasm の大きさとフォントの重さを切り離せる。

use std::sync::Mutex;
#[cfg(target_arch = "wasm32")]
use std::sync::atomic::{AtomicBool, Ordering};

/// 起動時に読み込むフォント 1 本。 [`font_asset!`](crate::font_asset) で作る。
///
/// - **native**: ビルド時に `include_bytes!` で埋め込まれ、 最初のフレームから効く。
/// - **wasm**: 中身を持たず、 起動時に `url` を fetch して [`add`] する。 届くまでは
///   組み込みフォント (`builtin-font-latin` など) か [`fonts()`](crate::DeclarativeApp::fonts)
///   で描く。 wasm の大きさにフォントの分が乗らない。
#[derive(Clone, Copy, Debug)]
pub struct FontAsset {
    url: &'static str,
    #[cfg(not(target_arch = "wasm32"))]
    bytes: &'static [u8],
}

impl FontAsset {
    #[doc(hidden)]
    #[cfg(not(target_arch = "wasm32"))]
    pub const fn __embedded(url: &'static str, bytes: &'static [u8]) -> Self {
        Self { url, bytes }
    }

    #[doc(hidden)]
    #[cfg(target_arch = "wasm32")]
    pub const fn __remote(url: &'static str) -> Self {
        Self { url }
    }

    /// wasm で取りに行く URL (ページからの相対)。
    pub fn url(&self) -> &'static str {
        self.url
    }
}

/// 起動時に読み込むフォントを宣言する。
///
/// `path` はクレート直下からの相対パス。 native ではそのファイルを埋め込み、
/// wasm では同じ文字列を URL として取りに行く。 URL を変えたいときは 2 つ目に渡す。
///
/// ```ignore
/// fn font_assets(&self) -> Vec<sabitori::fonts::FontAsset> {
///     vec![
///         sabitori::font_asset!("assets/fonts/NotoSansJP-Regular.otf"),
///         sabitori::font_asset!("assets/fonts/Hack-Bold.ttf", "fonts/Hack-Bold.ttf"),
///     ]
/// }
/// ```
///
/// trunk なら、 配信側にも同じパスで置く:
/// `<link data-trunk rel="copy-dir" href="assets/fonts" data-target-path="assets/fonts" />`
#[macro_export]
macro_rules! font_asset {
    ($path:literal) => {
        $crate::font_asset!($path, $path)
    };
    ($path:literal, $url:literal) => {{
        #[cfg(not(target_arch = "wasm32"))]
        let asset = $crate::fonts::FontAsset::__embedded(
            $url,
            include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/", $path)),
        );
        #[cfg(target_arch = "wasm32")]
        let asset = $crate::fonts::FontAsset::__remote($url);
        asset
    }};
}

/// 起動時に組版へ渡すフォント ([`DeclarativeApp::fonts`] → native の
/// [`FontAsset`] の順)。 wasm では [`FontAsset`] の fetch もここで始める。
///
/// 起動経路 (declarative / scene / テストの組版) はすべてここを通る。
///
/// [`DeclarativeApp::fonts`]: crate::DeclarativeApp::fonts
pub(crate) fn startup_fonts<A: crate::DeclarativeApp + ?Sized>(app: &A) -> Vec<Vec<u8>> {
    #[cfg_attr(target_arch = "wasm32", allow(unused_mut))]
    let mut fonts = app.fonts();
    #[cfg(not(target_arch = "wasm32"))]
    fonts.extend(app.font_assets().iter().map(|a| a.bytes.to_vec()));
    #[cfg(target_arch = "wasm32")]
    {
        let assets = app.font_assets();
        #[cfg(feature = "builtin-font-jp")]
        warn_if_builtin_jp_is_dead_weight(&fonts, !assets.is_empty());
        fetch_assets(assets);
    }
    fonts
}

/// 組み込みの HackGen (10.2MB) が**使われないのに wasm に載っている**なら、その理由。
///
/// 既定の `builtin-font-jp` は「`fonts()` を書かなくても日本語が出る」ためのもの。
/// アプリが自分で日本語を用意しているなら、 誰も使わない 10MB が初回表示を遅らせる
/// だけになる。 既定を変えずに、 この事故だけを拾う。
#[cfg_attr(not(all(target_arch = "wasm32", feature = "builtin-font-jp")), allow(dead_code))]
pub(crate) fn builtin_jp_dead_weight_reason(
    fonts: &[Vec<u8>],
    declares_assets: bool,
) -> Option<&'static str> {
    if declares_assets {
        return Some("font_assets() で取りに行く宣言がある");
    }
    if fonts.is_empty() {
        return None;
    }
    let mut shaper = sabitori_text::TextShaper::with_fonts_only("ja", fonts);
    let covers_jp = shaper
        .missing_glyphs("あア漢字", sabitori_core::build::TextShape::new(14.0))
        .is_empty();
    covers_jp.then_some("fonts() だけで日本語が組める")
}

#[cfg(all(target_arch = "wasm32", feature = "builtin-font-jp"))]
fn warn_if_builtin_jp_is_dead_weight(fonts: &[Vec<u8>], declares_assets: bool) {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        if let Some(why) = builtin_jp_dead_weight_reason(fonts, declares_assets) {
            log::warn!(
                "sabitori: 組み込みの HackGen (10.2MB) が使われないまま wasm に載っている ({why})。\n\
                 Cargo.toml で `sabitori = {{ ..., default-features = false, features = [\"builtin-font-latin\"] }}` にすると外れる。"
            );
        }
    });
}

/// 取りに行くのは 1 プロセスで 1 回 (renderer を作り直しても 2 度 fetch しない)。
#[cfg(target_arch = "wasm32")]
static ASSETS_STARTED: AtomicBool = AtomicBool::new(false);

/// 並列に取ってきて、 **宣言順に** [`add`] する (優先順 = 積んだ順なので)。
/// 先頭から揃った分だけ積むので、 小さい太字が大きい日本語を待たされることはあっても、
/// 順序が入れ替わることはない。落ちたものは飛ばす (組み込みで描き続けられる)。
#[cfg(target_arch = "wasm32")]
fn fetch_assets(assets: Vec<FontAsset>) {
    use std::cell::RefCell;
    use std::rc::Rc;

    if assets.is_empty() || ASSETS_STARTED.swap(true, Ordering::SeqCst) {
        return;
    }
    /// None = まだ届いていない。Some(None) = 落ちた。Some(Some(bytes)) = 届いた。
    type Slot = Option<Option<Vec<u8>>>;
    let slots: Rc<RefCell<Vec<Slot>>> = Rc::new(RefCell::new(vec![None; assets.len()]));
    let flushed = Rc::new(RefCell::new(0usize));
    for (i, asset) in assets.into_iter().enumerate() {
        let (slots, flushed) = (Rc::clone(&slots), Rc::clone(&flushed));
        wasm_bindgen_futures::spawn_local(async move {
            let got = match sabitori_net::http::get(asset.url).send().await {
                Ok(res) if res.ok() => Some(res.bytes().to_vec()),
                Ok(res) => {
                    log::warn!("フォントを取れなかった: {} ({})", asset.url, res.status());
                    None
                }
                Err(e) => {
                    log::warn!("フォントを取れなかった: {} ({e})", asset.url);
                    None
                }
            };
            let mut slots = slots.borrow_mut();
            slots[i] = Some(got);
            let mut next = flushed.borrow_mut();
            while let Some(Some(done)) = slots.get_mut(*next) {
                if let Some(bytes) = done.take() {
                    add(bytes);
                }
                *next += 1;
            }
        });
    }
}

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

    const LATIN_PATH: &str = "../sabitori-text/assets/Hack-Regular.ttf";
    const LATIN_BYTES: &[u8] = include_bytes!("../../sabitori-text/assets/Hack-Regular.ttf");

    /// **native では宣言したファイルがそのまま埋め込まれる。** URL は既定でパスと同じ。
    #[test]
    fn font_asset_embeds_the_file_on_native() {
        let asset = crate::font_asset!("../sabitori-text/assets/Hack-Regular.ttf");
        assert_eq!(asset.url(), LATIN_PATH);
        assert_eq!(asset.bytes, LATIN_BYTES);
    }

    /// 配信側の置き場所が違うときは URL だけ差し替えられる (埋め込む中身は同じ)。
    #[test]
    fn font_asset_url_can_differ_from_the_path() {
        let asset = crate::font_asset!("../sabitori-text/assets/Hack-Regular.ttf", "fonts/Hack.ttf");
        assert_eq!(asset.url(), "fonts/Hack.ttf");
        assert_eq!(asset.bytes, LATIN_BYTES);
    }

    struct Declared;
    impl crate::DeclarativeApp for Declared {
        fn view(&self, _ctx: &crate::ViewContext) -> sabitori_core::Element {
            sabitori_core::div()
        }
        fn fonts(&self) -> Vec<Vec<u8>> {
            vec![vec![1, 2, 3]]
        }
        fn font_assets(&self) -> Vec<FontAsset> {
            vec![crate::font_asset!("../sabitori-text/assets/Hack-Regular.ttf")]
        }
    }

    /// **`fonts()` が先、 宣言したものが後ろ。** 挿入順が優先順なので、
    /// 入れ替わると手で渡した face が負ける。
    #[test]
    fn startup_fonts_put_declared_assets_after_fonts() {
        let fonts = startup_fonts(&Declared);
        assert_eq!(fonts.len(), 2);
        assert_eq!(fonts[0], vec![1, 2, 3]);
        assert_eq!(fonts[1], LATIN_BYTES);
    }

    /// 宣言だけで、 画面と同じ組版 (テスト用の測り) にも入る。
    #[test]
    fn declared_assets_reach_the_app_shaper() {
        struct JpOnly;
        impl crate::DeclarativeApp for JpOnly {
            fn view(&self, _ctx: &crate::ViewContext) -> sabitori_core::Element {
                sabitori_core::div()
            }
            fn font_assets(&self) -> Vec<FontAsset> {
                vec![crate::font_asset!("../sabitori-text/assets/HackGen-Regular.ttf")]
            }
        }
        let fonts = startup_fonts(&JpOnly);
        let mut shaper = sabitori_text::TextShaper::with_fonts_only("ja", &fonts);
        assert!(
            shaper
                .missing_glyphs("予約", sabitori_core::build::TextShape::new(14.0))
                .is_empty(),
            "宣言したフォントで日本語が組めていない"
        );
    }

    /// **組み込みの HackGen に頼っているアプリには警告しない。** 既定のまま
    /// 日本語を出しているのは正しい使い方。
    #[test]
    fn builtin_jp_is_not_dead_weight_when_the_app_relies_on_it() {
        assert_eq!(builtin_jp_dead_weight_reason(&[], false), None);
        // 英字だけ足しているアプリは、日本語を組み込みに任せている。
        assert_eq!(builtin_jp_dead_weight_reason(&[LATIN_BYTES.to_vec()], false), None);
    }

    /// 自前で日本語を渡している / 取りに行くと宣言しているなら、組み込みは無駄。
    #[test]
    fn builtin_jp_is_dead_weight_when_the_app_brings_its_own_japanese() {
        let jp = include_bytes!("../../sabitori-text/assets/HackGen-Regular.ttf").to_vec();
        assert!(builtin_jp_dead_weight_reason(&[jp], false).is_some());
        assert!(builtin_jp_dead_weight_reason(&[], true).is_some());
    }
}
