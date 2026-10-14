//! 外 (Finder など) から持ち込まれたファイルのドラッグ: 入れ先の drop zone が `ctx.drag` に
//! 出ること、落とすと 1 回分まとめて落とした所の drop zone と一緒に届くこと。

use std::cell::RefCell;
use std::path::PathBuf;

use sabitori::testing::Harness;
use sabitori::*;

#[derive(Default)]
struct Folders {
    /// view が見た `ctx.drag` (data, over_drop_zone)。
    seen: RefCell<Option<(String, Option<String>)>>,
    dropped: Vec<(Vec<PathBuf>, Option<String>)>,
    hovered: usize,
    cancelled: usize,
}

impl DeclarativeApp for Folders {
    fn view(&self, ctx: &ViewContext) -> Element {
        *self.seen.borrow_mut() = ctx
            .drag
            .as_ref()
            .map(|d| (d.data.clone(), d.over_drop_zone.clone()));
        div()
            .w(Px(ctx.width))
            .h(Px(ctx.height))
            .flex_col()
            .children([
                div().id("docs").droppable().w(Px(200.0)).h(Px(40.0)),
                div().id("plain").w(Px(200.0)).h(Px(40.0)),
            ])
    }

    fn on_file_drop_at(&mut self, paths: Vec<PathBuf>, target_id: Option<&str>) {
        self.dropped.push((paths, target_id.map(str::to_string)));
    }

    fn on_file_hover(&mut self, _path: PathBuf) {
        self.hovered += 1;
    }

    fn on_file_hover_cancelled(&mut self) {
        self.cancelled += 1;
    }
}

fn files() -> Vec<PathBuf> {
    vec![PathBuf::from("/tmp/a.txt"), PathBuf::from("/tmp/b.txt")]
}

#[test]
fn hovering_files_shows_the_drop_zone_under_the_cursor() {
    let mut h = Harness::new(Folders::default(), 400.0, 300.0);
    h.frame();
    assert_eq!(*h.app().seen.borrow(), None, "ドラッグが無ければ出ない");
    h.hover_files(&files(), 20.0, 20.0);
    h.frame();
    assert_eq!(h.app().hovered, 2, "on_file_hover は 1 ファイルずつ");
    assert_eq!(
        *h.app().seen.borrow(),
        Some((FILE_DRAG.to_string(), Some("docs".to_string())))
    );
    // drop zone でない所の上では出ない
    h.move_file_hover(20.0, 60.0);
    h.frame();
    assert_eq!(*h.app().seen.borrow(), Some((FILE_DRAG.to_string(), None)));
    h.cancel_file_hover();
    h.frame();
    assert_eq!(*h.app().seen.borrow(), None, "外れたら消える");
    assert_eq!(h.app().cancelled, 1);
}

#[test]
fn dropping_delivers_all_files_at_once_with_the_drop_zone() {
    let mut h = Harness::new(Folders::default(), 400.0, 300.0);
    h.frame();
    h.hover_files(&files(), 20.0, 20.0);
    h.drop_files(&files(), 20.0, 20.0);
    assert_eq!(h.app().dropped, vec![(files(), Some("docs".to_string()))]);
    h.frame();
    assert_eq!(*h.app().seen.borrow(), None, "落としたらドラッグは終わる");
}

#[test]
fn dropping_outside_any_drop_zone_has_no_target() {
    let mut h = Harness::new(Folders::default(), 400.0, 300.0);
    h.frame();
    h.drop_files(&files(), 20.0, 60.0);
    assert_eq!(h.app().dropped, vec![(files(), None)]);
}

/// `on_file_drop` だけを書いたアプリにも、1 回のドロップ分がまとめて 1 回で届く。
#[test]
fn on_file_drop_gets_one_call_per_drop() {
    #[derive(Default)]
    struct Plain {
        calls: Vec<Vec<PathBuf>>,
    }
    impl DeclarativeApp for Plain {
        fn view(&self, _ctx: &ViewContext) -> Element {
            div()
        }
        fn on_file_drop(&mut self, paths: Vec<PathBuf>) {
            self.calls.push(paths);
        }
    }
    let mut h = Harness::new(Plain::default(), 400.0, 300.0);
    h.frame();
    h.drop_files(&files(), 10.0, 10.0);
    assert_eq!(h.app().calls, vec![files()]);
}
