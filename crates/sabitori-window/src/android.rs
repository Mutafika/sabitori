//! Android の入口と、Android でしか要らない小物。
//!
//! Android のアプリは `main` を持たない。OS が Activity を作り、ネイティブ
//! ライブラリの `android_main(app)` を別スレッドで呼ぶ。winit はその `app` が
//! 無いとイベントループを作れないので、最初に [`init`] で預かる:
//!
//! ```ignore
//! #[unsafe(no_mangle)]
//! fn android_main(app: sabitori::android::AndroidApp) {
//!     sabitori::android::init(app);
//!     sabitori::run_declarative(MyApp::new());
//! }
//! ```
//!
//! アプリの crate は `[lib] crate-type = ["cdylib"]`。

use std::io::Write;
use std::sync::OnceLock;

pub use winit::platform::android::activity::AndroidApp;

static APP: OnceLock<AndroidApp> = OnceLock::new();

/// `android_main` が受け取った `app` を預ける。`run_*` より前に呼ぶ。
/// 併せて、ログと panic を logcat に向ける (Android は stdout / stderr を捨てる)。
pub fn init(app: AndroidApp) {
    let _ = APP.set(app);
    std::panic::set_hook(Box::new(|info| {
        log_line(LOG_ERROR, &format!("panic: {info}"));
    }));
}

/// [`init`] で預かった `app`。
pub fn app() -> Option<&'static AndroidApp> {
    APP.get()
}

const LOG_INFO: i32 = 4;
const LOG_ERROR: i32 = 6;

#[link(name = "log")]
unsafe extern "C" {
    fn __android_log_write(prio: i32, tag: *const std::ffi::c_char, text: *const std::ffi::c_char) -> i32;
}

fn log_line(prio: i32, line: &str) {
    // 途中に NUL があると CString にならないので、そこまでを出す。
    let cut = line.split('\0').next().unwrap_or("");
    if let Ok(text) = std::ffi::CString::new(cut) {
        // SAFETY: どちらも NUL 終端の有効な C 文字列で、呼び出しの間だけ生きていればよい。
        unsafe { __android_log_write(prio, c"sabitori".as_ptr(), text.as_ptr()) };
    }
}

/// `tracing_subscriber::fmt().with_writer(..)` に渡す、logcat への書き手。
/// 1 行ごとに 1 件のログになる。
#[derive(Clone, Copy, Default)]
pub struct Logcat;

/// [`Logcat`] が貸し出す書き手。行が揃うまで溜める。
#[derive(Default)]
pub struct LogcatLine(Vec<u8>);

impl Write for LogcatLine {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.0.extend_from_slice(buf);
        while let Some(end) = self.0.iter().position(|&b| b == b'\n') {
            let line: Vec<u8> = self.0.drain(..=end).collect();
            log_line(LOG_INFO, String::from_utf8_lossy(&line[..end]).as_ref());
        }
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        if !self.0.is_empty() {
            let line = std::mem::take(&mut self.0);
            log_line(LOG_INFO, String::from_utf8_lossy(&line).as_ref());
        }
        Ok(())
    }
}

impl Drop for LogcatLine {
    fn drop(&mut self) {
        let _ = self.flush();
    }
}

impl<'a> tracing_subscriber::fmt::MakeWriter<'a> for Logcat {
    type Writer = LogcatLine;

    fn make_writer(&'a self) -> Self::Writer {
        LogcatLine::default()
    }
}
