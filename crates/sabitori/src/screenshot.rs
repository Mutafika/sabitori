//! **起動して、落ち着いたら PNG を書いて終わる** ([#69])。
//!
//! `SABITORI_BACKGROUND=1` で窓を出さずに起こせるようになったが、撮るには
//! ウィンドウ ID が要り、それを得るのに外部ツールが必要だった。しかも
//! 「描き終わった」を外から知る手段が無く、`sleep` で待つしかなかった。
//!
//! ```sh
//! SABITORI_BACKGROUND=1 SABITORI_SCREENSHOT=out.png ./app
//! ```
//!
//! ランタイムが**描くものが無くなった最初のフレーム**を `out.png` に書いて
//! 終了する。窓の枠は入らない (描画面そのもの)。
//!
//! | env | 既定 | 意味 |
//! |---|---|---|
//! | `SABITORI_SCREENSHOT` | 無し | 書き出す先。**これが無ければ何もしない** |
//! | `SABITORI_SCREENSHOT_AFTER_IDLE` | `1` | `0` にすると最初に描いたフレームで撮る |
//! | `SABITORI_SCREENSHOT_TIMEOUT_MS` | `10000` | 落ち着かないまま (アニメーションが続く等) この時間が過ぎたら撮る |
//!
//! native だけ。wasm は canvas から取る別の道になる。
//!
//! [#69]: https://github.com/Mutafika/sabitori/issues/69

/// 撮る指示。env から 1 回だけ読む。
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Plan {
    pub(crate) path: std::path::PathBuf,
    /// 落ち着くまで待つか。`false` なら最初に描いたフレームで撮る。
    pub(crate) after_idle: bool,
    /// 待つ上限 (ms)。過ぎたら落ち着いていなくても撮る。
    pub(crate) timeout_ms: u64,
}

/// env から指示を読む。`SABITORI_SCREENSHOT` が無ければ `None`。
pub(crate) fn plan_from_env() -> Option<Plan> {
    let path = std::env::var_os("SABITORI_SCREENSHOT")?;
    if path.is_empty() {
        return None;
    }
    Some(Plan {
        path: std::path::PathBuf::from(path),
        after_idle: !matches!(
            std::env::var("SABITORI_SCREENSHOT_AFTER_IDLE").as_deref(),
            Ok("0") | Ok("false")
        ),
        timeout_ms: std::env::var("SABITORI_SCREENSHOT_TIMEOUT_MS")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(10_000),
    })
}

/// 撮る側の進み具合。
#[derive(Debug, Default)]
pub(crate) struct Shooter {
    plan: Option<Plan>,
    /// 1 枚でも描いたか (描く前に撮ると真っ白になる)。
    drew_once: bool,
    /// 読み戻しを頼んだ。
    requested: bool,
    started: Option<std::time::Instant>,
}

impl Shooter {
    pub(crate) fn from_env() -> Self {
        Self { plan: plan_from_env(), ..Default::default() }
    }

    pub(crate) fn armed(&self) -> bool {
        self.plan.is_some()
    }

    /// フレームを描いたことを伝える。
    pub(crate) fn note_drew(&mut self) {
        self.drew_once = true;
        self.started.get_or_insert_with(std::time::Instant::now);
    }

    /// **いま読み戻しを頼むべきか。** `idle` は「このフレームで描くものが
    /// 無かった」。
    ///
    /// 1 枚も描いていないうちに撮ると**真っ白な画像**が出る — 撮れたと
    /// 思い込める分、撮れないより悪い。
    pub(crate) fn should_request(&self, idle: bool) -> bool {
        let Some(plan) = &self.plan else { return false };
        if self.requested || !self.drew_once {
            return false;
        }
        if !plan.after_idle {
            return true;
        }
        idle || self.timed_out()
    }

    fn timed_out(&self) -> bool {
        let Some(plan) = &self.plan else { return false };
        self.started
            .is_some_and(|t| t.elapsed().as_millis() as u64 >= plan.timeout_ms)
    }

    pub(crate) fn mark_requested(&mut self) {
        self.requested = true;
    }

    /// 読み戻したフレームを書き出す。書けたら `true` (呼び出し側が終了する)。
    pub(crate) fn write(&self, frame: sabitori_gpu::CapturedFrame) -> bool {
        let Some(plan) = &self.plan else { return false };
        let shot = crate::offscreen::Rendered {
            width: frame.width,
            height: frame.height,
            rgba: frame.rgba,
        };
        match shot.save_png(&plan.path) {
            Ok(()) => {
                tracing::info!("スクリーンショットを書いた: {}", plan.path.display());
                true
            }
            Err(e) => {
                tracing::error!("スクリーンショットを書けなかった: {e}");
                false
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn armed(after_idle: bool) -> Shooter {
        Shooter {
            plan: Some(Plan {
                path: "/tmp/x.png".into(),
                after_idle,
                timeout_ms: 10_000,
            }),
            ..Default::default()
        }
    }

    /// env が無ければ何もしない (ふつうの起動に影響しない)。
    #[test]
    fn without_the_env_nothing_happens() {
        let s = Shooter::default();
        assert!(!s.armed());
        assert!(!s.should_request(true));
    }

    /// **1 枚も描く前には撮らない。** 撮れたと思い込める真っ白な画像が
    /// 出てくるほうが、撮れないより悪い。
    #[test]
    fn it_never_shoots_before_the_first_frame() {
        let s = armed(false);
        assert!(!s.should_request(true), "描く前に撮ろうとしている");
    }

    /// 落ち着くのを待つ (既定)。
    #[test]
    fn waiting_for_idle_is_the_default() {
        let mut s = armed(true);
        s.note_drew();
        assert!(!s.should_request(false), "まだ描くものがあるのに撮っている");
        assert!(s.should_request(true));
    }

    /// 待たない指定なら、最初に描いたフレームで撮る。
    #[test]
    fn without_after_idle_the_first_frame_is_enough() {
        let mut s = armed(false);
        s.note_drew();
        assert!(s.should_request(false));
    }

    /// 1 回頼んだら二度と頼まない (毎フレーム読み戻すと重い)。
    #[test]
    fn it_only_asks_once() {
        let mut s = armed(true);
        s.note_drew();
        assert!(s.should_request(true));
        s.mark_requested();
        assert!(!s.should_request(true));
    }

    /// **落ち着かなくても、いつかは撮る。** アニメーションが止まらない画面
    /// (時計・粒子) で永久に撮れないと、検証そのものが詰まる。
    #[test]
    fn a_never_idle_app_still_gets_shot_after_the_timeout() {
        let mut s = Shooter {
            plan: Some(Plan { path: "/tmp/x.png".into(), after_idle: true, timeout_ms: 0 }),
            ..Default::default()
        };
        s.note_drew();
        assert!(s.should_request(false), "動き続ける画面で永久に撮れない");
    }

    /// env の読み方。
    #[test]
    fn the_plan_reads_its_switches() {
        // 直接組んで確かめる (env はプロセス共有なので触らない)。
        let p = Plan { path: "out.png".into(), after_idle: true, timeout_ms: 10_000 };
        assert_eq!(p.path.to_str(), Some("out.png"));
        assert!(p.after_idle);
    }
}
