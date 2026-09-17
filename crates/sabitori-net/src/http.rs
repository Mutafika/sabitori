//! 業務 API を叩くための HTTP ([#63](https://github.com/Mutafika/sabitori/issues/63))。
//!
//! これまで [`fetch_bytes`](crate::fetch::fetch_bytes) は **GET でバイト列を
//! 取るだけ**で、メソッド・ヘッダ・ボディ・Cookie・ステータスコードを扱えな
//! かった。結果、REST のクライアントをアプリが自前で持つことになり、
//!
//! - native: `reqwest::blocking` を直接依存に足す
//! - wasm: `web_sys` の fetch を手書きする
//!
//! が**アプリごとに、しかも native と wasm で別々に**書かれていた。
//!
//! ```ignore
//! let res = http::post("/api/auth/verify")
//!     .json(&Credentials { user, pass })?   // feature = "json"
//!     .credentials(true)                    // Cookie セッション
//!     .send()
//!     .await?;
//!
//! if res.status() == 401 {
//!     return Err("ユーザー名かパスワードが違います".into());
//! }
//! let me: Me = res.error_for_status()?.json()?;
//! ```
//!
//! [`Tasks`] と組み合わせると、業務アプリの 1 往復がこれで書ける:
//!
//! ```ignore
//! app.tasks.spawn(async move { http::get("/api/vehicles").send().await }, |app, res| {
//!     app.vehicles = res.and_then(|r| r.json()).unwrap_or_default();
//! });
//! ```
//!
//! # 範囲
//!
//! リトライ・キャッシュ・WebSocket / SSE は入っていない (別の話として残す)。
//!
//! [`Tasks`]: https://docs.rs/sabitori/latest/sabitori/tasks/struct.Tasks.html

use std::time::Duration;

/// HTTP メソッド。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Method {
    Get,
    Post,
    Put,
    Patch,
    Delete,
    Head,
}

impl Method {
    pub fn as_str(self) -> &'static str {
        match self {
            Method::Get => "GET",
            Method::Post => "POST",
            Method::Put => "PUT",
            Method::Patch => "PATCH",
            Method::Delete => "DELETE",
            Method::Head => "HEAD",
        }
    }
}

/// 失敗の理由。
///
/// **「繋がらない」と「HTTP は返ってきたが 2xx ではない」を分ける。**
/// 一緒くたにすると、アプリは「ネットワークを確認してください」と
/// 「入力が正しくありません」を出し分けられない。
#[derive(Clone, Debug, PartialEq)]
pub enum NetError {
    /// 繋がらない・途中で切れた・時間切れ・CORS で弾かれた。
    Transport(String),
    /// 応答は返ってきたが 2xx ではない。**本文も持つ** — 業務 API は
    /// エラーの理由を本文の JSON で返すので、ここで捨てると出せなくなる。
    Status { status: u16, body: Vec<u8> },
    /// 本文を解釈できない (JSON の形が違う、UTF-8 でない)。
    Decode(String),
}

impl std::fmt::Display for NetError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            NetError::Transport(m) => write!(f, "接続できません: {m}"),
            NetError::Status { status, .. } => write!(f, "HTTP {status}"),
            NetError::Decode(m) => write!(f, "応答を解釈できません: {m}"),
        }
    }
}

impl std::error::Error for NetError {}

/// 組み立て中のリクエスト。
#[derive(Clone, Debug)]
pub struct RequestBuilder {
    method: Method,
    url: String,
    headers: Vec<(String, String)>,
    body: Option<Vec<u8>>,
    credentials: bool,
    timeout: Option<Duration>,
}

/// 任意のメソッドで組み立てる。
pub fn request(method: Method, url: impl Into<String>) -> RequestBuilder {
    RequestBuilder {
        method,
        url: url.into(),
        headers: Vec::new(),
        body: None,
        credentials: false,
        timeout: None,
    }
}

pub fn get(url: impl Into<String>) -> RequestBuilder {
    request(Method::Get, url)
}
pub fn post(url: impl Into<String>) -> RequestBuilder {
    request(Method::Post, url)
}
pub fn put(url: impl Into<String>) -> RequestBuilder {
    request(Method::Put, url)
}
pub fn patch(url: impl Into<String>) -> RequestBuilder {
    request(Method::Patch, url)
}
pub fn delete(url: impl Into<String>) -> RequestBuilder {
    request(Method::Delete, url)
}

impl RequestBuilder {
    /// ヘッダを 1 つ足す。同じ名前を複数回渡せる。
    pub fn header(mut self, name: impl Into<String>, value: impl Into<String>) -> Self {
        self.headers.push((name.into(), value.into()));
        self
    }

    /// 本文をそのまま送る。`Content-Type` は自分で付けること。
    pub fn body(mut self, bytes: impl Into<Vec<u8>>) -> Self {
        self.body = Some(bytes.into());
        self
    }

    /// 本文を JSON で送る (`Content-Type: application/json` も付く)。
    #[cfg(feature = "json")]
    pub fn json<T: serde::Serialize>(self, value: &T) -> Result<Self, NetError> {
        let bytes = serde_json::to_vec(value).map_err(|e| NetError::Decode(e.to_string()))?;
        Ok(self.header("content-type", "application/json").body(bytes))
    }

    /// Cookie を送る / 受け取る。
    ///
    /// wasm では `credentials: "include"`、native では共有の cookie jar。
    /// **Cookie セッションの業務 API ではこれが無いと何も通らない。**
    pub fn credentials(mut self, include: bool) -> Self {
        self.credentials = include;
        self
    }

    /// 時間切れ。既定は無期限 (ブラウザ / OS の既定に任せる)。
    pub fn timeout(mut self, d: Duration) -> Self {
        self.timeout = Some(d);
        self
    }

    /// 送って応答を待つ。
    ///
    /// **2xx でなくても `Ok`** を返す — 401 や 422 は「通信としては成功」で、
    /// 本文に理由が入っていることが多いため。エラーとして扱いたいときは
    /// [`Response::error_for_status`] を挟む。
    pub async fn send(self) -> Result<Response, NetError> {
        platform::send(self).await
    }
}

/// 返ってきた応答。
#[derive(Clone, Debug)]
pub struct Response {
    status: u16,
    headers: Vec<(String, String)>,
    body: Vec<u8>,
}

impl Response {
    pub fn status(&self) -> u16 {
        self.status
    }

    /// 2xx か。
    pub fn ok(&self) -> bool {
        (200..300).contains(&self.status)
    }

    /// ヘッダを 1 つ読む (名前は大文字小文字を区別しない)。
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(name))
            .map(|(_, v)| v.as_str())
    }

    pub fn bytes(&self) -> &[u8] {
        &self.body
    }

    /// 本文を文字列として読む。
    pub fn text(&self) -> Result<String, NetError> {
        String::from_utf8(self.body.clone()).map_err(|e| NetError::Decode(e.to_string()))
    }

    /// 本文を JSON として読む。
    #[cfg(feature = "json")]
    pub fn json<T: serde::de::DeserializeOwned>(&self) -> Result<T, NetError> {
        serde_json::from_slice(&self.body).map_err(|e| NetError::Decode(e.to_string()))
    }

    /// 2xx でなければ [`NetError::Status`] にする (本文ごと持っていく)。
    pub fn error_for_status(self) -> Result<Self, NetError> {
        if self.ok() {
            Ok(self)
        } else {
            Err(NetError::Status { status: self.status, body: self.body })
        }
    }

    /// テストから応答を組み立てる。
    ///
    /// 本物の API を叩かずに「401 が返ったらこの画面」を書けるようにする口。
    pub fn for_test(status: u16, body: impl Into<Vec<u8>>) -> Self {
        Self { status, headers: Vec::new(), body: body.into() }
    }
}

// ---------------------------------------------------------------------------
// native
// ---------------------------------------------------------------------------

#[cfg(not(target_arch = "wasm32"))]
mod platform {
    use super::{NetError, RequestBuilder, Response};

    /// Cookie を跨いで保つには**クライアントを使い回す**必要がある。
    /// 毎回作ると、ログインで受け取ったセッションが次のリクエストへ渡らない。
    fn client() -> Result<&'static reqwest::Client, NetError> {
        static CLIENT: std::sync::OnceLock<Result<reqwest::Client, String>> =
            std::sync::OnceLock::new();
        CLIENT
            .get_or_init(|| {
                reqwest::Client::builder()
                    .user_agent(concat!("sabitori-net/", env!("CARGO_PKG_VERSION")))
                    .cookie_store(true)
                    .build()
                    .map_err(|e| e.to_string())
            })
            .as_ref()
            .map_err(|e| NetError::Transport(e.clone()))
    }

    pub async fn send(req: RequestBuilder) -> Result<Response, NetError> {
        let method = reqwest::Method::from_bytes(req.method.as_str().as_bytes())
            .map_err(|e| NetError::Transport(e.to_string()))?;
        let mut r = client()?.request(method, &req.url);
        for (k, v) in &req.headers {
            r = r.header(k, v);
        }
        if let Some(body) = req.body {
            r = r.body(body);
        }
        if let Some(t) = req.timeout {
            r = r.timeout(t);
        }
        // native の cookie jar は常に効いている (クライアント側の設定)。
        // `credentials(false)` でも切らないのは、切り分けが per-request では
        // 効かないため — 必要なら別クライアントを持つこと。
        let resp = r.send().await.map_err(|e| NetError::Transport(e.to_string()))?;
        let status = resp.status().as_u16();
        let headers = resp
            .headers()
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_str().unwrap_or_default().to_string()))
            .collect();
        let body = resp
            .bytes()
            .await
            .map_err(|e| NetError::Transport(e.to_string()))?
            .to_vec();
        Ok(Response { status, headers, body })
    }
}

// ---------------------------------------------------------------------------
// wasm
// ---------------------------------------------------------------------------

#[cfg(target_arch = "wasm32")]
mod platform {
    use super::{NetError, RequestBuilder, Response};
    use wasm_bindgen::JsCast;
    use wasm_bindgen_futures::JsFuture;

    pub async fn send(req: RequestBuilder) -> Result<Response, NetError> {
        let opts = web_sys::RequestInit::new();
        opts.set_method(req.method.as_str());
        opts.set_mode(web_sys::RequestMode::Cors);
        if req.credentials {
            // Cookie セッションの業務 API はこれが無いと何も通らない。
            opts.set_credentials(web_sys::RequestCredentials::Include);
        }
        if let Some(body) = &req.body {
            let array = js_sys::Uint8Array::from(&body[..]);
            opts.set_body(&array);
        }

        // 時間切れは AbortController で。fetch 自体に timeout は無い。
        let controller = web_sys::AbortController::new().ok();
        if let (Some(c), Some(t)) = (controller.as_ref(), req.timeout) {
            opts.set_signal(Some(&c.signal()));
            if let Some(window) = web_sys::window() {
                let abort = c.clone();
                let cb = wasm_bindgen::closure::Closure::once_into_js(move || abort.abort());
                let _ = window.set_timeout_with_callback_and_timeout_and_arguments_0(
                    cb.unchecked_ref(),
                    t.as_millis() as i32,
                );
            }
        }

        let request = web_sys::Request::new_with_str_and_init(&req.url, &opts)
            .map_err(|e| NetError::Transport(format!("{e:?}")))?;
        for (k, v) in &req.headers {
            request
                .headers()
                .set(k, v)
                .map_err(|e| NetError::Transport(format!("{e:?}")))?;
        }

        let window = web_sys::window().ok_or_else(|| NetError::Transport("no window".into()))?;
        let resp_value = JsFuture::from(window.fetch_with_request(&request))
            .await
            .map_err(|e| NetError::Transport(format!("{e:?}")))?;
        let resp: web_sys::Response = resp_value
            .dyn_into()
            .map_err(|_| NetError::Transport("not a Response".into()))?;

        let status = resp.status();
        let buf = JsFuture::from(
            resp.array_buffer()
                .map_err(|e| NetError::Transport(format!("{e:?}")))?,
        )
        .await
        .map_err(|e| NetError::Transport(format!("{e:?}")))?;
        let body = js_sys::Uint8Array::new(&buf).to_vec();

        // ヘッダは `Headers` を回して拾う (CORS で見えるものだけ)。
        let mut headers = Vec::new();
        let iter = js_sys::try_iter(resp.headers().as_ref()).ok().flatten();
        if let Some(iter) = iter {
            for entry in iter.flatten() {
                let pair = js_sys::Array::from(&entry);
                if pair.length() == 2 {
                    headers.push((
                        pair.get(0).as_string().unwrap_or_default(),
                        pair.get(1).as_string().unwrap_or_default(),
                    ));
                }
            }
        }

        Ok(Response { status, headers, body })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn methods_render_as_the_wire_names() {
        assert_eq!(Method::Get.as_str(), "GET");
        assert_eq!(Method::Delete.as_str(), "DELETE");
    }

    /// **「繋がらない」と「HTTP エラー」を混ぜないこと。** アプリはこの区別で
    /// 「ネットワークを確認してください」と「入力が正しくありません」を
    /// 出し分ける。
    #[test]
    fn transport_and_status_are_different_failures() {
        let http = Response::for_test(422, r#"{"error":"氏名は必須です"}"#.as_bytes().to_vec())
            .error_for_status()
            .unwrap_err();
        match http {
            NetError::Status { status, body } => {
                assert_eq!(status, 422);
                assert!(!body.is_empty(), "本文を捨てると理由が出せない");
            }
            other => panic!("Status のはずが {other:?}"),
        }
        assert!(matches!(
            NetError::Transport("dns".into()),
            NetError::Transport(_)
        ));
    }

    /// 2xx はそのまま通ること。
    #[test]
    fn a_success_passes_through() {
        let r = Response::for_test(204, Vec::new()).error_for_status().unwrap();
        assert!(r.ok());
        assert_eq!(r.status(), 204);
    }

    #[test]
    fn the_builder_records_what_it_was_given() {
        let r = get("https://example.test/api")
            .header("x-token", "abc")
            .timeout(Duration::from_secs(3))
            .credentials(true);
        assert_eq!(r.method, Method::Get);
        assert_eq!(r.headers, vec![("x-token".to_string(), "abc".to_string())]);
        assert_eq!(r.timeout, Some(Duration::from_secs(3)));
        assert!(r.credentials);
    }

    #[cfg(feature = "json")]
    #[test]
    fn json_round_trips() {
        #[derive(serde::Serialize, serde::Deserialize, PartialEq, Debug)]
        struct Body {
            name: String,
        }
        let body = Body { name: "予約".into() };
        let req = post("/x").json(&body).unwrap();
        let sent = req.body.clone().unwrap();
        assert_eq!(
            req.headers,
            vec![("content-type".to_string(), "application/json".to_string())]
        );

        let back: Body = Response::for_test(200, sent).json().unwrap();
        assert_eq!(back, body);
    }

    /// 壊れた JSON は `Decode` で返る (`Transport` に混ぜない)。
    #[cfg(feature = "json")]
    #[test]
    fn broken_json_is_a_decode_error() {
        #[derive(serde::Deserialize, Debug)]
        struct Body {
            #[allow(dead_code)]
            name: String,
        }
        let e = Response::for_test(200, b"{ not json".to_vec())
            .json::<Body>()
            .unwrap_err();
        assert!(matches!(e, NetError::Decode(_)), "{e:?}");
    }
}
