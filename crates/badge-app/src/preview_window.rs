//! The preview in its own window (Preview → Pop Out), mirroring the main
//! window's preview. F11 (also Ctrl+Cmd+F on macOS) or a double-click
//! toggles full screen, where the
//! title bar shows only near the top edge (macOS's own, there).

use gpui::prelude::*;
use gpui::{
    App, Bounds, Context, Entity, FocusHandle, Focusable, MouseButton, MouseMoveEvent, SharedString, Subscription,
    Window, WindowBounds, WindowDecorations, WindowHandle, WindowOptions, actions, div, px,
};

use crate::app::BadgeApp;
use crate::icons::{Icon, icon};
use crate::led_view;
use crate::theme;
use crate::widgets::TextStyle;

actions!(preview_window, [ClosePreview, ToggleFullscreen]);

pub fn bindings() -> Vec<gpui::KeyBinding> {
    let ctx = Some("PreviewWindow");
    let mut bindings = vec![
        gpui::KeyBinding::new("escape", ClosePreview, ctx),
        gpui::KeyBinding::new("f11", ToggleFullscreen, ctx),
    ];
    // macOS's own full screen shortcut (F11 is usually Show Desktop there).
    if cfg!(target_os = "macos") {
        bindings.push(gpui::KeyBinding::new("ctrl-cmd-f", ToggleFullscreen, ctx));
    }
    bindings
}

/// The full screen shortcut as shown in the interface.
pub fn fullscreen_key() -> &'static str {
    if cfg!(target_os = "macos") {
        "\u{2303}\u{2318}F"
    } else {
        "F11"
    }
}

/// How close to the top the pointer must be to show the title bar in full
/// screen.
const REVEAL: f32 = 48.;

/// Whether full screen needs our own title bar. macOS shows its own, with the
/// window buttons, when the pointer reaches the top of the screen.
const FULLSCREEN_BAR: bool = !cfg!(target_os = "macos");

pub struct PreviewWindow {
    app: Entity<BadgeApp>,
    focus: FocusHandle,
    /// Full screen: the pointer is near the top, so the title bar shows.
    show_bar: bool,
    /// The colors the native window frame was given last.
    frame_dark: Option<bool>,
    _observe: Subscription,
}

impl Focusable for PreviewWindow {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}

impl PreviewWindow {
    /// Opens the window, at `bounds` if given (to reopen it as it was);
    /// `None` if the system refused.
    pub fn open(app: Entity<BadgeApp>, bounds: Option<WindowBounds>, cx: &mut App) -> Option<WindowHandle<Self>> {
        let bounds = bounds
            .unwrap_or_else(|| WindowBounds::Windowed(Bounds::centered(None, gpui::size(px(900.), px(330.)), cx)));
        cx.open_window(
            WindowOptions {
                window_bounds: Some(bounds),
                titlebar: Some(gpui::TitlebarOptions {
                    title: Some(concat!("Preview - ", env!("BRAND_NAME")).into()),
                    ..Default::default()
                }),
                window_min_size: Some(gpui::size(px(320.), px(160.))),
                app_id: Some(crate::APP_ID.into()),
                window_decorations: cfg!(target_os = "linux").then_some(WindowDecorations::Client),
                window_background: if cfg!(target_os = "linux") {
                    gpui::WindowBackgroundAppearance::Transparent
                } else {
                    gpui::WindowBackgroundAppearance::Opaque
                },
                ..Default::default()
            },
            |window, cx| {
                cx.new(|cx| {
                    let observe = cx.observe(&app, |_, _, cx| cx.notify());
                    cx.on_release({
                        let app = app.downgrade();
                        let id = window.window_handle().window_id();
                        move |_, cx| {
                            app.update(cx, |t, cx| t.pop_out_closed(id, cx)).ok();
                        }
                    })
                    .detach();
                    // On Windows, closed the app's way: letting the close go
                    // ahead has Windows destroy the window itself, and GPUI
                    // then logs errors destroying it again.
                    #[cfg(windows)]
                    window.on_window_should_close(cx, |window, _| {
                        window.remove_window();
                        false
                    });
                    let focus = cx.focus_handle();
                    window.focus(&focus, cx);
                    PreviewWindow {
                        app,
                        focus,
                        show_bar: false,
                        frame_dark: None,
                        _observe: observe,
                    }
                })
            },
        )
        .ok()
    }

    /// Esc: leaves full screen first; otherwise closes the window.
    fn close(&mut self, _: &ClosePreview, window: &mut Window, _: &mut Context<Self>) {
        if window.is_fullscreen() {
            window.toggle_fullscreen();
        } else {
            window.remove_window();
        }
    }

    fn toggle_fullscreen(&mut self, _: &ToggleFullscreen, window: &mut Window, _: &mut Context<Self>) {
        window.toggle_fullscreen();
    }

    /// The LED matrix, as large as fits in `room`.
    fn matrix(&self, room: gpui::Size<gpui::Pixels>, window: &Window, cx: &App) -> gpui::AnyElement {
        let (frame, cols, mut style) = self.app.read(cx).preview_frame();
        let (w, h) = (f32::from(room.width), f32::from(room.height));
        style.look.cell = led_view::fit_cell(120., (w, h), cols, frame.height(), window.scale_factor());
        let fits = cols as f32 * style.look.cell <= w + 0.5;
        let matrix = led_view::led_matrix(frame, cols, style, None);
        if fits {
            matrix.into_any_element()
        } else {
            div()
                .id("preview-scroll")
                .max_w(px(w))
                .overflow_x_scroll()
                .child(matrix)
                .into_any_element()
        }
    }
}

impl Render for PreviewWindow {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if self.frame_dark != Some(theme::is_dark()) {
            self.frame_dark = Some(theme::is_dark());
            crate::system_theme::set_native_frame(window, theme::is_dark());
        }
        let fullscreen = window.is_fullscreen();
        let viewport = window.viewport_size();
        let frame_size = if fullscreen || window.window_decorations() == gpui::Decorations::Server {
            0.
        } else {
            36. + 2. * 10.
        };
        let margin = if fullscreen { 32. } else { 24. };
        let room = gpui::size(
            viewport.width - px(2. * margin + 2. * 10.),
            viewport.height - px(2. * margin + frame_size),
        );

        let content = div()
            .id("preview-window")
            .key_context("PreviewWindow")
            .track_focus(&self.focus)
            .on_action(cx.listener(Self::close))
            .on_action(cx.listener(Self::toggle_fullscreen))
            .map(|d| {
                // File → Close Window in the macOS menu bar.
                #[cfg(target_os = "macos")]
                let d = d.on_action(cx.listener(|_, _: &crate::app::CloseWindow, window, _| window.remove_window()));
                d
            })
            .on_mouse_move(cx.listener(|t, e: &MouseMoveEvent, _, cx| {
                let near = FULLSCREEN_BAR && f32::from(e.position.y) < REVEAL;
                if near != t.show_bar {
                    t.show_bar = near;
                    cx.notify();
                }
            }))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|_, e: &gpui::MouseDownEvent, window, _| {
                    if e.click_count == 2 {
                        window.toggle_fullscreen();
                    }
                }),
            )
            .relative()
            .size_full()
            .flex()
            .items_center()
            .justify_center()
            .bg(gpui::rgb(0x050506))
            .child(self.matrix(room, window, cx))
            .when(!fullscreen, |d| {
                d.child(
                    div()
                        .absolute()
                        .bottom(px(6.))
                        .right(px(10.))
                        .text_label()
                        .text_color(theme::text_dim())
                        .child(format!(
                            "Esc closes \u{b7} {} or double-click for full screen",
                            fullscreen_key()
                        )),
                )
            })
            .when(fullscreen && self.show_bar, |d| d.child(fullscreen_bar(cx)));

        if fullscreen {
            return content.into_any_element();
        }
        let title: SharedString = concat!("Preview - ", env!("BRAND_NAME")).into();
        crate::chrome::decorate(content, title, window, |window, _| window.remove_window())
    }
}

/// The bar shown at the top in full screen: title, leave full screen, close
/// (not on macOS; see [`FULLSCREEN_BAR`]).
fn fullscreen_bar(cx: &mut Context<PreviewWindow>) -> impl IntoElement + use<> {
    let control = |id: &'static str, i: Icon| {
        div()
            .id(id)
            .w(px(46.))
            .h_full()
            .flex()
            .items_center()
            .justify_center()
            .cursor_pointer()
            .hover(|s| s.bg(theme::hover()))
            .child(icon(i, 12., theme::text()))
    };
    div()
        .absolute()
        .top_0()
        .left_0()
        .right_0()
        .h(px(36.))
        .flex()
        .flex_row()
        .items_center()
        .pl_3()
        .bg(theme::panel())
        .border_b_1()
        .border_color(theme::border())
        .text_body()
        .text_color(theme::text())
        .child(concat!("Preview - ", env!("BRAND_NAME")))
        .child(div().flex_1())
        .child(
            control("leave-fullscreen", Icon::Restore)
                .on_click(cx.listener(|_, _, window, _| window.toggle_fullscreen())),
        )
        .child(
            control("close-preview", Icon::Close)
                .hover(|s| s.bg(theme::danger()))
                .on_click(cx.listener(|_, _, window, _| window.remove_window())),
        )
}

#[cfg(all(test, target_os = "macos"))]
mod tests {
    use super::*;

    #[test]
    fn ctrl_cmd_f_toggles_full_screen() {
        let typed = [gpui::Keystroke::parse("ctrl-cmd-f").unwrap()];
        assert!(
            bindings()
                .iter()
                .any(|b| b.match_keystrokes(&typed) == Some(false) && b.action().partial_eq(&ToggleFullscreen))
        );
    }
}
