//! 主窓を複数持つ ([`DeclarativeApp::take_new_windows`])。
//!
//! 窓ごとに [`AppState`] を 1 つ持つ — アプリの値も、入力・描画・別窓の状態もまるごと
//! 窓ごと。winit のループは 1 本なので、ここが窓の id で振り分け、閉じた窓を落とし、
//! 頼まれた窓を作り、次に起きる時刻を窓どうしで合わせる。窓が 1 つのアプリは、
//! `AppState` をそのまま回していたときと同じに動く (閉じたら終わる)。

use winit::application::ApplicationHandler;
use winit::dpi::{PhysicalPosition, Position};
use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, ControlFlow};
use winit::window::WindowId;

use super::{AppState, DeclarativeApp};

/// 新しい窓を、頼んだ窓からずらす量 (論理 px)。
const CASCADE: f64 = 24.0;

pub(crate) struct Host<A: DeclarativeApp> {
    windows: Vec<AppState<A>>,
    /// 最後に焦点を持った主窓。OS から渡されたパスと開き直しの届け先。
    focused: Option<WindowId>,
}

impl<A: DeclarativeApp> Host<A> {
    pub(crate) fn new(app: A) -> Self {
        Self {
            windows: vec![AppState::new(app)],
            focused: None,
        }
    }

    /// 閉じた窓を落とす (アプリの `Drop` もここで走る)。残りが無ければ終わる。
    fn reap(&mut self, event_loop: &ActiveEventLoop) {
        self.windows.retain(|w| !w.closed);
        if self.windows.is_empty() {
            event_loop.exit();
        }
    }

    /// アプリが頼んだ窓を作る。頼んだ窓から少しずつずらして置く。作ったら `true`。
    fn open_requested(&mut self, event_loop: &ActiveEventLoop) -> bool {
        let mut opened = Vec::new();
        for w in &mut self.windows {
            let apps = w.app.take_new_windows();
            if apps.is_empty() {
                continue;
            }
            let from = w.window.as_ref().and_then(|win| {
                let pos = win.outer_position().ok()?;
                Some((pos, CASCADE * win.scale_factor()))
            });
            for (k, app) in apps.into_iter().enumerate() {
                let mut state = AppState::new(app);
                state.position_hint = from.map(|(pos, step)| {
                    let d = step * (k + 1) as f64;
                    Position::Physical(PhysicalPosition::new(
                        (pos.x as f64 + d) as i32,
                        (pos.y as f64 + d) as i32,
                    ))
                });
                opened.push(state);
            }
        }
        let any = !opened.is_empty();
        for mut state in opened {
            state.resumed(event_loop);
            self.windows.push(state);
        }
        any
    }

    /// OS から渡されたパスと開き直しを、最後に焦点を持った窓のアプリへ (macOS)。
    /// 窓ができるまでは溜めたまま待つ — 起動時の分を窓のない状態で渡さない。
    fn deliver_from_os(&mut self) {
        #[cfg(target_os = "macos")]
        {
            let ids: Vec<Option<WindowId>> = self
                .windows
                .iter()
                .map(|w| w.window.as_ref().map(|win| win.id()))
                .collect();
            let Some(i) = open_target(&ids, self.focused.as_ref()) else {
                return;
            };
            let paths = crate::macos_open::take();
            if !paths.is_empty() {
                self.windows[i].app.on_open_paths(paths);
            }
            for visible in crate::macos_open::take_reopens() {
                self.windows[i].app.on_reopen(visible);
            }
        }
    }
}

impl<A: DeclarativeApp> ApplicationHandler for Host<A> {
    fn user_event(&mut self, event_loop: &ActiveEventLoop, event: ()) {
        for w in &mut self.windows {
            w.user_event(event_loop, event);
        }
        self.deliver_from_os();
    }

    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        for w in &mut self.windows {
            w.resumed(event_loop);
        }
        // 窓ができる前に届いていた分 (起動のきっかけになったパス)。
        self.deliver_from_os();
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, id: WindowId, event: WindowEvent) {
        let Some(w) = self.windows.iter_mut().find(|w| w.owns(id)) else {
            return;
        };
        let primary = w.window.as_ref().is_some_and(|win| win.id() == id);
        if primary && matches!(event, WindowEvent::Focused(true)) {
            self.focused = Some(id);
        }
        w.window_event(event_loop, id, event);
        if w.closed {
            self.reap(event_loop);
        }
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        // 窓ごとに次に起きたい時刻を聞き、いちばん早いものにする。何も言わない窓
        // (閉じた・間引いた) は「待つ」扱い — 前の窓が置いた値を読まないよう毎回戻す。
        let mut flow = ControlFlow::Wait;
        for w in &mut self.windows {
            event_loop.set_control_flow(ControlFlow::Wait);
            w.about_to_wait(event_loop);
            flow = more_urgent(flow, event_loop.control_flow());
        }
        self.reap(event_loop);
        if self.windows.is_empty() {
            return;
        }
        if self.open_requested(event_loop) {
            // 新しい窓の最初の 1 枚をすぐに描く。
            flow = ControlFlow::Poll;
        }
        event_loop.set_control_flow(flow);
    }
}

/// 2 つの窓の起きたい時刻のうち、早い方。
fn more_urgent(a: ControlFlow, b: ControlFlow) -> ControlFlow {
    use ControlFlow::{Poll, Wait, WaitUntil};
    match (a, b) {
        (Poll, _) | (_, Poll) => Poll,
        (WaitUntil(x), WaitUntil(y)) => WaitUntil(x.min(y)),
        (WaitUntil(t), Wait) | (Wait, WaitUntil(t)) => WaitUntil(t),
        (Wait, Wait) => Wait,
    }
}

/// OS から渡されたパスと開き直しの届け先。最後に焦点を持った窓、それが無ければ (閉じた・
/// まだ焦点が来ていない) 窓ができている最初のもの。どれも窓が無ければ `None`。
fn open_target<T: PartialEq>(ids: &[Option<T>], focused: Option<&T>) -> Option<usize> {
    focused
        .and_then(|f| ids.iter().position(|id| id.as_ref() == Some(f)))
        .or_else(|| ids.iter().position(Option::is_some))
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, Instant};

    use super::*;

    #[test]
    fn the_earliest_wake_wins() {
        let now = Instant::now();
        let soon = ControlFlow::WaitUntil(now + Duration::from_millis(8));
        let later = ControlFlow::WaitUntil(now + Duration::from_millis(16));
        assert_eq!(more_urgent(later, soon), soon);
        assert_eq!(more_urgent(soon, later), soon);
        assert_eq!(more_urgent(ControlFlow::Wait, later), later, "待つだけの窓は急がない");
        assert_eq!(more_urgent(later, ControlFlow::Wait), later);
        assert_eq!(more_urgent(soon, ControlFlow::Poll), ControlFlow::Poll);
        assert_eq!(more_urgent(ControlFlow::Wait, ControlFlow::Wait), ControlFlow::Wait);
    }

    #[test]
    fn opened_paths_go_to_the_last_focused_window() {
        let ids = [Some(1), Some(2), Some(3)];
        assert_eq!(open_target(&ids, Some(&2)), Some(1));
        assert_eq!(open_target(&ids, None), Some(0), "焦点がまだ来ていなければ最初の窓");
        assert_eq!(open_target(&ids, Some(&9)), Some(0), "焦点の窓が閉じていれば最初の窓");
    }

    #[test]
    fn opened_paths_wait_for_a_window() {
        assert_eq!(open_target::<u32>(&[None], None), None, "窓ができるまで渡さない");
        assert_eq!(open_target(&[None, Some(5)], None), Some(1));
    }
}
