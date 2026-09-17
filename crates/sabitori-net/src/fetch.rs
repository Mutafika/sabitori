//! HTTP GET → bytes. Platform-split: reqwest on native, `fetch` on WASM.

/// Local-API bearer token. ホスト app (例: bisquit-reader) が起動時に
/// `SABITORI_LOCAL_BEARER` 環境変数をセットすると、`127.0.0.1` /
/// `localhost` 宛のリクエストに `Authorization: Bearer <token>` を
/// 自動付与する. それ以外のホスト (外部 og:image 取得など) には付けない.
/// 値が空 / 環境変数が無ければ無認証.
fn local_host(url: &str) -> bool {
    url.starts_with("http://127.0.0.1")
        || url.starts_with("http://localhost")
        || url.starts_with("http://[::1]")
}

/// GET してバイト列を返す。2xx でなければ人が読めるエラー文字列。
///
/// **[`crate::http`] の上に載っている。** 以前は native (reqwest) と wasm
/// (web_sys) の 2 本立てを**この関数が丸ごと 2 回**書いていた。1 本に寄せて
/// あるので、Cookie・ヘッダ・時間切れの扱いが GET だけ別物になることは無い
/// ([#63](https://github.com/Mutafika/sabitori/issues/63))。
pub async fn fetch_bytes(url: &str) -> Result<Vec<u8>, String> {
    let mut req = crate::http::get(url);

    // ローカル API 宛だけ bearer を足す (この関数だけの約束。上の doc を参照)。
    #[cfg(not(target_arch = "wasm32"))]
    if local_host(url) {
        if let Ok(t) = std::env::var("SABITORI_LOCAL_BEARER") {
            if !t.is_empty() {
                req = req.header("authorization", format!("Bearer {t}"));
            }
        }
    }

    let resp = req.send().await.map_err(|e| e.to_string())?;
    if !resp.ok() {
        return Err(format!("http {} for {}", resp.status(), url));
    }
    Ok(resp.bytes().to_vec())
}
