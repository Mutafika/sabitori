//! 矩形と goo を「1 枚ぶん上げて、範囲で描く」描画先。
//!
//! 木の順に描く経路 (`sabitori::bridge::OrderedFrame`) は、矩形・goo・画像・文字…を
//! 書いた順に、同じ種類が続く所だけまとめて描く。描画器ごとに 1 枚ぶんを先に全部
//! 上げ (`queue.write_buffer` は 1 回の提出につき最後の書き込みしか効かない)、
//! 描くときは範囲で指す。画像・リング・線・文字の描画器はそれぞれ自分で持つが、
//! 矩形と goo は窓の描画器 ([`GpuRenderer`](crate::GpuRenderer)) と、窓を持たない
//! 描画器 ([`UiOverlayRenderer`](crate::UiOverlayRenderer)) の 2 つが持っている。
//! この trait で両方を同じ形にする。

use crate::instance::{GooInstance, RectInstance};

pub trait RectTarget {
    /// 1 枚ぶんの矩形を上げる。パスを開く前に呼ぶ。
    fn upload_rect_instances(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, rects: &[RectInstance]);
    /// 1 枚ぶんの goo を上げる。パスを開く前に呼ぶ。
    fn upload_goo_instances(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, goo: &[GooInstance]);
    /// 上げた矩形の `range` を描く。
    fn draw_rect_range(&self, pass: &mut wgpu::RenderPass<'_>, range: std::ops::Range<u32>);
    /// 上げた goo の `range` を描く。
    fn draw_goo_range(&self, pass: &mut wgpu::RenderPass<'_>, range: std::ops::Range<u32>);
    /// rect.wgsl 系のシェーダーが共通で使う globals。画像・文字の描画器にも渡す。
    fn globals(&self) -> &wgpu::BindGroup;
    /// 描き先の大きさ (物理 px)。グループ (先に別の絵に描く中身) の描き先をこれに合わせる。
    fn target_size(&self) -> (u32, u32);
}
