//! 自由な形のパス ([`crate::element::path`])。
//!
//! 直線・2 次 / 3 次ベジェ・円弧でできた輪郭を、細かい折れ線に直す (`flatten`)。線は
//! その折れ線を線の描画器で描き、塗りは描く側 (bridge) が三角形に割って描く。
//! 「線が描かれていく」アニメ ([`trim`]) と点線 ([`dash`]) も、折れ線の長さで切る。
//!
//! SVG の `d` 属性 ([`parse_svg`]) も読める (`M L H V C S Q T A Z` と小文字の相対形)。

use crate::Color;

/// パスの 1 手。座標は要素の箱の左上からの論理 px (`view_box` があればその座標)。
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum PathCmd {
    MoveTo(f32, f32),
    LineTo(f32, f32),
    /// 制御点, 終点。
    QuadTo(f32, f32, f32, f32),
    /// 制御点 1, 制御点 2, 終点。
    CubicTo(f32, f32, f32, f32, f32, f32),
    /// 今の輪郭を始点へ閉じる。
    Close,
}

/// [`crate::element::path`] の中身。
#[derive(Clone, Debug, PartialEq)]
pub struct PathKind {
    pub cmds: Vec<PathCmd>,
    /// 塗りの色。`None` なら塗らない。
    pub fill: Option<Color>,
    /// 塗りの規則。`true` なら偶奇 (SVG の `evenodd`)、`false` なら非ゼロ (`nonzero`、既定)。
    pub even_odd: bool,
    /// 線の色。`None` なら線を引かない。
    pub stroke: Option<Color>,
    /// 線の太さ (論理 px)。
    pub stroke_width: f32,
    /// 線を引く範囲 (全長に対する割合、`0.0..=1.0`)。`(0.0, 0.4)` なら頭から 4 割だけ。
    pub trim: (f32, f32),
    /// 点線: (描く長さ, 空ける長さ) 論理 px。
    pub dash: Option<(f32, f32)>,
    /// パスの座標系 (x, y, 幅, 高さ)。あれば要素の箱いっぱいに引き伸ばす (SVG の
    /// `viewBox`、縦横比は保つ)。無ければ論理 px のまま。
    pub view_box: Option<(f32, f32, f32, f32)>,
}

impl Default for PathKind {
    fn default() -> Self {
        Self {
            cmds: Vec::new(),
            fill: None,
            even_odd: false,
            stroke: None,
            stroke_width: 1.5,
            trim: (0.0, 1.0),
            dash: None,
            view_box: None,
        }
    }
}

/// 折れ線に直した輪郭 1 本。
#[derive(Clone, Debug, PartialEq)]
pub struct Contour {
    pub points: Vec<(f32, f32)>,
    pub closed: bool,
}

/// 曲線を、`tolerance` (px) より離れない折れ線に直す。`map` で座標を変えてから測る
/// (画面での誤差で分割数を決めるため)。
pub fn flatten(cmds: &[PathCmd], tolerance: f32, map: impl Fn(f32, f32) -> (f32, f32)) -> Vec<Contour> {
    let tol = tolerance.max(0.01);
    let mut out: Vec<Contour> = Vec::new();
    let mut cur: Vec<(f32, f32)> = Vec::new();
    let mut start = (0.0, 0.0);
    let mut at = (0.0, 0.0);
    let finish = |cur: &mut Vec<(f32, f32)>, closed: bool, out: &mut Vec<Contour>| {
        if cur.len() >= 2 {
            out.push(Contour { points: std::mem::take(cur), closed });
        } else {
            cur.clear();
        }
    };
    for &c in cmds {
        match c {
            PathCmd::MoveTo(x, y) => {
                finish(&mut cur, false, &mut out);
                start = map(x, y);
                at = start;
                cur.push(at);
            }
            PathCmd::LineTo(x, y) => {
                if cur.is_empty() {
                    cur.push(at);
                }
                at = map(x, y);
                cur.push(at);
            }
            PathCmd::QuadTo(cx, cy, x, y) => {
                if cur.is_empty() {
                    cur.push(at);
                }
                let (p0, p1, p2) = (at, map(cx, cy), map(x, y));
                let n = segments(dist(p0, p1) + dist(p1, p2), tol);
                for i in 1..=n {
                    let t = i as f32 / n as f32;
                    let u = 1.0 - t;
                    cur.push((
                        u * u * p0.0 + 2.0 * u * t * p1.0 + t * t * p2.0,
                        u * u * p0.1 + 2.0 * u * t * p1.1 + t * t * p2.1,
                    ));
                }
                at = p2;
            }
            PathCmd::CubicTo(c1x, c1y, c2x, c2y, x, y) => {
                if cur.is_empty() {
                    cur.push(at);
                }
                let (p0, p1, p2, p3) = (at, map(c1x, c1y), map(c2x, c2y), map(x, y));
                let n = segments(dist(p0, p1) + dist(p1, p2) + dist(p2, p3), tol);
                for i in 1..=n {
                    let t = i as f32 / n as f32;
                    let u = 1.0 - t;
                    let (a, b, c, d) = (u * u * u, 3.0 * u * u * t, 3.0 * u * t * t, t * t * t);
                    cur.push((
                        a * p0.0 + b * p1.0 + c * p2.0 + d * p3.0,
                        a * p0.1 + b * p1.1 + c * p2.1 + d * p3.1,
                    ));
                }
                at = p3;
            }
            PathCmd::Close => {
                if cur.len() >= 2 && cur.first() != cur.last() {
                    cur.push(start);
                }
                finish(&mut cur, true, &mut out);
                at = start;
            }
        }
    }
    finish(&mut cur, false, &mut out);
    out
}

fn dist(a: (f32, f32), b: (f32, f32)) -> f32 {
    ((b.0 - a.0).powi(2) + (b.1 - a.1).powi(2)).sqrt()
}

/// 制御点の折れ線の長さから分割数を決める (長いほど細かく、誤差 `tol` 程度)。
fn segments(hull: f32, tol: f32) -> usize {
    ((hull / tol).sqrt() * 0.75).ceil().clamp(1.0, 256.0) as usize
}

/// 輪郭たちを、全長に対する割合 `start..end` の所だけ残した折れ線にする (つなぎ順は
/// 書いた順)。「線が描かれていく」アニメに使う。
pub fn trim(contours: &[Contour], start: f32, end: f32) -> Vec<Vec<(f32, f32)>> {
    let (start, end) = (start.clamp(0.0, 1.0), end.clamp(0.0, 1.0));
    if end <= start {
        return Vec::new();
    }
    let total: f32 = contours.iter().map(|c| length(&c.points)).sum();
    if total <= 0.0 {
        return Vec::new();
    }
    let (from, to) = (start * total, end * total);
    let mut out = Vec::new();
    let mut walked = 0.0;
    for c in contours {
        let len = length(&c.points);
        let (a, b) = ((from - walked).max(0.0), (to - walked).min(len));
        if b > a {
            let piece = cut(&c.points, a, b);
            if piece.len() >= 2 {
                out.push(piece);
            }
        }
        walked += len;
    }
    out
}

/// 折れ線を点線にする: `on` px 描いて `off` px 空ける、を繰り返す。
pub fn dash(line: &[(f32, f32)], on: f32, off: f32) -> Vec<Vec<(f32, f32)>> {
    let (on, off) = (on.max(0.01), off.max(0.0));
    let len = length(line);
    let mut out = Vec::new();
    let mut at = 0.0;
    while at < len {
        let piece = cut(line, at, (at + on).min(len));
        if piece.len() >= 2 {
            out.push(piece);
        }
        at += on + off;
    }
    out
}

fn length(p: &[(f32, f32)]) -> f32 {
    p.windows(2).map(|w| dist(w[0], w[1])).sum()
}

/// 折れ線の、頭から `a` px 〜 `b` px の部分。
fn cut(p: &[(f32, f32)], a: f32, b: f32) -> Vec<(f32, f32)> {
    let mut out = Vec::new();
    let mut walked = 0.0;
    for w in p.windows(2) {
        let l = dist(w[0], w[1]);
        let (s, e) = (walked, walked + l);
        if e > a && s < b && l > 0.0 {
            let lerp = |d: f32| {
                let t = ((d - s) / l).clamp(0.0, 1.0);
                (w[0].0 + (w[1].0 - w[0].0) * t, w[0].1 + (w[1].1 - w[0].1) * t)
            };
            if out.is_empty() {
                out.push(lerp(a.max(s)));
            }
            out.push(lerp(b.min(e)));
        }
        walked = e;
        if walked > b {
            break;
        }
    }
    out
}

/// SVG の `d` 属性を読む。読めない所で止め、そこまでの手と理由を返す。
pub fn parse_svg(d: &str) -> Result<Vec<PathCmd>, String> {
    let mut out = Vec::new();
    let mut toks = Tokens { s: d.as_bytes(), i: 0 };
    let (mut cx, mut cy) = (0.0f32, 0.0f32);
    let (mut sx, mut sy) = (0.0f32, 0.0f32);
    // 直前の曲線の制御点 (S / T の折り返し用)。
    let mut last_c2: Option<(f32, f32)> = None;
    let mut last_q: Option<(f32, f32)> = None;
    let mut cmd = b' ';
    loop {
        toks.skip_sep();
        if toks.done() {
            break;
        }
        if let Some(c) = toks.letter() {
            cmd = c;
        } else if cmd == b' ' {
            return Err(format!("{} 文字目: コマンドの前に数がある", toks.i + 1));
        }
        let rel = cmd.is_ascii_lowercase();
        let (ox, oy) = if rel { (cx, cy) } else { (0.0, 0.0) };
        let num = |t: &mut Tokens| t.number().ok_or_else(|| format!("{} 文字目: 数が読めない", t.i + 1));
        let (mut nc2, mut nq) = (None, None);
        match cmd.to_ascii_uppercase() {
            b'M' => {
                let (x, y) = (num(&mut toks)? + ox, num(&mut toks)? + oy);
                out.push(PathCmd::MoveTo(x, y));
                (cx, cy, sx, sy) = (x, y, x, y);
                // M の後に続く座標の組は L として読む。
                cmd = if rel { b'l' } else { b'L' };
            }
            b'L' => {
                let (x, y) = (num(&mut toks)? + ox, num(&mut toks)? + oy);
                out.push(PathCmd::LineTo(x, y));
                (cx, cy) = (x, y);
            }
            b'H' => {
                let x = num(&mut toks)? + ox;
                out.push(PathCmd::LineTo(x, cy));
                cx = x;
            }
            b'V' => {
                let y = num(&mut toks)? + oy;
                out.push(PathCmd::LineTo(cx, y));
                cy = y;
            }
            b'C' => {
                let v: Vec<f32> = (0..6).map(|_| num(&mut toks)).collect::<Result<_, _>>()?;
                let (c1, c2, p) = ((v[0] + ox, v[1] + oy), (v[2] + ox, v[3] + oy), (v[4] + ox, v[5] + oy));
                out.push(PathCmd::CubicTo(c1.0, c1.1, c2.0, c2.1, p.0, p.1));
                nc2 = Some(c2);
                (cx, cy) = p;
            }
            b'S' => {
                let v: Vec<f32> = (0..4).map(|_| num(&mut toks)).collect::<Result<_, _>>()?;
                let c1 = last_c2.map_or((cx, cy), |(x, y)| (2.0 * cx - x, 2.0 * cy - y));
                let (c2, p) = ((v[0] + ox, v[1] + oy), (v[2] + ox, v[3] + oy));
                out.push(PathCmd::CubicTo(c1.0, c1.1, c2.0, c2.1, p.0, p.1));
                nc2 = Some(c2);
                (cx, cy) = p;
            }
            b'Q' => {
                let v: Vec<f32> = (0..4).map(|_| num(&mut toks)).collect::<Result<_, _>>()?;
                let (c, p) = ((v[0] + ox, v[1] + oy), (v[2] + ox, v[3] + oy));
                out.push(PathCmd::QuadTo(c.0, c.1, p.0, p.1));
                nq = Some(c);
                (cx, cy) = p;
            }
            b'T' => {
                let p = (num(&mut toks)? + ox, num(&mut toks)? + oy);
                let c = last_q.map_or((cx, cy), |(x, y)| (2.0 * cx - x, 2.0 * cy - y));
                out.push(PathCmd::QuadTo(c.0, c.1, p.0, p.1));
                nq = Some(c);
                (cx, cy) = p;
            }
            b'A' => {
                let (rx, ry, rot) = (num(&mut toks)?, num(&mut toks)?, num(&mut toks)?);
                let large = toks.flag().ok_or("円弧の large-arc-flag が読めない")?;
                let sweep = toks.flag().ok_or("円弧の sweep-flag が読めない")?;
                let p = (num(&mut toks)? + ox, num(&mut toks)? + oy);
                arc_to_cubics((cx, cy), rx, ry, rot, large, sweep, p, &mut out);
                (cx, cy) = p;
            }
            b'Z' => {
                out.push(PathCmd::Close);
                (cx, cy) = (sx, sy);
                // Z の後に数は続かない。
                cmd = b' ';
            }
            other => return Err(format!("{} 文字目: 知らないコマンド `{}`", toks.i, other as char)),
        }
        last_c2 = nc2;
        last_q = nq;
    }
    Ok(out)
}

/// SVG の円弧 (端点で書く形) を 3 次ベジェに直す (SVG 1.1 付録 F.6)。
#[allow(clippy::too_many_arguments)]
fn arc_to_cubics(
    p0: (f32, f32),
    rx: f32,
    ry: f32,
    rot_deg: f32,
    large: bool,
    sweep: bool,
    p1: (f32, f32),
    out: &mut Vec<PathCmd>,
) {
    let (mut rx, mut ry) = (rx.abs(), ry.abs());
    if rx == 0.0 || ry == 0.0 || p0 == p1 {
        out.push(PathCmd::LineTo(p1.0, p1.1));
        return;
    }
    let phi = rot_deg.to_radians();
    let (sp, cp) = phi.sin_cos();
    let dx = (p0.0 - p1.0) * 0.5;
    let dy = (p0.1 - p1.1) * 0.5;
    let x1 = cp * dx + sp * dy;
    let y1 = -sp * dx + cp * dy;
    let lambda = (x1 * x1) / (rx * rx) + (y1 * y1) / (ry * ry);
    if lambda > 1.0 {
        let k = lambda.sqrt();
        rx *= k;
        ry *= k;
    }
    let num = (rx * rx * ry * ry - rx * rx * y1 * y1 - ry * ry * x1 * x1).max(0.0);
    let den = rx * rx * y1 * y1 + ry * ry * x1 * x1;
    let mut co = if den > 0.0 { (num / den).sqrt() } else { 0.0 };
    if large == sweep {
        co = -co;
    }
    let cx1 = co * rx * y1 / ry;
    let cy1 = -co * ry * x1 / rx;
    let cx = cp * cx1 - sp * cy1 + (p0.0 + p1.0) * 0.5;
    let cy = sp * cx1 + cp * cy1 + (p0.1 + p1.1) * 0.5;
    let angle = |ux: f32, uy: f32, vx: f32, vy: f32| {
        let a = (ux * vy - uy * vx).atan2(ux * vx + uy * vy);
        a
    };
    let ux = (x1 - cx1) / rx;
    let uy = (y1 - cy1) / ry;
    let vx = (-x1 - cx1) / rx;
    let vy = (-y1 - cy1) / ry;
    let theta1 = angle(1.0, 0.0, ux, uy);
    let mut dtheta = angle(ux, uy, vx, vy);
    if !sweep && dtheta > 0.0 {
        dtheta -= std::f32::consts::TAU;
    } else if sweep && dtheta < 0.0 {
        dtheta += std::f32::consts::TAU;
    }
    // 90° ごとに 1 本の 3 次ベジェで近似する。
    let n = (dtheta.abs() / std::f32::consts::FRAC_PI_2).ceil().max(1.0) as usize;
    let step = dtheta / n as f32;
    let k = 4.0 / 3.0 * (step / 4.0).tan();
    let point = |t: f32| {
        let (st, ct) = t.sin_cos();
        (cx + rx * ct * cp - ry * st * sp, cy + rx * ct * sp + ry * st * cp)
    };
    let deriv = |t: f32| {
        let (st, ct) = t.sin_cos();
        (-rx * st * cp - ry * ct * sp, -rx * st * sp + ry * ct * cp)
    };
    for i in 0..n {
        let (a, b) = (theta1 + step * i as f32, theta1 + step * (i + 1) as f32);
        let (pa, pb) = (point(a), point(b));
        let (da, db) = (deriv(a), deriv(b));
        let end = if i + 1 == n { p1 } else { pb };
        out.push(PathCmd::CubicTo(
            pa.0 + k * da.0,
            pa.1 + k * da.1,
            pb.0 - k * db.0,
            pb.1 - k * db.1,
            end.0,
            end.1,
        ));
    }
}

struct Tokens<'a> {
    s: &'a [u8],
    i: usize,
}

impl Tokens<'_> {
    fn done(&self) -> bool {
        self.i >= self.s.len()
    }

    fn skip_sep(&mut self) {
        while self.i < self.s.len() && (self.s[self.i].is_ascii_whitespace() || self.s[self.i] == b',') {
            self.i += 1;
        }
    }

    fn letter(&mut self) -> Option<u8> {
        self.skip_sep();
        let c = *self.s.get(self.i)?;
        if c.is_ascii_alphabetic() && c != b'e' && c != b'E' {
            self.i += 1;
            Some(c)
        } else {
            None
        }
    }

    /// 円弧の旗 (`0` / `1`)。区切り無しで続けて書いてよい (`a1 1 0 01 10 10`)。
    fn flag(&mut self) -> Option<bool> {
        self.skip_sep();
        let c = *self.s.get(self.i)?;
        self.i += 1;
        match c {
            b'0' => Some(false),
            b'1' => Some(true),
            _ => None,
        }
    }

    fn number(&mut self) -> Option<f32> {
        self.skip_sep();
        let start = self.i;
        let s = self.s;
        let mut i = self.i;
        if i < s.len() && (s[i] == b'+' || s[i] == b'-') {
            i += 1;
        }
        let mut dot = false;
        while i < s.len() && (s[i].is_ascii_digit() || (s[i] == b'.' && !dot)) {
            dot |= s[i] == b'.';
            i += 1;
        }
        if i < s.len() && (s[i] == b'e' || s[i] == b'E') {
            let mut j = i + 1;
            if j < s.len() && (s[j] == b'+' || s[j] == b'-') {
                j += 1;
            }
            if j < s.len() && s[j].is_ascii_digit() {
                while j < s.len() && s[j].is_ascii_digit() {
                    j += 1;
                }
                i = j;
            }
        }
        let v = std::str::from_utf8(&s[start..i]).ok()?.parse().ok()?;
        self.i = i;
        Some(v)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn id(x: f32, y: f32) -> (f32, f32) {
        (x, y)
    }

    #[test]
    fn svg_reads_absolute_relative_and_shorthands() {
        let cmds = parse_svg("M10,10 h20 v20 H10 Z m5 5 l1-1").unwrap();
        assert_eq!(
            cmds,
            [
                PathCmd::MoveTo(10.0, 10.0),
                PathCmd::LineTo(30.0, 10.0),
                PathCmd::LineTo(30.0, 30.0),
                PathCmd::LineTo(10.0, 30.0),
                PathCmd::Close,
                PathCmd::MoveTo(15.0, 15.0),
                PathCmd::LineTo(16.0, 14.0),
            ]
        );
        // M の後の座標の組は L、数の続け書き (`-1.5-2`, `.5.5`) も読む。
        let cmds = parse_svg("M0 0 10 0 l-1.5-2 .5.5").unwrap();
        assert_eq!(cmds[1], PathCmd::LineTo(10.0, 0.0));
        assert_eq!(cmds[2], PathCmd::LineTo(8.5, -2.0));
        assert_eq!(cmds[3], PathCmd::LineTo(9.0, -1.5));
        assert!(parse_svg("M0 0 X 1").is_err());
    }

    /// S / T は直前の制御点を折り返す。
    #[test]
    fn smooth_curves_reflect_the_previous_control_point() {
        let cmds = parse_svg("M0 0 C 0 10 10 10 10 0 S 20 -10 20 0").unwrap();
        assert_eq!(cmds[2], PathCmd::CubicTo(10.0, -10.0, 20.0, -10.0, 20.0, 0.0));
        let cmds = parse_svg("M0 0 Q 5 10 10 0 T 20 0").unwrap();
        assert_eq!(cmds[2], PathCmd::QuadTo(15.0, -10.0, 20.0, 0.0));
    }

    /// 半円の円弧は、真ん中で半径ぶん離れた所を通る。
    #[test]
    fn arcs_pass_through_the_right_points() {
        let cmds = parse_svg("M0 0 A 10 10 0 0 1 20 0").unwrap();
        let c = flatten(&cmds, 0.05, id);
        let pts = &c[0].points;
        assert_eq!(*pts.last().unwrap(), (20.0, 0.0));
        let top = pts.iter().map(|p| p.1).fold(f32::MAX, f32::min);
        // sweep=1 (時計回り、y 下向き) は上を回る。
        assert!((top + 10.0).abs() < 0.2, "半円の頂点が違う: {top}");
    }

    #[test]
    fn curves_flatten_within_tolerance() {
        // 4 分の 1 円 (半径 100) を 3 次で近似したものを、0.25px で折れ線にする。
        let k = 0.5523 * 100.0;
        let c = flatten(&[PathCmd::MoveTo(100.0, 0.0), PathCmd::CubicTo(100.0, k, k, 100.0, 0.0, 100.0)], 0.25, id);
        let pts = &c[0].points;
        assert!(pts.len() > 8, "粗すぎる: {}", pts.len());
        for w in pts.windows(2) {
            let mid = ((w[0].0 + w[1].0) * 0.5, (w[0].1 + w[1].1) * 0.5);
            let r = (mid.0 * mid.0 + mid.1 * mid.1).sqrt();
            assert!((r - 100.0).abs() < 0.5, "円から離れすぎ: {r}");
        }
    }

    #[test]
    fn trim_keeps_the_requested_share_of_the_length() {
        let c = flatten(&[PathCmd::MoveTo(0.0, 0.0), PathCmd::LineTo(100.0, 0.0), PathCmd::LineTo(100.0, 100.0)], 0.25, id);
        let half = trim(&c, 0.0, 0.5);
        assert_eq!(half, vec![vec![(0.0, 0.0), (100.0, 0.0)]]);
        let mid = trim(&c, 0.25, 0.75);
        assert_eq!(mid, vec![vec![(50.0, 0.0), (100.0, 0.0), (100.0, 50.0)]]);
        assert!(trim(&c, 0.5, 0.5).is_empty());
    }

    #[test]
    fn dash_alternates_on_and_off() {
        let d = dash(&[(0.0, 0.0), (100.0, 0.0)], 10.0, 15.0);
        assert_eq!(d.len(), 4);
        assert_eq!(d[0], vec![(0.0, 0.0), (10.0, 0.0)]);
        assert_eq!(d[1], vec![(25.0, 0.0), (35.0, 0.0)]);
    }
}
