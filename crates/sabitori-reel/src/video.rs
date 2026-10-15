//! 動画素材を reel に貼る ([`Video`])。
//!
//! `ffmpeg` に動画を生の RGBA で流させ、場面の時刻のコマを画像として返す。reel は
//! コマを頭から順に描くので、ふつうは続きを読むだけで済む。戻ったとき (プレビューの
//! 巻き戻し) や大きく飛んだときは、その時刻から `ffmpeg` を開き直す。

use std::cell::RefCell;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use sabitori_core::element::{image, Element, ImageData};

/// 書き出しと同じ `ffmpeg` (`SABITORI_REEL_FFMPEG` で差し替えられる) を呼ぶ形。
pub(crate) fn ffmpeg_command() -> Command {
    let ffmpeg = std::env::var("SABITORI_REEL_FFMPEG").unwrap_or_else(|_| "ffmpeg".to_string());
    let mut cmd = Command::new(&ffmpeg);
    // Let a self-contained ffmpeg find its bundled dylibs (Remotion's build
    // needs this on macOS). Skipped for a bare "ffmpeg" from PATH.
    if let Some(parent) = Path::new(&ffmpeg).parent().filter(|p| !p.as_os_str().is_empty()) {
        cmd.env("DYLD_LIBRARY_PATH", parent);
    }
    cmd
}

/// 貼る動画 1 本。[`Video::frame`] / [`Video::element`] で、場面の時刻 (秒、動画の頭が 0)
/// のコマを取り出す。
///
/// ```ignore
/// let clip = Video::open("assets/screen.mp4", 960, 540, 30)?;
/// let scene = move |ctx: FrameCtx| {
///     div().w(Px(1920.0)).h(Px(1080.0))
///         .child(clip.element(ctx.t() - 1.0).w(Px(960.0)).h(Px(540.0)).rounded_px(16.0))
/// };
/// ```
///
/// 開いたときの大きさ (`width × height`) と `fps` に揃えて取り出す。場面と同じ fps に
/// しておけば、コマの取りこぼしも重複も無い。モーションブラーの標本 (コマの間の時刻) は
/// 同じコマを返す。
pub struct Video {
    path: PathBuf,
    width: u32,
    height: u32,
    fps: u32,
    id: u64,
    state: RefCell<Decoder>,
}

#[derive(Default)]
struct Decoder {
    child: Option<Child>,
    /// 次に読むコマの番号。
    next: u64,
    /// 最後に読んだコマ (番号, 画素)。
    last: Option<(u64, Arc<Vec<u8>>)>,
    /// 流れが尽きた (これより先にコマは無い)。
    ended: bool,
}

impl Drop for Decoder {
    fn drop(&mut self) {
        if let Some(mut c) = self.child.take() {
            let _ = c.kill();
            let _ = c.wait();
        }
    }
}

static NEXT_ID: AtomicU64 = AtomicU64::new(0);

impl Video {
    /// `path` の動画を、`width × height` (px) と `fps` で取り出せるように開く。
    /// `ffmpeg` が無い・ファイルが無いときは、最初のコマを読むところで分かる
    /// ([`Video::frame`] が `None`)。ここでは開けるかだけを確かめる。
    pub fn open(path: impl AsRef<Path>, width: u32, height: u32, fps: u32) -> std::io::Result<Self> {
        let path = path.as_ref().to_path_buf();
        if !path.exists() {
            return Err(std::io::Error::new(std::io::ErrorKind::NotFound, format!("{} が無い", path.display())));
        }
        Ok(Self {
            path,
            width: width.max(1),
            height: height.max(1),
            fps: fps.max(1),
            id: NEXT_ID.fetch_add(1, Ordering::Relaxed),
            state: RefCell::new(Decoder::default()),
        })
    }

    /// 動画の時刻 `t` 秒のコマ。頭より前・終わりより後は `None`。
    pub fn frame(&self, t: f32) -> Option<ImageData> {
        self.frame_at(t).map(|(_, rgba)| ImageData { rgba, width: self.width, height: self.height })
    }

    /// [`Video::frame`] を画像の要素にしたもの。コマが無ければ何も描かない箱
    /// (置き場所は変わらない)。大きさや角丸は呼び手が付ける。
    pub fn element(&self, t: f32) -> Element {
        match self.frame_at(t) {
            Some((idx, rgba)) => image(
                format!("reel-video-{}-{idx}", self.id),
                ImageData { rgba, width: self.width, height: self.height },
            ),
            None => sabitori_core::element::div(),
        }
    }

    fn frame_at(&self, t: f32) -> Option<(u64, Arc<Vec<u8>>)> {
        if !t.is_finite() || t < 0.0 {
            return None;
        }
        // コマの間の時刻 (モーションブラーの標本) は同じコマ。浮動小数の誤差で 1 つ前に
        // 落ちないよう少しだけ足す。
        let idx = (t * self.fps as f32 + 1e-3).floor() as u64;
        let mut st = self.state.borrow_mut();
        if let Some((n, rgba)) = &st.last {
            if *n == idx {
                return Some((idx, rgba.clone()));
            }
        }
        // 戻った・大きく飛んだ・まだ開いていない: その時刻から開き直す。
        let far = idx > st.next + self.fps as u64 * 2;
        if st.child.is_none() || idx < st.next || far {
            if st.ended && !far && idx >= st.next {
                return None;
            }
            self.restart(&mut st, idx);
        }
        let size = (self.width * self.height * 4) as usize;
        while st.next <= idx {
            let mut buf = vec![0u8; size];
            let read = st.child.as_mut().and_then(|c| c.stdout.as_mut()).map(|out| out.read_exact(&mut buf));
            if !matches!(read, Some(Ok(()))) {
                st.ended = true;
                if let Some(mut c) = st.child.take() {
                    let _ = c.kill();
                    let _ = c.wait();
                }
                return None;
            }
            let n = st.next;
            st.next += 1;
            st.last = Some((n, Arc::new(buf)));
        }
        st.last.clone()
    }

    fn restart(&self, st: &mut Decoder, idx: u64) {
        if let Some(mut c) = st.child.take() {
            let _ = c.kill();
            let _ = c.wait();
        }
        let start = idx as f64 / self.fps as f64;
        let filter = format!("fps={},scale={}:{}", self.fps, self.width, self.height);
        let child = ffmpeg_command()
            .args(["-v", "error", "-ss", &format!("{start:.6}"), "-i"])
            .arg(&self.path)
            .args(["-vf", &filter, "-f", "rawvideo", "-pix_fmt", "rgba", "-"])
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .ok();
        st.child = child;
        st.next = idx;
        st.last = None;
        st.ended = false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `ffmpeg` で、前半が赤・後半が青の 1 秒 (10fps) の動画を作る。無ければ `None`。
    fn red_then_blue(dir: &Path) -> Option<PathBuf> {
        let out = dir.join("rb.mp4");
        let ok = ffmpeg_command()
            .args([
                "-v", "error", "-y",
                "-f", "lavfi", "-i", "color=c=red:s=32x32:d=0.5:r=10",
                "-f", "lavfi", "-i", "color=c=blue:s=32x32:d=0.5:r=10",
                "-filter_complex", "[0:v][1:v]concat=n=2:v=1[v]", "-map", "[v]",
                "-pix_fmt", "yuv420p",
            ])
            .arg(&out)
            .status()
            .map(|s| s.success())
            .unwrap_or(false);
        if !ok {
            eprintln!("skip: ffmpeg が無い");
            return None;
        }
        Some(out)
    }

    fn color(v: &Video, t: f32) -> Option<[u8; 3]> {
        let f = v.frame(t)?;
        let i = ((16 * 32 + 16) * 4) as usize;
        Some([f.rgba[i], f.rgba[i + 1], f.rgba[i + 2]])
    }

    fn is_red(c: [u8; 3]) -> bool {
        c[0] > 180 && c[2] < 80
    }

    fn is_blue(c: [u8; 3]) -> bool {
        c[2] > 180 && c[0] < 80
    }

    /// 時刻のコマが出る。前へ進んでも、戻っても、飛んでも同じ時刻なら同じ色。
    #[test]
    fn frames_follow_the_clip_forwards_and_backwards() {
        let dir = std::env::temp_dir().join(format!("sabitori-reel-video-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let Some(path) = red_then_blue(&dir) else { return };
        let v = Video::open(&path, 32, 32, 10).unwrap();
        assert!(is_red(color(&v, 0.0).unwrap()), "頭が赤でない");
        assert!(is_red(color(&v, 0.3).unwrap()), "0.3 秒が赤でない");
        assert!(is_blue(color(&v, 0.8).unwrap()), "0.8 秒が青でない");
        // 戻る (開き直す)。
        assert!(is_red(color(&v, 0.1).unwrap()), "戻ったのに赤でない");
        // コマの間の時刻は同じコマ。
        let a = v.frame(0.2).unwrap();
        let b = v.frame(0.27).unwrap();
        assert!(Arc::ptr_eq(&a.rgba, &b.rgba), "コマの間で別のコマになった");
        // 終わりの後と頭の前は無い。
        assert!(v.frame(5.0).is_none(), "終わりの後にコマがある");
        assert!(v.frame(-0.1).is_none(), "頭の前にコマがある");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
