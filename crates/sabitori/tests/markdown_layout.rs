//! **Markdown の塊が、横並びの文字の 1 行の既定 (#109) で切れない**。
//!
//! コードブロック・引用・箇条書き・見出しは横並びの入れ物に文字を置いていたので、
//! 1 行の「…」の既定に当たって、複数行のコードが 1 行目だけになった。

use sabitori::testing::Harness;
use sabitori::*;
use sabitori_core::element::{div, Px};
use sabitori_core::render_list::RenderCommand;
use sabitori_markdown::{render_markdown, MarkdownOptions};

const LONG: &str = "This paragraph is long enough that it has to wrap onto several lines inside a narrow column of text.";

struct Doc;

impl DeclarativeApp for Doc {
    fn view(&self, _ctx: &ViewContext) -> Element {
        let md = format!(
            "# {LONG}\n\n```\nfn main() {{\n    println!(\"hi\");\n}}\n```\n\n> {LONG}\n\n- {LONG}\n"
        );
        div().w(Px(260.0)).flex_col().child(render_markdown(&md, &MarkdownOptions::default()))
    }
}

#[test]
fn no_markdown_block_is_cut_to_one_line() {
    let mut h = Harness::with_real_text(Doc, 260.0, 1200.0);
    h.settle();
    let texts: Vec<_> = h
        .build()
        .render_list
        .commands
        .iter()
        .filter_map(|c| match c {
            RenderCommand::Text(t) => Some((t.content.to_string(), t.max_lines, t.max_height)),
            _ => None,
        })
        .collect();
    let code = texts.iter().find(|t| t.0.contains("println")).expect("コードが無い");
    assert_eq!(code.1, None, "コードが 1 行に切れる");
    let blocks: Vec<_> = texts.iter().filter(|t| t.0 == LONG).collect();
    assert_eq!(blocks.len(), 3, "見出し・引用・箇条書き");
    for b in blocks {
        assert_eq!(b.1, None, "1 行に切れる");
        assert!(b.2 > 40.0, "折り返していない: {}", b.2);
    }
    assert!(h.overflows().is_empty(), "{:?}", h.overflows());
}
