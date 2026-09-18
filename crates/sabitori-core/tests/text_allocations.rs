//! **text 要素 1 個あたりの確保回数**を数える ([#80])。
//!
//! 端末や表のように `text()` が数百〜千個ある画面では、シェーピングも
//! アトラスも効いたあとに**ここが支配的**になる。数えていないと、
//! 「1 要素につき 1 個増やす」変更が入ったことに誰も気づけない。
//!
//! 数え方はプロセス全体のアロケータを差し替えるだけ。テストはこのファイルに
//! **1 本だけ**置く — 並走するテストの確保が混ざると数が揺れるため。
//!
//! [#80]: https://github.com/Mutafika/sabitori/issues/80

use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicUsize, Ordering};

static ALLOCS: AtomicUsize = AtomicUsize::new(0);

struct Counting;

unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        ALLOCS.fetch_add(1, Ordering::Relaxed);
        unsafe { System.alloc(layout) }
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        unsafe { System.dealloc(ptr, layout) }
    }
    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        ALLOCS.fetch_add(1, Ordering::Relaxed);
        unsafe { System.realloc(ptr, layout, new_size) }
    }
}

#[global_allocator]
static A: Counting = Counting;

use sabitori_core::build::build_tree;
use sabitori_core::element::{div, text, Element, Px};

/// 端末の 1 画面ぶん (80×24 のうち半分が非空) に近い形。
fn grid(cells: usize) -> Element {
    let children: Vec<Element> = (0..cells)
        .map(|i| {
            text(format!("{:02}", i % 100))
                .font_size(14.0)
                .mono()
                .w(Px(10.0))
                .h(Px(18.0))
        })
        .collect();
    div().w(Px(800.0)).h(Px(600.0)).flex_wrap(sabitori_core::FlexWrap::Wrap).children(children)
}

/// **1 要素あたりの確保回数に上限を置く。**
///
/// 数字そのものより「増えたら落ちる」ことが目的。上限を上げるときは、
/// 何と引き換えに増やしたのかをコミットに書くこと。
///
/// 実測 (960 要素):
///
/// | | 確保 / 要素 |
/// |---|---|
/// | #80 の前 (`content` が `String` で 2 回 clone) | **2.06** |
/// | いま (`Arc<str>` を参照カウントで持ち回す) | **0.06** |
#[test]
fn building_a_text_grid_stays_within_its_allocation_budget() {
    let cells = 960;
    let tree = grid(cells);

    // ツリーの構築ぶんは数えない (毎フレーム組み直すのは view() の仕事で、
    // ここで見たいのは build_tree の中)。1 回空回しして、遅延初期化を済ませる。
    let _ = build_tree(&tree, 800.0, 600.0);

    let before = ALLOCS.load(Ordering::Relaxed);
    let built = build_tree(&tree, 800.0, 600.0);
    let after = ALLOCS.load(Ordering::Relaxed);

    let per_element = (after - before) as f64 / cells as f64;
    eprintln!("build_tree: {} 確保 / {cells} 要素 = {per_element:.2} 確保/要素", after - before);

    // 落ちたときに何を見ればいいかが分かるよう、中身も出す。
    assert!(built.render_list.commands.len() >= cells, "文字が出ていない");
    assert!(
        per_element <= 0.5,
        "1 要素あたりの確保が増えている: {per_element:.2} (上限 0.5)"
    );
}
