//! `hot_reload` シムの契約。
//!
//! ホットパッチそのもの (dx が撃つ jump table の適用) はここでは検証できない。
//! 検証するのは、feature を on/off どちらにしても **アプリの振る舞いが変わらない**
//! こと — つまりホットリロードを切った本番ビルドが、有効なときと同じ結果を返す
//! こと。ここが崩れると「開発中は動くのに release で挙動が違う」という最悪の
//! バグクラスが生まれる。

use sabitori::hot_reload;

#[test]
fn call_returns_the_closure_value() {
    assert_eq!(hot_reload::call(|| 40 + 2), 42);
}

#[test]
fn call_is_transparent_to_captured_state() {
    // `view` は `&self` を借りて Element を組む。シムがその借用を邪魔しないこと。
    let label = String::from("sabitori");
    let out = hot_reload::call(|| format!("{label}!"));
    assert_eq!(out, "sabitori!");
}

#[test]
fn call_runs_the_closure_exactly_once() {
    // `FnMut` を取るので、うっかり複数回呼ぶ実装だと `view` の副作用が二重になる。
    let mut calls = 0;
    hot_reload::call(|| calls += 1);
    assert_eq!(calls, 1);
}

#[test]
fn call_nests() {
    // overlay は view の内側で組まれることがある。境界の入れ子で死なないこと。
    assert_eq!(hot_reload::call(|| hot_reload::call(|| 7) + 1), 8);
}

#[test]
fn init_without_a_devserver_is_a_noop() {
    // `cargo run` で普通に起動した場合。devserver は居ないので、接続を諦めて
    // 静かに帰るだけで、パニックもブロックもしてはいけない。
    hot_reload::init(|| unreachable!("パッチが来ていないのにハンドラが呼ばれた"));
}
