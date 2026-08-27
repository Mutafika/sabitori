//! Element のサイズ回帰テスト (#56)。
//!
//! Element はビルダー式の入れ子 1 段ごとにスタックへ値渡しで積まれる。かつて
//! ElementStyle (536B) と hover/active の StateStyle (220B×2) をインラインで抱えて
//! 1224B あり、wasm32 の既定スタック 1MB を view() 構築だけで食い潰して
//! 「RuntimeError: memory access out of bounds」で即死していた (native は 8MB で無症状)。
//! 太いフィールドを Box 化して 272B に落とした。ここで上限を張り、フィールド追加で
//! 音もなく太るのを止める。

use sabitori_core::element::Element;

#[test]
fn element_stays_small_enough_for_wasm_stacks() {
    let size = std::mem::size_of::<Element>();
    println!("size_of::<Element>() = {size} B");
    assert!(
        size <= 320,
        "Element が {size}B に太った (上限 320B)。インラインの大きいフィールドは \
         Box に置く (#56 — wasm32 の 1MB スタックを view() 構築が食い潰す)"
    );
}
