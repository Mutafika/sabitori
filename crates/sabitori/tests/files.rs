//! ファイル選択の結果がアプリに届くまで (#77)。
//!
//! ダイアログそのものは OS / ブラウザのものなので、ヘッドレスからは開けない。
//! ここで固定するのは**その先** — 結果が届いて、アプリが読んで、画面が
//! 変わるところまで。ダイアログ自体は `e2e/web/probes/files.mjs` (web の
//! ダウンロード) と手での確認に任せる。

use sabitori::files::{self, PickOptions, PickResult, PickedFile};
use sabitori::testing::Harness;
use sabitori::*;

#[derive(Default)]
struct Restore {
    status: String,
    restored: Option<Vec<u8>>,
}

impl DeclarativeApp for Restore {
    fn view(&self, ctx: &ViewContext) -> Element {
        div().w_full().h_full().flex_col().children([
            div()
                .w(Px(120.0))
                .h(Px(32.0))
                .click(ctx, "restore", |_app: &mut Restore| {
                    files::pick("restore", PickOptions::new().accept([".db"]));
                }),
            text(self.status.clone()),
        ])
    }

    fn on_files_picked(&mut self, key: &str, result: PickResult) {
        if key != "restore" {
            return;
        }
        self.status = match result {
            PickResult::Picked(files) => {
                let f = &files[0];
                self.restored = Some(f.bytes.clone());
                format!("{} を読み込みました", f.name)
            }
            PickResult::Cancelled => "やめました".into(),
            // **「選べない」を「キャンセル」と同じにしない。** 一緒にすると、
            // Windows で押しても何も言わない画面になる。
            PickResult::Unsupported => "この環境では選べません".into(),
        };
    }
}

fn app() -> Harness<Restore> {
    let mut h = Harness::new(Restore::default(), 400.0, 300.0);
    h.frame();
    h
}

/// 選ばれた中身がアプリに届き、画面に出ること。
#[test]
fn a_picked_file_reaches_the_app() {
    let mut h = app();

    files::test_deliver(
        "restore",
        PickResult::Picked(vec![PickedFile::new("backup.db", b"SQLite format 3\0".to_vec())]),
    );
    h.frame();

    assert_eq!(h.app().restored.as_deref(), Some(&b"SQLite format 3\0"[..]));
    assert_eq!(h.app().status, "backup.db を読み込みました");
}

/// キャンセルと「この環境では選べない」が区別できること。
#[test]
fn cancelled_and_unsupported_are_different_answers() {
    let mut h = app();

    files::test_deliver("restore", PickResult::Cancelled);
    h.frame();
    assert_eq!(h.app().status, "やめました");

    files::test_deliver("restore", PickResult::Unsupported);
    h.frame();
    assert_eq!(h.app().status, "この環境では選べません");
}

/// 結果は 1 回だけ届くこと。2 回届くと、復元が二重に走る。
#[test]
fn a_result_is_delivered_once() {
    let mut h = app();

    files::test_deliver("restore", PickResult::Cancelled);
    h.frame();
    h.app_mut().status = "(まだ)".into();
    h.frame();

    assert_eq!(h.app().status, "(まだ)", "同じ結果が 2 回届いている");
}

/// 札 (`key`) が違う結果は無視されること。
#[test]
fn a_result_for_another_key_is_ignored() {
    let mut h = app();

    files::test_deliver("import-csv", PickResult::Cancelled);
    h.frame();

    assert_eq!(h.app().status, "");
}

/// 開いたファイルの場所が届き、⌘S でそこへ書き戻せること (csvmac は場所が無かったので、
/// 開いたファイルを保存するたびに保存先を聞いていた)。
#[test]
fn an_opened_file_carries_its_path_so_save_writes_back_there() {
    #[derive(Default)]
    struct Editor {
        path: Option<std::path::PathBuf>,
        text: String,
    }
    impl DeclarativeApp for Editor {
        fn view(&self, _ctx: &ViewContext) -> Element {
            div()
        }
        fn on_files_picked(&mut self, _key: &str, result: PickResult) {
            if let PickResult::Picked(files) = result {
                self.text = String::from_utf8_lossy(&files[0].bytes).into_owned();
                self.path = files[0].path.clone();
            }
        }
    }

    let dir = std::env::temp_dir().join(format!("sabitori-files-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("見積.csv");
    std::fs::write(&file, "a,b\n").unwrap();

    let mut h = Harness::new(Editor::default(), 200.0, 100.0);
    h.frame();
    files::test_deliver("open", PickResult::Picked(vec![PickedFile::new("見積.csv", b"a,b\n".to_vec()).with_path(&file)]));
    h.frame();
    assert_eq!(h.app().path.as_deref(), Some(file.as_path()));

    // ⌘S: 聞かずに、開いた場所へ書き戻す。
    h.app_mut().text.push_str("c,d\n");
    let (path, text) = (h.app().path.clone().unwrap(), h.app().text.clone());
    std::fs::write(&path, text).unwrap();
    assert_eq!(std::fs::read_to_string(&file).unwrap(), "a,b\nc,d\n");
    std::fs::remove_dir_all(&dir).ok();
}
