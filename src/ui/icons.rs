use eframe::egui::{self, Color32, Pos2, Response, Shape, Stroke, Ui, Vec2};

#[derive(Clone, Copy)]
pub enum Icon {
    ArrowForward,
    Delete,
    FastForward,
    FastRewind,
    Play,
    SkipNext,
    SkipPrevious,
    ArrowUp,
}

fn point(rect: egui::Rect, x: f32, y: f32) -> Pos2 {
    Pos2::new(rect.left() + rect.width() * x, rect.top() + rect.height() * y)
}

fn triangle(rect: egui::Rect, left: f32, right: f32, top: f32, bottom: f32, color: Color32) -> Shape {
    Shape::convex_polygon(
        vec![
            point(rect, left, top),
            point(rect, right, 0.5),
            point(rect, left, bottom),
        ],
        color,
        Stroke::NONE,
    )
}

pub fn paint(painter: &egui::Painter, color: Color32, icon: Icon, rect: egui::Rect) {
    match icon {
        // src/img/arrow_forward.svg
        // "m321-80-71-71 329-329-329-329 71-71 400 400L321-80Z" in viewBox 0 -960 960 960,
        // split into two convex halves so the chevron fills correctly.
        Icon::ArrowForward => {
            let tip_outer = point(rect, 0.751_04, 0.5);
            let tip_inner = point(rect, 0.603_13, 0.5);
            painter.add(Shape::convex_polygon(
                vec![
                    point(rect, 0.334_38, 0.083_33),
                    tip_outer,
                    tip_inner,
                    point(rect, 0.260_42, 0.157_29),
                ],
                color,
                Stroke::NONE,
            ));
            painter.add(Shape::convex_polygon(
                vec![
                    point(rect, 0.260_42, 0.842_71),
                    tip_inner,
                    tip_outer,
                    point(rect, 0.334_38, 0.916_67),
                ],
                color,
                Stroke::NONE,
            ));
        }
        Icon::ArrowUp => {
            painter.add(triangle(rect, 0.12, 0.88, 0.12, 0.88, color));
        }
        // src/img/play_arrow.svg: an outlined triangle (the inner triangle is a hole),
        // drawn here as the three trapezoids of the ring so it needs no backdrop.
        Icon::Play => {
            let outer = [
                point(rect, 0.324_86, 0.806_86),
                point(rect, 0.324_86, 0.193_14),
                point(rect, 0.807_36, 0.5),
            ];
            let inner = [
                point(rect, 0.419_66, 0.634_90),
                point(rect, 0.419_66, 0.365_10),
                point(rect, 0.629_69, 0.5),
            ];
            for i in 0..3 {
                let j = (i + 1) % 3;
                painter.add(Shape::convex_polygon(
                    vec![outer[i], outer[j], inner[j], inner[i]],
                    color,
                    Stroke::NONE,
                ));
            }
        }
        Icon::FastForward => {
            painter.add(triangle(rect, 0.08, 0.5, 0.1, 0.9, color));
            painter.add(triangle(rect, 0.5, 0.92, 0.1, 0.9, color));
        }
        Icon::FastRewind => {
            painter.add(Shape::convex_polygon(
                vec![point(rect, 0.5, 0.1), point(rect, 0.08, 0.5), point(rect, 0.5, 0.9)],
                color,
                Stroke::NONE,
            ));
            painter.add(Shape::convex_polygon(
                vec![point(rect, 0.92, 0.1), point(rect, 0.5, 0.5), point(rect, 0.92, 0.9)],
                color,
                Stroke::NONE,
            ));
        }
        Icon::SkipNext => {
            painter.add(triangle(rect, 0.08, 0.75, 0.1, 0.9, color));
            painter.rect_filled(
                egui::Rect::from_min_max(point(rect, 0.78, 0.1), point(rect, 0.92, 0.9)),
                0.0,
                color,
            );
        }
        Icon::SkipPrevious => {
            painter.rect_filled(
                egui::Rect::from_min_max(point(rect, 0.08, 0.1), point(rect, 0.22, 0.9)),
                0.0,
                color,
            );
            painter.add(Shape::convex_polygon(
                vec![point(rect, 0.92, 0.1), point(rect, 0.25, 0.5), point(rect, 0.92, 0.9)],
                color,
                Stroke::NONE,
            ));
        }
        // src/img/delete.svg: the can outline (body is a hole) plus the two inner bars.
        Icon::Delete => {
            for (x0, y0, x1, y1) in [
                (0.166_67, 0.166_67, 0.833_33, 0.25),   // lid
                (0.375, 0.125, 0.625, 0.166_67),        // handle
                (0.208_33, 0.25, 0.291_67, 0.791_67),   // left wall
                (0.708_33, 0.25, 0.791_67, 0.791_67),   // right wall
                (0.208_33, 0.791_67, 0.791_67, 0.875),  // bottom
                (0.375, 0.333_33, 0.458_33, 0.708_33),  // left bar
                (0.541_67, 0.333_33, 0.625, 0.708_33),  // right bar
            ] {
                painter.rect_filled(
                    egui::Rect::from_min_max(point(rect, x0, y0), point(rect, x1, y1)),
                    0.0,
                    color,
                );
            }
        }
    }
}

pub fn paint_at(ui: &Ui, icon: Icon, rect: egui::Rect) {
    paint(ui.painter(), ui.visuals().text_color(), icon, rect);
}

pub fn button(ui: &mut Ui, icon: Icon, size: f32) -> Response {
    let response = ui.add(egui::Button::new("").min_size(Vec2::splat(size + 6.0)));
    paint_at(ui, icon, response.rect.shrink(3.0));
    response
}
