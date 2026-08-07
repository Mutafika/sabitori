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

/// Fetch bytes at `url`. On success returns the raw body; on non-2xx status
/// or I/O error returns a human-readable string.
#[cfg(not(target_arch = "wasm32"))]
pub async fn fetch_bytes(url: &str) -> Result<Vec<u8>, String> {
    let client = reqwest::Client::builder()
        .user_agent("sabitori-net/0.1")
        .build()
        .map_err(|e| format!("build client: {e}"))?;
    let mut req = client.get(url);
    if local_host(url) {
        if let Ok(t) = std::env::var("SABITORI_LOCAL_BEARER") {
            if !t.is_empty() {
                req = req.bearer_auth(t);
            }
        }
    }
    let resp = req.send().await.map_err(|e| format!("send: {e}"))?;
    if !resp.status().is_success() {
        return Err(format!("http {} for {}", resp.status().as_u16(), url));
    }
    let bytes = resp.bytes().await.map_err(|e| format!("read body: {e}"))?;
    Ok(bytes.to_vec())
}

#[cfg(target_arch = "wasm32")]
pub async fn fetch_bytes(url: &str) -> Result<Vec<u8>, String> {
    use js_sys::Uint8Array;
    use wasm_bindgen::JsCast;
    use wasm_bindgen_futures::JsFuture;
    use web_sys::{Request, RequestInit, RequestMode, Response};

    let opts = RequestInit::new();
    opts.set_method("GET");
    opts.set_mode(RequestMode::Cors);
    let req = Request::new_with_str_and_init(url, &opts)
        .map_err(|e| format!("new request: {e:?}"))?;
    let window = web_sys::window().ok_or_else(|| "no window".to_string())?;
    let resp_value = JsFuture::from(window.fetch_with_request(&req))
        .await
        .map_err(|e| format!("fetch: {e:?}"))?;
    let resp: Response = resp_value.dyn_into().map_err(|_| "not a Response".to_string())?;
    if !resp.ok() {
        return Err(format!("http {} for {}", resp.status(), url));
    }
    let buf = JsFuture::from(
        resp.array_buffer()
            .map_err(|e| format!("array_buffer: {e:?}"))?,
    )
    .await
    .map_err(|e| format!("array_buffer await: {e:?}"))?;
    let u8 = Uint8Array::new(&buf);
    let mut bytes = vec![0u8; u8.length() as usize];
    u8.copy_to(&mut bytes);
    Ok(bytes)
}
