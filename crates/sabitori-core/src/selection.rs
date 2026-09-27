//! **選ばれている文字の範囲をアプリが読む口**
//! ([#107](https://github.com/Mutafika/sabitori/issues/107))。
//!
//! 文字の選択 (ドラッグで選び、⌘C で写す) はランタイムが持っている。ランタイムの
//! 中では「描画の順で何番目の文字か + バイト位置」で覚えているが、その番号は
//! フレームごとに変わりうるので、アプリは保存できない。ここでは、アプリが
//! **安定した位置に戻せる形**にして渡す:
//!
//! - どの段落か: id の付いたいちばん近い祖先の id ([`SelectedPiece::owner`])
//! - 段落の中身: その文字要素の全文 ([`SelectedPiece::content`])
//! - どこからどこまでか: 全文の中のバイト範囲 ([`SelectedPiece::range`])
//!
//! 読むのは `ctx.text_selection()`、変わったことを知るのは
//! `DeclarativeApp::on_selection_changed`。付けたマーカーは既存の
//! [`HighlightSpec`](crate::HighlightSpec) で描ける。

use std::ops::Range;
use std::sync::Arc;

use crate::Rect;

/// 選択範囲のうち、1 つの文字要素に掛かっている部分。
#[derive(Clone, Debug, PartialEq)]
pub struct SelectedPiece {
    /// id の付いたいちばん近い祖先 (自分を含む) の id。無ければ `None`。
    ///
    /// マーカーを保存するときの「どの段落か」。法令なら `art@第709条` のように、
    /// 段落の要素に id を付けておく。
    pub owner: Option<Arc<str>>,
    /// この文字要素の全文 (`text(..)` に渡したもの)。1 つの id の下に文字要素が
    /// いくつもあるとき、どれかを見分ける材料にもなる。
    pub content: Arc<str>,
    /// `content` の中で選ばれているバイト範囲 (文字の境目に揃っている)。
    pub range: Range<usize>,
    /// 選ばれている字の画面上の外接矩形 (論理 px)。選択の近くに自前のメニューを
    /// 出すのに使う。
    pub rect: Rect,
}

impl SelectedPiece {
    /// 選ばれている部分の文字列。
    pub fn text(&self) -> &str {
        &self.content[self.range.clone()]
    }
}

/// 選ばれている文字の範囲。文字要素をまたぐと、要素ごとの [`SelectedPiece`] が
/// 読む順 (描いた順) に並ぶ。
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SelectedText {
    pub pieces: Vec<SelectedPiece>,
    /// ⌘C で写るのと同じ文字列 (要素の間は、見た目の行が変われば改行)。
    pub text: String,
}

impl SelectedText {
    /// 全体の画面上の外接矩形。
    pub fn bounds(&self) -> Option<Rect> {
        self.pieces.iter().map(|p| p.rect).reduce(|a, b| {
            let x0 = a.origin.x.min(b.origin.x);
            let y0 = a.origin.y.min(b.origin.y);
            let x1 = (a.origin.x + a.size.width).max(b.origin.x + b.size.width);
            let y1 = (a.origin.y + a.size.height).max(b.origin.y + b.size.height);
            Rect::new(x0, y0, x1 - x0, y1 - y0)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn piece(owner: &str, content: &str, range: Range<usize>, rect: Rect) -> SelectedPiece {
        SelectedPiece { owner: Some(owner.into()), content: content.into(), range, rect }
    }

    #[test]
    fn bounds_cover_every_piece() {
        let sel = SelectedText {
            pieces: vec![
                piece("a", "第一項", 3..9, Rect::new(40.0, 10.0, 60.0, 20.0)),
                piece("b", "第二項", 0..3, Rect::new(10.0, 40.0, 20.0, 20.0)),
            ],
            text: String::new(),
        };
        assert_eq!(sel.bounds(), Some(Rect::new(10.0, 10.0, 90.0, 50.0)));
        assert_eq!(sel.pieces[0].text(), "一項");
        assert_eq!(SelectedText::default().bounds(), None);
    }
}
