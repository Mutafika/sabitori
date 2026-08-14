use sabitori::{
    AlignItems, Color, Corners, Dimension, DimensionExt, Display, EdgeDimensions,
    FlexDirection, FlexWrap, JustifyContent, LayoutEngine, LayoutNodeId, NodeId, NodeStyle,
    NodeTree, Overflow, Point, Rect, RectInstance, SabitoriApp, StyleProps, Theme,
};

/// Demonstrates Taffy layout: a header, scrollable card grid, and status bar.
struct LayoutApp {
    theme: Theme,
}

struct LayoutNode {
    taffy_id: LayoutNodeId,
    style_props: StyleProps,
    visual: VisualStyle,
    hover_visual: Option<VisualStyle>,
    active_visual: Option<VisualStyle>,
    interactive: bool,
    children: Vec<usize>, // indices into the flat list
}

#[derive(Clone)]
struct VisualStyle {
    fill: Color,
    border_color: Color,
    border_width: f32,
    corner_radius: Corners<f32>,
    shadow_color: Color,
    shadow_offset: Point,
    shadow_blur: f32,
    shadow_spread: f32,
}

impl Default for VisualStyle {
    fn default() -> Self {
        Self {
            fill: Color::TRANSPARENT,
            border_color: Color::TRANSPARENT,
            border_width: 0.0,
            corner_radius: Corners::all(0.0),
            shadow_color: Color::TRANSPARENT,
            shadow_offset: Point::ZERO,
            shadow_blur: 0.0,
            shadow_spread: 0.0,
        }
    }
}

impl SabitoriApp for LayoutApp {
    fn build(&self, tree: &mut NodeTree, width: f32, height: f32) {
        let t = &self.theme;
        let mut engine = LayoutEngine::new();
        let mut nodes: Vec<LayoutNode> = Vec::new();

        // Helper to add a layout node
        macro_rules! leaf {
            ($style:expr, $visual:expr) => {{
                let taffy_id = engine.add_leaf(&$style);
                let idx = nodes.len();
                nodes.push(LayoutNode {
                    taffy_id,
                    style_props: $style,
                    visual: $visual,
                    hover_visual: None,
                    active_visual: None,
                    interactive: false,
                    children: vec![],
                });
                idx
            }};
        }

        // --- Header ---
        let header = leaf!(
            StyleProps {
                width: Dimension::Px(width),
                height: Dimension::Px(52.0),
                padding: EdgeDimensions::axes(0.0.px(), 20.0.px()),
                align_items: AlignItems::Center,
                ..Default::default()
            },
            VisualStyle {
                fill: t.surface_elevated,
                shadow_color: t.shadow,
                shadow_offset: Point::new(0.0, 2.0),
                shadow_blur: 8.0,
                ..Default::default()
            }
        );

        // Header title placeholder
        let title = leaf!(
            StyleProps {
                width: Dimension::Px(120.0),
                height: Dimension::Px(16.0),
                ..Default::default()
            },
            VisualStyle {
                fill: t.text_primary.with_alpha(0.8),
                corner_radius: Corners::all(4.0),
                ..Default::default()
            }
        );
        engine.add_child(nodes[header].taffy_id, nodes[title].taffy_id);
        nodes[header].children.push(title);

        // --- Card Grid Area ---
        let card_area = leaf!(
            StyleProps {
                flex_grow: 1.0,
                width: Dimension::Px(width),
                padding: EdgeDimensions::all(24.0.px()),
                flex_direction: FlexDirection::Row,
                flex_wrap: FlexWrap::Wrap,
                gap: 20.0,
                align_items: AlignItems::Start,
                justify_content: JustifyContent::Center,
                display: Display::Flex,
                overflow: Overflow::Hidden,
                ..Default::default()
            },
            VisualStyle {
                fill: t.surface,
                ..Default::default()
            }
        );

        let accent_colors = [
            Color::from_hex("#6c63ff"),
            Color::from_hex("#e84393"),
            Color::from_hex("#00cec9"),
            Color::from_hex("#fdcb6e"),
            Color::from_hex("#6c5ce7"),
            Color::from_hex("#00b894"),
            Color::from_hex("#fd79a8"),
            Color::from_hex("#0984e3"),
        ];

        // Create cards
        let mut card_indices = Vec::new();
        for i in 0..8 {
            let accent = accent_colors[i % accent_colors.len()];

            let card = leaf!(
                StyleProps {
                    width: Dimension::Px(200.0),
                    height: Dimension::Px(260.0),
                    flex_direction: FlexDirection::Column,
                    padding: EdgeDimensions::all(0.0.px()),
                    ..Default::default()
                },
                VisualStyle {
                    fill: t.surface_elevated,
                    border_color: t.border,
                    border_width: 1.0,
                    corner_radius: Corners::all(12.0),
                    shadow_color: t.shadow,
                    shadow_offset: Point::new(0.0, 4.0),
                    shadow_blur: 16.0,
                    shadow_spread: 2.0,
                    ..Default::default()
                }
            );
            nodes[card].interactive = true;
            nodes[card].hover_visual = Some(VisualStyle {
                fill: t.surface_hover,
                border_color: accent.with_alpha(0.6),
                border_width: 2.0,
                corner_radius: Corners::all(12.0),
                shadow_color: Color::from_hex("#00000080"),
                shadow_offset: Point::new(0.0, 8.0),
                shadow_blur: 24.0,
                shadow_spread: 4.0,
            });
            nodes[card].active_visual = Some(VisualStyle {
                fill: t.surface_active,
                border_color: accent,
                border_width: 2.0,
                corner_radius: Corners::all(12.0),
                shadow_color: Color::from_hex("#00000040"),
                shadow_offset: Point::new(0.0, 2.0),
                shadow_blur: 6.0,
                shadow_spread: 0.0,
            });

            // Accent top bar
            let bar = leaf!(
                StyleProps {
                    width: 100.0.pct(),
                    height: Dimension::Px(4.0),
                    ..Default::default()
                },
                VisualStyle {
                    fill: accent,
                    corner_radius: Corners::new(12.0, 12.0, 0.0, 0.0),
                    ..Default::default()
                }
            );
            engine.add_child(nodes[card].taffy_id, nodes[bar].taffy_id);
            nodes[card].children.push(bar);

            // Card content area
            let content = leaf!(
                StyleProps {
                    flex_grow: 1.0,
                    flex_direction: FlexDirection::Column,
                    padding: EdgeDimensions::all(16.0.px()),
                    gap: 8.0,
                    ..Default::default()
                },
                VisualStyle::default()
            );

            // Circle icon
            let circle = leaf!(
                StyleProps {
                    width: Dimension::Px(36.0),
                    height: Dimension::Px(36.0),
                    ..Default::default()
                },
                VisualStyle {
                    fill: accent.with_alpha(0.2),
                    border_color: accent,
                    border_width: 2.0,
                    corner_radius: Corners::all(18.0),
                    ..Default::default()
                }
            );
            engine.add_child(nodes[content].taffy_id, nodes[circle].taffy_id);
            nodes[content].children.push(circle);

            // Title line
            let line1 = leaf!(
                StyleProps {
                    width: Dimension::Px(140.0),
                    height: Dimension::Px(12.0),
                    ..Default::default()
                },
                VisualStyle {
                    fill: t.text_primary.with_alpha(0.7),
                    corner_radius: Corners::all(4.0),
                    ..Default::default()
                }
            );
            engine.add_child(nodes[content].taffy_id, nodes[line1].taffy_id);
            nodes[content].children.push(line1);

            // Subtitle line
            let line2 = leaf!(
                StyleProps {
                    width: Dimension::Px(100.0),
                    height: Dimension::Px(10.0),
                    ..Default::default()
                },
                VisualStyle {
                    fill: t.text_secondary.with_alpha(0.5),
                    corner_radius: Corners::all(4.0),
                    ..Default::default()
                }
            );
            engine.add_child(nodes[content].taffy_id, nodes[line2].taffy_id);
            nodes[content].children.push(line2);

            // Spacer
            let spacer = leaf!(
                StyleProps {
                    flex_grow: 1.0,
                    ..Default::default()
                },
                VisualStyle::default()
            );
            engine.add_child(nodes[content].taffy_id, nodes[spacer].taffy_id);
            nodes[content].children.push(spacer);

            // Button
            let btn = leaf!(
                StyleProps {
                    width: Dimension::Px(80.0),
                    height: Dimension::Px(32.0),
                    ..Default::default()
                },
                VisualStyle {
                    fill: accent,
                    corner_radius: Corners::all(8.0),
                    shadow_color: Color::from_hex("#00000040"),
                    shadow_offset: Point::new(0.0, 2.0),
                    shadow_blur: 6.0,
                    ..Default::default()
                }
            );
            nodes[btn].interactive = true;
            nodes[btn].hover_visual = Some(VisualStyle {
                fill: accent.lighten(0.15),
                corner_radius: Corners::all(8.0),
                shadow_color: Color::from_hex("#00000060"),
                shadow_offset: Point::new(0.0, 4.0),
                shadow_blur: 10.0,
                ..Default::default()
            });
            engine.add_child(nodes[content].taffy_id, nodes[btn].taffy_id);
            nodes[content].children.push(btn);

            engine.add_child(nodes[card].taffy_id, nodes[content].taffy_id);
            nodes[card].children.push(content);

            engine.add_child(nodes[card_area].taffy_id, nodes[card].taffy_id);
            nodes[card_area].children.push(card);
            card_indices.push(card);
        }

        // --- Status Bar ---
        let status = leaf!(
            StyleProps {
                width: Dimension::Px(width),
                height: Dimension::Px(32.0),
                padding: EdgeDimensions::axes(0.0.px(), 16.0.px()),
                align_items: AlignItems::Center,
                ..Default::default()
            },
            VisualStyle {
                fill: t.surface_elevated,
                border_color: t.border,
                border_width: 1.0,
                ..Default::default()
            }
        );

        let status_text = leaf!(
            StyleProps {
                width: Dimension::Px(80.0),
                height: Dimension::Px(8.0),
                ..Default::default()
            },
            VisualStyle {
                fill: t.text_secondary.with_alpha(0.5),
                corner_radius: Corners::all(4.0),
                ..Default::default()
            }
        );
        engine.add_child(nodes[status].taffy_id, nodes[status_text].taffy_id);
        nodes[status].children.push(status_text);

        // --- Root container ---
        let root_style = StyleProps {
            width: Dimension::Px(width),
            height: Dimension::Px(height),
            flex_direction: FlexDirection::Column,
            ..Default::default()
        };
        let root = engine.add_with_children(
            &root_style,
            &[
                nodes[header].taffy_id,
                nodes[card_area].taffy_id,
                nodes[status].taffy_id,
            ],
        );

        // Compute layout
        engine.compute(root, width, height);

        // Convert to scene tree with absolute positions
        fn add_to_tree(
            engine: &LayoutEngine,
            nodes: &[LayoutNode],
            idx: usize,
            tree: &mut NodeTree,
            parent_x: f32,
            parent_y: f32,
            parent_id: Option<NodeId>,
        ) {
            let node = &nodes[idx];
            let layout = engine.get_layout(node.taffy_id);
            let abs_x = parent_x + layout.x;
            let abs_y = parent_y + layout.y;

            let vis = &node.visual;
            let style = NodeStyle {
                fill_color: vis.fill,
                border_color: vis.border_color,
                border_width: vis.border_width,
                corner_radii: vis.corner_radius,
                shadow_color: vis.shadow_color,
                shadow_offset: vis.shadow_offset,
                shadow_blur: vis.shadow_blur,
                shadow_spread: vis.shadow_spread,
            };

            let scene_id = if let Some(pid) = parent_id {
                tree.add_child(pid, Rect::new(abs_x, abs_y, layout.width, layout.height), style)
            } else {
                tree.add(Rect::new(abs_x, abs_y, layout.width, layout.height), style)
            };

            if node.interactive {
                if let Some(ref hover) = node.hover_visual {
                    tree.set_hover_style(
                        scene_id,
                        NodeStyle {
                            fill_color: hover.fill,
                            border_color: hover.border_color,
                            border_width: hover.border_width,
                            corner_radii: hover.corner_radius,
                            shadow_color: hover.shadow_color,
                            shadow_offset: hover.shadow_offset,
                            shadow_blur: hover.shadow_blur,
                            shadow_spread: hover.shadow_spread,
                        },
                    );
                }
                if let Some(ref active) = node.active_visual {
                    tree.set_active_style(
                        scene_id,
                        NodeStyle {
                            fill_color: active.fill,
                            border_color: active.border_color,
                            border_width: active.border_width,
                            corner_radii: active.corner_radius,
                            shadow_color: active.shadow_color,
                            shadow_offset: active.shadow_offset,
                            shadow_blur: active.shadow_blur,
                            shadow_spread: active.shadow_spread,
                        },
                    );
                }
            }

            for &child_idx in &node.children {
                add_to_tree(engine, nodes, child_idx, tree, abs_x, abs_y, Some(scene_id));
            }
        }

        // Add header, card_area, status to scene
        add_to_tree(&engine, &nodes, header, tree, 0.0, 0.0, None);
        add_to_tree(&engine, &nodes, card_area, tree, 0.0, 0.0, None);
        add_to_tree(&engine, &nodes, status, tree, 0.0, 0.0, None);
    }

    fn render(&self, tree: &NodeTree) -> Vec<RectInstance> {
        let mut rects = Vec::new();

        // Background
        rects.push(RectInstance {
            rect: [0.0, 0.0, 2000.0, 2000.0],
            corner_radii: [0.0; 4],
            fill_color: self.theme.surface.to_array(),
            border_color: [0.0; 4],
            border_width: 0.0,
            gradient_angle: 0.0, rotation: 0.0, _pad0: 0.0, clip_rect: [0.0; 4],
            shadow_color: [0.0; 4],
            shadow_offset: [0.0; 2],
            shadow_params: [0.0; 2],
                        gradient_end_color: [0.0, 0.0, 0.0, 0.0],
        });

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
}

fn main() {
    sabitori::run(LayoutApp {
        theme: Theme::midnight(),
    });
}
