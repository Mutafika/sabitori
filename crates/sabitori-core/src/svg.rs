//! SVG の絵を要素の木にする ([`Svg`])。
//!
//! 解釈は `usvg` に任せる (グループ・変形・CSS・`use`・単位などを、変形の付いた
//! パスの木にならしてくれる)。それを sabitori の [`path()`](crate::element::path) の
//! 要素に移す:
//!
//! - 変形は座標に焼き込み、全体を指定の幅に合わせて拡大する。
//! - `id` の付いたグループは、自分の範囲を箱にした `div` になる (中身はその中に
//!   置く)。グループ単位で回す・ずらす・薄くするができる ([`Svg::element_with`])。
//!   `id` の無いグループは親に溶かす (要素を増やさない)。
//! - 塗りと線は色。グラデーションは止めの色を混ぜた 1 色で近似する (パスの塗りが
//!   1 色だけなので)。模様 (pattern)、画像、文字は描かない。
//! - グループの不透明度は、中の色に掛ける (重なった所は二重に薄くなる)。

use usvg::{Group, Node, Paint};

use crate::element::{div, path, Element, Px};
use crate::path::PathCmd;
use crate::Color;

/// 読み込んだ SVG。[`Svg::element`] で要素にする。
pub struct Svg {
    tree: usvg::Tree,
}

/// [`Svg::element_with`] が渡す、部品 1 つの素性。
#[derive(Clone, Debug)]
pub struct SvgPart<'a> {
    /// その部品 (パスかグループ) の `id`。無ければ空。
    pub id: &'a str,
    /// グループなら `true`。
    pub is_group: bool,
    /// 祖先のグループの `id` (外から順、`id` の無いグループは入らない)。
    pub groups: &'a [String],
    /// 書いてある順の番号 (パスとグループを通して 0 から)。`id` の無い絵 (絵文字の
    /// 素材など) で、何番目の形かで手を加えるのに使う。
    pub index: usize,
}

impl SvgPart<'_> {
    /// 自分か祖先のグループが `id` か。
    pub fn is(&self, id: &str) -> bool {
        self.id == id || self.groups.iter().any(|g| g == id)
    }
}

impl Svg {
    /// SVG の文字列を読む。読めなければ理由を返す。
    pub fn parse(src: &str) -> Result<Self, String> {
        let tree = usvg::Tree::from_str(src, &usvg::Options::default()).map_err(|e| e.to_string())?;
        Ok(Self { tree })
    }

    /// 絵の元の大きさ (SVG の幅と高さ)。
    pub fn size(&self) -> (f32, f32) {
        let s = self.tree.size();
        (s.width(), s.height())
    }

    /// 幅 `width` (論理 px) の要素にする。高さは縦横比から決まる。
    pub fn element(&self, width: f32) -> Element {
        self.element_with(width, |_, e| e)
    }

    /// [`Svg::element`] に、部品ごとの手を加える。`f(部品, 要素)` が返した要素を置く。
    /// グループの要素は中身ごと動く (中心はグループの範囲の真ん中)。
    ///
    /// ```ignore
    /// svg.element_with(400.0, |part, e| if part.is("steam") { e.ty(-wave).opacity(0.8) } else { e })
    /// ```
    pub fn element_with(&self, width: f32, f: impl Fn(&SvgPart<'_>, Element) -> Element) -> Element {
        let (w, h) = self.size();
        let k = if w > 0.0 { width / w } else { 1.0 };
        let mut kids = Vec::new();
        let mut ids = Vec::new();
        let mut index = 0;
        collect(self.tree.root(), k, (0.0, 0.0), 1.0, &mut ids, &mut index, &f, &mut kids);
        div().w(Px(width)).h(Px(h * k)).children(kids)
    }
}

#[allow(clippy::too_many_arguments)]
/// `group` の中身を要素にして `out` へ。`origin` は置く先の箱の左上 (絵の座標 × k)。
fn collect(
    group: &Group,
    k: f32,
    origin: (f32, f32),
    opacity: f32,
    ids: &mut Vec<String>,
    index: &mut usize,
    f: &dyn Fn(&SvgPart<'_>, Element) -> Element,
    out: &mut Vec<Element>,
) {
    for node in group.children() {
        match node {
            Node::Group(g) => {
                let op = opacity * g.opacity().get();
                if g.id().is_empty() {
                    collect(g, k, origin, op, ids, index, f, out);
                    continue;
                }
                let my = *index;
                *index += 1;
                // id の付いたグループは自分の範囲を箱にする。
                let b = g.abs_bounding_box();
                let (bx, by) = (b.x() * k, b.y() * k);
                let mut inner = Vec::new();
                ids.push(g.id().to_string());
                collect(g, k, (bx, by), op, ids, index, f, &mut inner);
                ids.pop();
                let el = div()
                    .absolute()
                    .pos(bx - origin.0, by - origin.1)
                    .w(Px(b.width() * k))
                    .h(Px(b.height() * k))
                    .children(inner);
                out.push(f(&SvgPart { id: g.id(), is_group: true, groups: ids, index: my }, el));
            }
            Node::Path(p) => {
                if !p.is_visible() {
                    continue;
                }
                if let Some(el) = path_element(p, k, origin, opacity) {
                    out.push(f(&SvgPart { id: p.id(), is_group: false, groups: ids, index: *index }, el));
                }
                *index += 1;
            }
            Node::Image(_) | Node::Text(_) => {}
        }
    }
}

/// パス 1 本を、自分の範囲を箱にした `path()` 要素にする。
fn path_element(p: &usvg::Path, k: f32, origin: (f32, f32), opacity: f32) -> Option<Element> {
    let ts = p.abs_transform();
    let data = p.data().clone().transform(ts)?;
    let b = data.bounds();
    // 線の太さも変形に合わせる (縦横で違う変形は平均で近似)。
    let scale = (ts.sx * ts.sy - ts.kx * ts.ky).abs().sqrt();
    let stroke_w = p.stroke().map_or(0.0, |s| s.width().get() * scale * k);
    let pad = stroke_w * 0.5 + 1.0;
    let (bx, by) = (b.x() * k - pad, b.y() * k - pad);
    let (bw, bh) = (b.width() * k + pad * 2.0, b.height() * k + pad * 2.0);
    let mut e = path();
    for seg in data.segments() {
        use usvg::tiny_skia_path::PathSegment as S;
        let pt = |q: usvg::tiny_skia_path::Point| (q.x * k - bx, q.y * k - by);
        let cmd = match seg {
            S::MoveTo(a) => {
                let (x, y) = pt(a);
                PathCmd::MoveTo(x, y)
            }
            S::LineTo(a) => {
                let (x, y) = pt(a);
                PathCmd::LineTo(x, y)
            }
            S::QuadTo(c, a) => {
                let ((cx, cy), (x, y)) = (pt(c), pt(a));
                PathCmd::QuadTo(cx, cy, x, y)
            }
            S::CubicTo(c1, c2, a) => {
                let ((ax, ay), (bx2, by2), (x, y)) = (pt(c1), pt(c2), pt(a));
                PathCmd::CubicTo(ax, ay, bx2, by2, x, y)
            }
            S::Close => PathCmd::Close,
        };
        if let crate::element::ElementKind::Path(kind) = &mut e.kind {
            kind.cmds.push(cmd);
        }
    }
    if let Some(fill) = p.fill() {
        if let Some(c) = paint_color(fill.paint(), fill.opacity().get() * opacity) {
            e = e.fill(c);
            if fill.rule() == usvg::FillRule::EvenOdd {
                e = e.fill_even_odd();
            }
        }
    }
    if let Some(s) = p.stroke() {
        if let Some(c) = paint_color(s.paint(), s.opacity().get() * opacity) {
            e = e.stroke(c, stroke_w);
            if let Some(d) = s.dasharray().filter(|d| d.len() >= 2) {
                e = e.dash(d[0] * scale * k, d[1] * scale * k);
            }
        }
    }
    Some(e.absolute().pos(bx - origin.0, by - origin.1).w(Px(bw)).h(Px(bh)))
}

/// 塗りの色。グラデーションは止めの色を、間隔で重み付けして混ぜた 1 色にする。
fn paint_color(paint: &Paint, opacity: f32) -> Option<Color> {
    let srgb = |c: usvg::Color, a: f32| Color::from_srgb8(c.red, c.green, c.blue, 255).with_alpha(a);
    match paint {
        Paint::Color(c) => Some(srgb(*c, opacity)),
        Paint::LinearGradient(g) => mix_stops(g.stops(), opacity),
        Paint::RadialGradient(g) => mix_stops(g.stops(), opacity),
        Paint::Pattern(_) => None,
    }
}

fn mix_stops(stops: &[usvg::Stop], opacity: f32) -> Option<Color> {
    let first = stops.first()?;
    if stops.len() == 1 {
        let c = first.color();
        return Some(Color::from_srgb8(c.red, c.green, c.blue, 255).with_alpha(first.opacity().get() * opacity));
    }
    let (mut r, mut g, mut b, mut a, mut wsum) = (0.0f32, 0.0f32, 0.0f32, 0.0f32, 0.0f32);
    for w in stops.windows(2) {
        let span = (w[1].offset().get() - w[0].offset().get()).max(0.0);
        for s in w {
            let c = s.color();
            r += c.red as f32 * span * 0.5;
            g += c.green as f32 * span * 0.5;
            b += c.blue as f32 * span * 0.5;
            a += s.opacity().get() * span * 0.5;
        }
        wsum += span;
    }
    if wsum <= 0.0 {
        let c = first.color();
        return Some(Color::from_srgb8(c.red, c.green, c.blue, 255).with_alpha(first.opacity().get() * opacity));
    }
    let u8c = |v: f32| (v / wsum).round().clamp(0.0, 255.0) as u8;
    Some(Color::from_srgb8(u8c(r), u8c(g), u8c(b), 255).with_alpha(a / wsum * opacity))
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" width="100" height="50" viewBox="0 0 100 50">
        <rect x="10" y="10" width="20" height="20" fill="#ff0000"/>
        <g id="moved" transform="translate(50 0)">
            <circle cx="20" cy="20" r="10" fill="#0000ff"/>
        </g>
        <g opacity="0.5"><rect x="0" y="40" width="10" height="10" fill="#00ff00"/></g>
    </svg>"##;

    #[test]
    fn size_and_parts() {
        let svg = Svg::parse(SAMPLE).unwrap();
        assert_eq!(svg.size(), (100.0, 50.0));
        let seen = std::cell::RefCell::new(Vec::new());
        let el = svg.element_with(200.0, |p, e| {
            seen.borrow_mut().push((p.id.to_string(), p.is_group, p.is("moved")));
            e
        });
        let seen = seen.into_inner();
        // 赤の四角・(id の付いた) グループの中の円・グループ・(溶かした) 緑の四角。
        assert_eq!(seen.len(), 4, "{seen:?}");
        assert!(seen.iter().any(|(id, g, _)| id == "moved" && *g));
        assert_eq!(seen.iter().filter(|(_, _, in_moved)| *in_moved).count(), 2, "{seen:?}");
        assert_eq!(el.children.len(), 3);
    }

    #[test]
    fn gradients_become_their_mixed_colour() {
        let g = r##"<svg xmlns="http://www.w3.org/2000/svg" width="10" height="10">
            <defs><linearGradient id="g"><stop offset="0" stop-color="#000000"/><stop offset="1" stop-color="#ffffff"/></linearGradient></defs>
            <rect width="10" height="10" fill="url(#g)"/></svg>"##;
        let svg = Svg::parse(g).unwrap();
        let el = svg.element(10.0);
        let crate::element::ElementKind::Path(p) = &el.children[0].kind else { panic!("パスでない") };
        let c = p.fill.unwrap();
        let mid = Color::from_srgb8(128, 128, 128, 255);
        assert!((c.r - mid.r).abs() < 0.02, "{c:?}");
    }

    #[test]
    fn broken_svg_is_an_error() {
        assert!(Svg::parse("<svg").is_err());
    }
}
