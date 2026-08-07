use sabitori::{
    Color, Corners, NodeId, NodeStyle, NodeTree, Point, Rect, RectInstance, SabitoriApp,
};

struct HelloApp {
    click_count: u32,
}

impl HelloApp {
    fn make_card_style(border_color: Color) -> NodeStyle {
        NodeStyle {
            fill_color: Color::from_hex("#22223a"),
            border_color,
            border_width: 1.0,
            corner_radii: Corners::all(12.0),
            shadow_color: Color::from_hex("#00000060"),
            shadow_offset: Point::new(0.0, 4.0),
            shadow_blur: 16.0,
            shadow_spread: 4.0,
        }
    }

    fn make_card_hover(border_color: Color) -> NodeStyle {
        NodeStyle {
            fill_color: Color::from_hex("#2a2a48"),
            border_color,
            border_width: 2.0,
            corner_radii: Corners::all(12.0),
            shadow_color: Color::from_hex("#00000080"),
            shadow_offset: Point::new(0.0, 8.0),
            shadow_blur: 24.0,
            shadow_spread: 6.0,
        }
    }

    fn make_card_active(border_color: Color) -> NodeStyle {
        NodeStyle {
            fill_color: Color::from_hex("#1e1e34"),
            border_color: border_color.lighten(0.2),
            border_width: 2.0,
            corner_radii: Corners::all(12.0),
            shadow_color: Color::from_hex("#00000040"),
            shadow_offset: Point::new(0.0, 2.0),
            shadow_blur: 8.0,
            shadow_spread: 2.0,
        }
    }

    fn make_button_style(color: Color) -> NodeStyle {
        NodeStyle {
            fill_color: color,
            border_color: Color::TRANSPARENT,
            border_width: 0.0,
            corner_radii: Corners::all(8.0),
            shadow_color: Color::from_hex("#00000040"),
            shadow_offset: Point::new(0.0, 2.0),
            shadow_blur: 6.0,
            shadow_spread: 0.0,
        }
    }

    fn make_button_hover(color: Color) -> NodeStyle {
        NodeStyle {
            fill_color: color.lighten(0.15),
            border_color: Color::TRANSPARENT,
            border_width: 0.0,
            corner_radii: Corners::all(8.0),
            shadow_color: Color::from_hex("#00000060"),
            shadow_offset: Point::new(0.0, 4.0),
            shadow_blur: 12.0,
            shadow_spread: 2.0,
        }
    }

    fn make_button_active(color: Color) -> NodeStyle {
        NodeStyle {
            fill_color: color.darken(0.1),
            border_color: Color::TRANSPARENT,
            border_width: 0.0,
            corner_radii: Corners::all(8.0),
            shadow_color: Color::from_hex("#00000020"),
            shadow_offset: Point::new(0.0, 1.0),
            shadow_blur: 3.0,
            shadow_spread: 0.0,
        }
    }
}

impl SabitoriApp for HelloApp {
    fn build(&self, tree: &mut NodeTree, width: f32, _height: f32) {
        let card_colors = [
            Color::from_hex("#6c63ff"),
            Color::from_hex("#e84393"),
            Color::from_hex("#00cec9"),
            Color::from_hex("#fdcb6e"),
            Color::from_hex("#6c5ce7"),
            Color::from_hex("#00b894"),
        ];

        let card_w = 280.0;
        let card_h = 180.0;
        let gap = 24.0;
        let start_x = (width - (card_w * 3.0 + gap * 2.0)) / 2.0;
        let start_y = 80.0;

        for i in 0..6 {
            let col = i % 3;
            let row = i / 3;
            let x = start_x + col as f32 * (card_w + gap);
            let y = start_y + row as f32 * (card_h + gap);
            let accent = card_colors[i];

            // Card (interactive)
            let border = accent.with_alpha(0.5);
            let card_id = tree.add(
                Rect::new(x, y, card_w, card_h),
                Self::make_card_style(border),
            );
            tree.set_hover_style(card_id, Self::make_card_hover(accent));
            tree.set_active_style(card_id, Self::make_card_active(accent));

            // Accent bar (non-interactive, child of card)
            tree.add_child(
                card_id,
                Rect::new(x, y, card_w, 4.0),
                NodeStyle {
                    fill_color: accent,
                    corner_radii: Corners::new(12.0, 12.0, 0.0, 0.0),
                    ..Default::default()
                },
            );

            // Circle icon
            tree.add_child(
                card_id,
                Rect::new(x + 24.0, y + 32.0, 40.0, 40.0),
                NodeStyle {
                    fill_color: accent.with_alpha(0.2),
                    border_color: accent,
                    border_width: 2.0,
                    corner_radii: Corners::all(20.0),
                    ..Default::default()
                },
            );

            // Text placeholders
            tree.add_child(
                card_id,
                Rect::new(x + 80.0, y + 36.0, 140.0, 12.0),
                NodeStyle {
                    fill_color: Color::from_hex("#e8e8f0").with_alpha(0.8),
                    corner_radii: Corners::all(4.0),
                    ..Default::default()
                },
            );
            tree.add_child(
                card_id,
                Rect::new(x + 80.0, y + 56.0, 100.0, 10.0),
                NodeStyle {
                    fill_color: Color::from_hex("#9090a8").with_alpha(0.5),
                    corner_radii: Corners::all(4.0),
                    ..Default::default()
                },
            );

            // Button (interactive)
            let btn_id = tree.add_child(
                card_id,
                Rect::new(x + card_w - 116.0, y + card_h - 52.0, 100.0, 36.0),
                Self::make_button_style(accent),
            );
            tree.set_hover_style(btn_id, Self::make_button_hover(accent));
            tree.set_active_style(btn_id, Self::make_button_active(accent));
            tree.set_on_click(btn_id, move || {
                tracing::info!("Button {i} clicked!");
            });
        }

        // Click counter display
        let counter_y = start_y + 2.0 * (card_h + gap) + 40.0;
        tree.add(
            Rect::new(
                (width - 200.0) / 2.0,
                counter_y,
                200.0,
                48.0,
            ),
            NodeStyle {
                fill_color: if self.click_count > 0 {
                    Color::from_hex("#6c63ff").with_alpha(0.3)
                } else {
                    Color::TRANSPARENT
                },
                border_color: Color::from_hex("#6c63ff").with_alpha(0.5),
                border_width: 1.0,
                corner_radii: Corners::all(24.0),
                ..Default::default()
            },
        );
    }

    fn render(&self, tree: &NodeTree) -> Vec<RectInstance> {
        let mut rects = Vec::new();

        // Background
        rects.push(RectInstance {
            rect: [0.0, 0.0, 2000.0, 2000.0],
            corner_radii: [0.0; 4],
            fill_color: Color::from_hex("#1a1a2e").to_array(),
            border_color: [0.0; 4],
            border_width: 0.0,
            gradient_angle: 0.0, rotation: 0.0, _pad0: 0.0, clip_rect: [0.0; 4],
            shadow_color: [0.0; 4],
            shadow_offset: [0.0; 2],
            shadow_params: [0.0; 2],
                        gradient_end_color: [0.0, 0.0, 0.0, 0.0],
        });

        // Render all nodes
        fn render_nodes(tree: &NodeTree, children: &[NodeId], rects: &mut Vec<RectInstance>) {
            for &id in children {
                if let Some(node) = tree.nodes.get(id) {
                    let style = &node.style;
                    rects.push(RectInstance {
                        rect: [
                            node.bounds.origin.x,
                            node.bounds.origin.y,
                            node.bounds.size.width,
                            node.bounds.size.height,
                        ],
                        corner_radii: style.corner_radii.to_array(),
                        fill_color: node.current_fill.to_array(),
                        border_color: node.current_border.to_array(),
                        border_width: style.border_width,
                        gradient_angle: 0.0, rotation: 0.0, _pad0: 0.0, clip_rect: [0.0; 4],
                        shadow_color: style.shadow_color.to_array(),
                        shadow_offset: [style.shadow_offset.x, style.shadow_offset.y],
                        shadow_params: [style.shadow_blur, style.shadow_spread],
                        gradient_end_color: [0.0, 0.0, 0.0, 0.0],
                    });
                    render_nodes(tree, &node.children, rects);
                }
            }
        }

        render_nodes(tree, &tree.root_children, &mut rects);
        rects
    }

    fn on_click(&mut self, _id: NodeId) {
        self.click_count += 1;
        tracing::info!("Total clicks: {}", self.click_count);
    }
}

fn main() {
    sabitori::run(HelloApp { click_count: 0 });
}
