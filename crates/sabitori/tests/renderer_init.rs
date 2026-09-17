//! ウィンドウのレンダラー初期化が 1 か所であることを、ソースを読んで確かめる。
//!
//! # なぜソースを読むのか
//!
//! v0.11.2 まで、レンダラーの列挙は native (`resumed`) / wasm (`spawn_local`) /
//! 追加ウィンドウ (`spawn_extra`) の 3 か所に手で並んでいた。そして wasm の
//! 写しだけ `LineRenderer` を作っておらず、**web でだけ `polyline()` が 1 本も
//! 描かれなかった** (#66)。native と同じコードなのに、panic もログも出ない。
//!
//! 初期化には GPU が要るのでテストから走らせられない。走らせられないなら
//! せめて「2 か所に増えたこと」を落とす — 次にレンダラーを足す人が、
//! `init_renderers` に足せば全経路に届く、という形を壊せないようにする。

const SRC: &str = include_str!("../src/declarative.rs");

/// 各レンダラーを作る式はソース全体で 1 つだけ。増えたら、それは
/// `init_renderers` を通らない経路が生えたということ。
#[test]
fn every_renderer_is_constructed_in_exactly_one_place() {
    for name in ["TextRenderer", "ImageRenderer", "RingRenderer", "LineRenderer"] {
        let n = SRC.matches(&format!("{name}::new(")).count();
        assert_eq!(
            n, 1,
            "{name}::new( が {n} か所にある。init_renderers 1 本に寄せること \
             (経路ごとに写すと、wasm だけ抜ける #66 が再発する)"
        );
    }
}

/// その 1 か所は `init_renderers` の中。
#[test]
fn the_one_place_is_init_renderers() {
    let start = SRC
        .find("fn init_renderers")
        .expect("fn init_renderers が無い");
    let body = &SRC[start..];
    let end = body.find("\n}\n").expect("init_renderers の終端");
    let body = &body[..end];

    for name in ["TextRenderer", "ImageRenderer", "RingRenderer", "LineRenderer"] {
        assert!(
            body.contains(&format!("{name}::new(")),
            "init_renderers が {name} を作っていない"
        );
    }

    // フォント設定も同じ場所に置く。native だけ set_preferred_family を
    // 呼んでいて wasm と追加ウィンドウが呼んでいなかったのが #66 の後半。
    for call in [
        "prefer_user_fonts",
        "set_preferred_family",
        "set_preferred_monospace_family",
        "set_texture_budget_bytes",
    ] {
        assert!(body.contains(call), "init_renderers が {call} を呼んでいない");
    }

    // 初期化しか行わないものは 1 か所だけ。
    for call in ["prefer_user_fonts", "set_texture_budget_bytes"] {
        assert_eq!(
            SRC.matches(&format!("{call}(")).count(),
            1,
            "{call}( が複数ある。経路ごとに書くと、また 1 つ書き忘れる"
        );
    }
}

/// 2 つの font family は必ず同じ場所から呼ぶ。片方だけ毎フレーム更新されて
/// いたせいで、フォントピッカーで本文書体を変えても無視されていた。
#[test]
fn both_font_families_are_set_from_the_same_places() {
    let proportional = SRC.matches("set_preferred_family(").count();
    let monospace = SRC.matches("set_preferred_monospace_family(").count();
    assert_eq!(
        proportional, monospace,
        "本文 {proportional} 回 / 等幅 {monospace} 回 — 片方だけ呼ぶ場所がある"
    );
    assert!(proportional >= 2, "初期化と毎フレームの更新で最低 2 か所");
}
