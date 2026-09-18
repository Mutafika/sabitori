//! **起動に失敗したことを画面に出す** ([#82])。
//!
//! wasm では「GPU が用意できない」が実際に起きる — WebGL2 も WebGPU も無い、
//! ハードウェアアクセラレーションが切られている、上限が足りない
//! ([#72](https://github.com/Mutafika/sabitori/issues/72))。それまでは
//! `expect` で落ちるだけだったので、**canvas が真っ白なまま**で、console を
//! 開かないと何が起きたのか分からなかった。
//!
//! 出すのは canvas の上に重ねた `<div>` 1 枚。フレームワークの中で DOM を
//! 触るのはここと [`web_ime`](crate::web_ime) / [`web_history`](crate::web_history)
//! だけ。
//!
//! [#82]: https://github.com/Mutafika/sabitori/issues/82

use wasm_bindgen::JsCast;

/// 画面いっぱいに理由を出す。出せなくても黙って諦める (出す先が無いだけ)。
pub fn show(message: &str) {
    log::error!("{message}");
    let Some(document) = web_sys::window().and_then(|w| w.document()) else { return };
    let Some(body) = document.body() else { return };

    // 2 回目以降は差し替える (初期化を何度も試す実装でも積み上がらない)。
    if let Some(existing) = document.get_element_by_id(BOX_ID) {
        existing.set_text_content(Some(message));
        return;
    }

    let Ok(el) = document.create_element("div") else { return };
    el.set_id(BOX_ID);
    el.set_text_content(Some(message));
    if let Some(html) = el.dyn_ref::<web_sys::HtmlElement>() {
        // canvas の上に出す。色は固定 — **アプリのテーマは読めない**
        // (初期化に失敗しているので、アプリはまだ動いていない)。
        let _ = html.style().set_css_text(
            "position:fixed; inset:0; z-index:2147483647; \
             display:flex; align-items:center; justify-content:center; \
             padding:24px; box-sizing:border-box; text-align:center; \
             background:#1e1e2e; color:#e8e8f0; \
             font-family:system-ui, sans-serif; font-size:15px; line-height:1.7; \
             white-space:pre-wrap;",
        );
    }
    let _ = body.append_child(&el);
}

const BOX_ID: &str = "sabitori-error";
