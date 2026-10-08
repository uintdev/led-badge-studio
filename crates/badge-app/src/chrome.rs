//! Client-side window decorations: title bar with window controls, drag to
//! move, edge resizing and a drop shadow. Used when the compositor doesn't
//! draw a title bar for us (e.g. GNOME on Wayland); otherwise the content is
//! returned unchanged.

use std::rc::Rc;

use gpui::prelude::*;
use gpui::{
    AnyElement, App, Bounds, BoxShadow, CursorStyle, Decorations, HitboxBehavior, Hsla, MouseButton, Pixels, Point,
    ResizeEdge, SharedString, Size, Tiling, Window, canvas, div, point, px,
};

use crate::icons::{Icon, icon};
use crate::theme;
use crate::widgets::TextStyle;

const SHADOW: Pixels = px(10.0);
const ROUNDING: Pixels = px(10.0);
const TITLEBAR_HEIGHT: f32 = 36.0;

/// Wraps `content` in decorations when the window uses client-side ones.
pub fn decorate(
    content: impl IntoElement,
    title: SharedString,
    window: &mut Window,
    on_close: impl Fn(&mut Window, &mut App) + 'static,
) -> AnyElement {
    let Decorations::Client { tiling } = window.window_decorations() else {
        window.set_client_inset(px(0.));
        return div().size_full().child(content).into_any_element();
    };
    window.set_client_inset(SHADOW);
    let on_close = Rc::new(on_close);

    let frame = div()
        .id("window-frame")
        .size_full()
        .flex()
        .flex_col()
        .overflow_hidden()
        .bg(theme::bg())
        .border_color(theme::border())
        .cursor(CursorStyle::Arrow)
        .when(!(tiling.top || tiling.right), |d| d.rounded_tr(ROUNDING))
        .when(!(tiling.top || tiling.left), |d| d.rounded_tl(ROUNDING))
        .when(!(tiling.bottom || tiling.right), |d| d.rounded_br(ROUNDING))
        .when(!(tiling.bottom || tiling.left), |d| d.rounded_bl(ROUNDING))
        .when(!tiling.top, gpui::Styled::border_t_1)
        .when(!tiling.bottom, gpui::Styled::border_b_1)
        .when(!tiling.left, gpui::Styled::border_l_1)
        .when(!tiling.right, gpui::Styled::border_r_1)
        .when(!tiling.is_tiled(), |d| {
            d.shadow(vec![BoxShadow {
                color: Hsla {
                    h: 0.,
                    s: 0.,
                    l: 0.,
                    a: 0.45,
                },
                blur_radius: SHADOW / 2.,
                spread_radius: px(0.),
                inset: false,
                offset: point(px(0.), px(0.)),
            }])
        })
        .on_mouse_move(|_, _, cx| cx.stop_propagation())
        .child(titlebar(
            title,
            window,
            on_close,
            !(tiling.top || tiling.left),
            !(tiling.top || tiling.right),
        ))
        .child(div().flex_1().min_h_0().child(content));

    // Transparent margin around the frame: shadow + resize handles.
    div()
        .id("window-backdrop")
        .size_full()
        .bg(gpui::transparent_black())
        .child(
            canvas(
                |_bounds, window, _cx| {
                    window.insert_hitbox(
                        Bounds::new(point(px(0.), px(0.)), window.window_bounds().get_bounds().size),
                        HitboxBehavior::Normal,
                    )
                },
                move |_bounds, hitbox, window, _cx| {
                    let size = window.window_bounds().get_bounds().size;
                    if let Some(edge) = resize_edge(window.mouse_position(), size, tiling) {
                        window.set_cursor_style(edge_cursor(edge), &hitbox);
                    }
                },
            )
            .size_full()
            .absolute(),
        )
        .when(!tiling.top, |d| d.pt(SHADOW))
        .when(!tiling.bottom, |d| d.pb(SHADOW))
        .when(!tiling.left, |d| d.pl(SHADOW))
        .when(!tiling.right, |d| d.pr(SHADOW))
        .on_mouse_move(|_, window, _| window.refresh())
        .on_mouse_down(MouseButton::Left, move |e, window, _| {
            let size = window.window_bounds().get_bounds().size;
            if let Some(edge) = resize_edge(e.position, size, tiling) {
                window.start_window_resize(edge);
            }
        })
        .child(frame)
        .into_any_element()
}

/// GPUI clips children to rectangles, so whatever is painted in a rounded
/// top corner (`round_left`, `round_right`) needs the radius itself.
fn titlebar(
    title: SharedString,
    window: &mut Window,
    on_close: crate::widgets::WindowAction,
    round_left: bool,
    round_right: bool,
) -> impl IntoElement + use<> {
    let inner = ROUNDING - px(1.);
    let controls = window.window_controls();
    let maximized = window.is_maximized();
    div()
        .id("titlebar")
        .flex()
        .flex_row()
        .flex_none()
        .items_center()
        .h(px(TITLEBAR_HEIGHT))
        .pl_4()
        .bg(theme::panel())
        .when(round_left, |d| d.rounded_tl(inner))
        .when(round_right, |d| d.rounded_tr(inner))
        .border_b_1()
        .border_color(theme::border())
        .on_mouse_down(MouseButton::Left, |e, window, cx| {
            if e.click_count >= 2 {
                window.zoom_window();
            } else {
                window.start_window_move();
            }
            cx.stop_propagation();
        })
        .on_mouse_down(MouseButton::Right, |e, window, cx| {
            window.show_window_menu(e.position);
            cx.stop_propagation();
        })
        .child({
            // The unsaved-changes dot ("• name - app") on its own, with room
            // after it: a space alone looks cramped in this font.
            let (dot, name) = match title.strip_prefix("\u{2022} ") {
                Some(name) => (true, SharedString::from(name.to_string())),
                None => (false, title),
            };
            div()
                .flex()
                .flex_row()
                .items_center()
                .gap_2()
                .text_body()
                .font_weight(gpui::FontWeight::SEMIBOLD)
                .text_color(theme::text())
                .when(dot, |d| d.child("\u{2022}"))
                .child(name)
        })
        .child(div().flex_1())
        .when(controls.minimize, |d| {
            d.child(control_button("win-min", Icon::Minimize, false, false, |w, _| {
                w.minimize_window()
            }))
        })
        .when(controls.maximize, |d| {
            d.child(control_button(
                "win-max",
                if maximized { Icon::Restore } else { Icon::Maximize },
                false,
                false,
                |w, _| w.zoom_window(),
            ))
        })
        .child(control_button(
            "win-close",
            Icon::Close,
            true,
            round_right,
            move |w, cx| on_close(w, cx),
        ))
}

fn control_button(
    id: &'static str,
    glyph: Icon,
    danger: bool,
    round_top_right: bool,
    action: impl Fn(&mut Window, &mut App) + 'static,
) -> impl IntoElement {
    div()
        .when(round_top_right, |d| d.rounded_tr(ROUNDING - px(1.)))
        .id(SharedString::from(id))
        .w(px(46.))
        .h_full()
        .flex()
        .items_center()
        .justify_center()
        .text_body()
        .text_color(theme::text_muted())
        .cursor_pointer()
        .hover(move |s| {
            if danger {
                s.bg(theme::danger()).text_color(gpui::white())
            } else {
                s.bg(theme::hover()).text_color(theme::text())
            }
        })
        // Keep the title bar from starting a window move on these.
        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
        .on_click(move |_, w, cx| action(w, cx))
        .group(id)
        .child({
            // Icons have a fixed color: show the hover-colored one on hover.
            let hover: gpui::Hsla = if danger { gpui::white() } else { theme::text().into() };
            div()
                .relative()
                .size(px(12.))
                .child(
                    icon(glyph, 12., theme::text_muted())
                        .absolute()
                        .group_hover(id, |s| s.invisible()),
                )
                .child(
                    icon(glyph, 12., hover)
                        .absolute()
                        .invisible()
                        .group_hover(id, |s| s.visible()),
                )
        })
}

/// The resize handle under `pos`, if any. Tiled edges have no margin (the
/// content reaches the edge there), so they have no handle.
fn resize_edge(pos: Point<Pixels>, size: Size<Pixels>, tiling: Tiling) -> Option<ResizeEdge> {
    let s = SHADOW;
    let top = !tiling.top && pos.y < s;
    let bottom = !tiling.bottom && pos.y > size.height - s;
    let left = !tiling.left && pos.x < s;
    let right = !tiling.right && pos.x > size.width - s;
    Some(match (top, bottom, left, right) {
        (true, _, true, _) => ResizeEdge::TopLeft,
        (true, _, _, true) => ResizeEdge::TopRight,
        (_, true, true, _) => ResizeEdge::BottomLeft,
        (_, true, _, true) => ResizeEdge::BottomRight,
        (true, ..) => ResizeEdge::Top,
        (_, true, ..) => ResizeEdge::Bottom,
        (_, _, true, _) => ResizeEdge::Left,
        (_, _, _, true) => ResizeEdge::Right,
        _ => return None,
    })
}

fn edge_cursor(edge: ResizeEdge) -> CursorStyle {
    match edge {
        ResizeEdge::Top | ResizeEdge::Bottom => CursorStyle::ResizeUpDown,
        ResizeEdge::Left | ResizeEdge::Right => CursorStyle::ResizeLeftRight,
        ResizeEdge::TopLeft | ResizeEdge::BottomRight => CursorStyle::ResizeUpLeftDownRight,
        ResizeEdge::TopRight | ResizeEdge::BottomLeft => CursorStyle::ResizeUpRightDownLeft,
    }
}
