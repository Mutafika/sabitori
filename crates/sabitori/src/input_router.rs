//! Shared pointer/touch gesture plumbing used by both `declarative.rs` and
//! `scene_app.rs`. The runtimes differ in what they do with gestures (managed
//! scroll, drag-and-drop, custom scene panning) but they share the same
//! state model: which modality owns the primary flow, which finger is
//! driving the single-touch path, and whether a two-finger pinch is active.

/// Minimum distance (logical px) a touch must travel before it's considered a
/// scroll/drag rather than a tap. Matches Android's default touch slop range.
pub(crate) const TOUCH_SLOP: f32 = 10.0;

/// Per-touch state for the primary finger. Drives tap-vs-scroll disambiguation.
pub(crate) struct TouchDrag {
    pub id: u64,
    pub start: (f32, f32),
    pub last: (f32, f32),
    /// Wall-clock of the previous Moved sample, used to compute velocity for fling.
    pub last_move_time: Option<web_time::Instant>,
    /// Id of the topmost clickable region under the initial touch, if any.
    pub click_target: Option<String>,
    /// Id of the nearest managed scroll container under the initial touch, if any.
    /// Only used by runtimes that have managed scroll containers.
    pub scroll_target: Option<String>,
    /// Once the finger crosses [`TOUCH_SLOP`] this is set; no tap will fire on release.
    pub moved_beyond_slop: bool,
    /// この指の押下が連続タップの何回目か ([`sabitori_input::ClickCounter`])。
    /// タップは解放で確定するので、押下時に数えた値をここで運んで
    /// `on_double_click` の判定に使う。
    pub click_count: u32,
}

/// winit のホイール delta を、配る単位 (論理 px) と精度フラグに直す。
///
/// `LineDelta` (刻みホイール) は [`sabitori_input::LINE_DELTA_PX`] 倍、`PixelDelta`
/// (トラックパッド) はそのまま。2 ランタイムが別々に `* 20.0` を書いていたのを
/// 1 箇所にした。戻り値は `(delta_x, delta_y, precise)`。
pub(crate) fn wheel_delta_px(delta: winit::event::MouseScrollDelta) -> (f32, f32, bool) {
    match delta {
        winit::event::MouseScrollDelta::LineDelta(x, y) => {
            let k = sabitori_input::LINE_DELTA_PX;
            (x * k, y * k, false)
        }
        winit::event::MouseScrollDelta::PixelDelta(pos) => (pos.x as f32, pos.y as f32, true),
    }
}

/// winit の `TouchPhase` (ホイールにも付いてくる) を [`sabitori_input::WheelPhase`] へ。
pub(crate) fn wheel_phase(phase: winit::event::TouchPhase) -> sabitori_input::WheelPhase {
    use sabitori_input::WheelPhase;
    match phase {
        winit::event::TouchPhase::Started => WheelPhase::Started,
        winit::event::TouchPhase::Moved => WheelPhase::Moved,
        winit::event::TouchPhase::Ended => WheelPhase::Ended,
        winit::event::TouchPhase::Cancelled => WheelPhase::Cancelled,
    }
}

/// Two-finger pinch gesture state.
pub(crate) struct PinchGesture {
    pub id_a: u64,
    pub id_b: u64,
    /// Distance between the two fingers when the gesture started. Used to
    /// compute the absolute scale factor (current / start).
    pub start_distance: f32,
}

/// Which input modality currently owns the primary-pointer flow.
///
/// First-come wins: once set to `Mouse` or `Touch`, events from the other
/// modality are ignored for primary routing (click / scroll / drag / hover)
/// until this returns to `None`. Raw `InputEvent::Pointer*` still fires for
/// apps that want both streams.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum PrimaryInput {
    None,
    Mouse,
    Touch,
}

/// Distance + midpoint between two active touches, or `None` if either is missing.
pub(crate) fn pinch_metrics(
    active: &std::collections::HashMap<u64, (f32, f32)>,
    id_a: u64,
    id_b: u64,
) -> Option<(f32, (f32, f32))> {
    let a = active.get(&id_a)?;
    let b = active.get(&id_b)?;
    let dx = a.0 - b.0;
    let dy = a.1 - b.1;
    let distance = (dx * dx + dy * dy).sqrt();
    let center = ((a.0 + b.0) * 0.5, (a.1 + b.1) * 0.5);
    Some((distance, center))
}

/// 下限。 `1.0 + delta` が 0 以下になると倍率の符号が反転して、 以降ずっと
/// 壊れた値を配ることになる。 factor 側で床を張って絶たれないようにする。
const MIN_PINCH_FACTOR: f32 = 0.01;

/// トラックパッド (macOS) のピンチ状態。
///
/// winit は [`WindowEvent::PinchGesture`] を**増分** (`delta`) で配るが、
/// [`DeclarativeApp::on_pinch`] の規約は「つまみ始めからの**累積**倍率」
/// — タッチ側の `dist / start_distance` と同じ意味 — なので、 ここで
/// 積んでから渡す。 受け手が両方の入力を区別せずに書けるのがこの型の役目。
///
/// [`WindowEvent::PinchGesture`]: winit::event::WindowEvent::PinchGesture
/// [`DeclarativeApp::on_pinch`]: crate::DeclarativeApp::on_pinch
pub(crate) struct TrackpadPinch {
    /// つまみ始め (= 1.0) からの累積倍率。
    pub scale: f32,
}

impl TrackpadPinch {
    /// つまみ始め。 倍率は 1.0 から。
    pub fn started() -> Self {
        Self { scale: 1.0 }
    }

    /// 増分 `delta` を積んで、 新しい累積倍率を返す。
    ///
    /// winit の doc が「`delta` は NaN になり得る」 と言っているので、
    /// 有限でない値は**捨てる** (`None`) — 一度でも混ぜると `scale` が NaN に
    /// 固着して、 以降そのジェスチャが丸ごと死ぬ。
    pub fn apply(&mut self, delta: f64) -> Option<f32> {
        if !delta.is_finite() {
            return None;
        }
        let factor = (1.0 + delta as f32).max(MIN_PINCH_FACTOR);
        self.scale *= factor;
        Some(self.scale)
    }
}

#[cfg(test)]
mod trackpad_pinch_tests {
    use super::*;

    /// 規約の確認: `on_pinch` に渡すのは**累積**倍率で、 1.0 から始まる。
    ///
    /// タッチ側は `dist / start_distance` を渡している。 ここが増分のままだと
    /// 受け手が入力の出どころで場合分けする羽目になり、 「片方だけ効く」
    /// 実装が量産される。
    #[test]
    fn scale_accumulates_from_one() {
        let mut p = TrackpadPinch::started();
        assert_eq!(p.scale, 1.0);

        // +10% を 2 回 → 1.21 (増分の和 1.2 ではない)。
        assert_eq!(p.apply(0.1), Some(1.1));
        let after = p.apply(0.1).unwrap();
        assert!((after - 1.21).abs() < 1e-6, "{after}");
    }

    /// 縮小方向も同じ積み方で、 拡大を打ち消せば 1.0 に戻ること。
    #[test]
    fn shrinking_undoes_magnifying() {
        let mut p = TrackpadPinch::started();
        p.apply(0.25);
        let back = p.apply(1.0 / 1.25 - 1.0).unwrap();
        assert!((back - 1.0).abs() < 1e-6, "{back}");
    }

    /// winit の doc が「NaN が来ることがある」 と言っている。 積んでしまうと
    /// `scale` が NaN に固着して、 そのジェスチャが丸ごと死ぬ。 捨てること。
    #[test]
    fn non_finite_delta_is_dropped_and_leaves_scale_intact() {
        let mut p = TrackpadPinch::started();
        p.apply(0.5);
        let good = p.scale;

        for bad in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
            assert_eq!(p.apply(bad), None, "{bad} を受け付けている");
            assert_eq!(p.scale, good, "{bad} で scale が壊れた");
        }

        // 捨てたあとも普通に続けられること。
        assert!(p.apply(0.1).is_some());
    }

    /// `1.0 + delta` が 0 以下になっても倍率の符号を反転させないこと。
    /// 一度負に振れると、 以降の拡大縮小が全部裏返って戻らない。
    #[test]
    fn extreme_negative_delta_never_flips_the_sign() {
        let mut p = TrackpadPinch::started();
        for delta in [-1.0, -2.0, -50.0] {
            let scale = p.apply(delta).expect("有限値は受け付ける");
            assert!(scale > 0.0, "delta={delta} で scale={scale} が非正になった");
        }
        // 床を張ったあとも、 拡大方向へ復帰できること。
        assert!(p.apply(1.0).unwrap() > p.scale / 2.0);
    }
}
