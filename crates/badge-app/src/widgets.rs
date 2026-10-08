//! Small stateless widgets built from GPUI primitives.

use std::rc::Rc;

use gpui::prelude::*;
use gpui::{AnyElement, AnyView, App, ClickEvent, ElementId, Hsla, MouseButton, SharedString, Window, div, px};

use crate::icons::{Icon, icon};
use crate::theme;

type ClickHandler = Rc<dyn Fn(&ClickEvent, &mut Window, &mut App)>;
/// The app's text sizes (see `theme`), for any element.
pub trait TextStyle: Styled + Sized {
    /// Body text: controls, menu items, the message box.
    fn text_body(self) -> Self {
        self.text_size(px(theme::TEXT))
    }
    /// Labels in front of controls, secondary lines.
    fn text_label(self) -> Self {
        self.text_size(px(theme::TEXT_SMALL))
    }
    /// Hints and captions.
    fn text_caption(self) -> Self {
        self.text_size(px(theme::TEXT_TINY))
    }
    /// Card titles.
    fn text_title(self) -> Self {
        self.text_size(px(theme::TEXT_TITLE))
            .font_weight(gpui::FontWeight::SEMIBOLD)
    }
    /// Dialog titles.
    fn text_heading(self) -> Self {
        self.text_size(px(theme::TEXT_HEADING))
            .font_weight(gpui::FontWeight::SEMIBOLD)
    }
}

impl<T: Styled + Sized> TextStyle for T {}

/// A callback that only needs the window and app.
pub type WindowAction = Rc<dyn Fn(&mut Window, &mut App)>;

thread_local! {
    static FOCUS_VISIBLE: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// Whether keyboard focus is shown: only after the keyboard moved it, until
/// the next click.
pub fn focus_visible() -> bool {
    FOCUS_VISIBLE.with(std::cell::Cell::get)
}

pub fn set_focus_visible(on: bool) {
    FOCUS_VISIBLE.with(|f| f.set(on));
}

/// The focus ring, drawn outside the element so its size doesn't change.
pub fn focus_ring(s: gpui::StyleRefinement) -> gpui::StyleRefinement {
    s.shadow(vec![gpui::BoxShadow {
        color: Hsla::from(theme::border_focus()).opacity(0.7),
        offset: gpui::point(px(0.), px(0.)),
        blur_radius: px(0.5),
        spread_radius: px(2.),
        inset: false,
    }])
}

/// Makes a control reachable with Tab. Clicking it doesn't take the keyboard
/// away from where it was.
pub fn tab_stop(el: gpui::Stateful<gpui::Div>) -> gpui::Stateful<gpui::Div> {
    el.tab_index(0)
        .on_mouse_down(MouseButton::Left, |_, window, _| window.prevent_default())
        .when(focus_visible(), |d| d.focus(focus_ring))
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum ButtonStyle {
    Primary,
    Secondary,
    Danger,
}

/// A glowing status dot: the badge connection light on the device button.
#[derive(Clone)]
pub struct StatusLight {
    pub color: Hsla,
    /// Explains the color (shown on hover).
    pub tip: SharedString,
}

#[derive(IntoElement)]
pub struct Button {
    id: ElementId,
    label: SharedString,
    style: ButtonStyle,
    disabled: bool,
    selected: bool,
    compact: bool,
    /// Part of a `segmented` group: borderless, inside the group's frame.
    segment: bool,
    /// Shares the row's width with the other growing buttons.
    grow: bool,
    tooltip: Option<SharedString>,
    swatch: Option<Hsla>,
    /// Glowing dot before the label.
    light: Option<Hsla>,
    /// Icon before the label (or alone, with an empty label).
    icon: Option<Icon>,
    /// Icon after the label (dropdown arrows).
    trailing: Option<Icon>,
    /// Focus handle for dialog buttons.
    focus: Option<gpui::FocusHandle>,
    on_click: Option<ClickHandler>,
}

pub fn button(id: impl Into<ElementId>, label: impl Into<SharedString>) -> Button {
    Button {
        id: id.into(),
        label: label.into(),
        style: ButtonStyle::Secondary,
        disabled: false,
        selected: false,
        compact: false,
        segment: false,
        grow: false,
        tooltip: None,
        swatch: None,
        light: None,
        icon: None,
        trailing: None,
        focus: None,
        on_click: None,
    }
}

impl Button {
    pub fn style(mut self, s: ButtonStyle) -> Self {
        self.style = s;
        self
    }
    pub fn primary(self) -> Self {
        self.style(ButtonStyle::Primary)
    }
    pub fn danger(self) -> Self {
        self.style(ButtonStyle::Danger)
    }
    pub fn disabled(mut self, d: bool) -> Self {
        self.disabled = d;
        self
    }
    pub fn selected(mut self, s: bool) -> Self {
        self.selected = s;
        self
    }
    pub fn compact(mut self) -> Self {
        self.compact = true;
        self
    }
    pub fn tooltip(mut self, t: impl Into<SharedString>) -> Self {
        self.tooltip = Some(t.into());
        self
    }
    /// A glowing status dot before the label, explained by its tooltip.
    pub fn light(mut self, light: StatusLight) -> Self {
        self.light = Some(light.color);
        self.tooltip(light.tip)
    }
    fn segment(mut self) -> Self {
        self.segment = true;
        self
    }
    /// Takes an equal share of the row's width (in a `segmented` group
    /// given a width).
    pub fn grow(mut self) -> Self {
        self.grow = true;
        self
    }
    pub fn focus(mut self, handle: &gpui::FocusHandle) -> Self {
        self.focus = Some(handle.clone());
        self
    }
    pub fn icon(mut self, i: Icon) -> Self {
        self.icon = Some(i);
        self
    }
    pub fn trailing(mut self, i: Icon) -> Self {
        self.trailing = Some(i);
        self
    }
    pub fn swatch(mut self, c: Hsla) -> Self {
        self.swatch = Some(c);
        self
    }
    pub fn on_click(mut self, f: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static) -> Self {
        self.on_click = Some(Rc::new(f));
        self
    }
}

impl RenderOnce for Button {
    fn render(self, _window: &mut Window, _cx: &mut App) -> impl IntoElement {
        let (bg, hover, fg, border) = match (self.style, self.selected) {
            (_, false) if self.segment => (
                gpui::transparent_black().into(),
                theme::hover(),
                theme::text_muted(),
                gpui::transparent_black().into(),
            ),
            (_, true) if self.segment => (
                theme::selected(),
                theme::selected(),
                theme::text(),
                theme::border_focus(),
            ),
            (ButtonStyle::Primary, _) => (
                theme::accent(),
                theme::accent_hover(),
                gpui::white().into(),
                theme::accent_border(),
            ),
            (_, true) => (
                theme::selected(),
                theme::selected(),
                theme::text(),
                theme::border_focus(),
            ),
            (ButtonStyle::Secondary, false) => (
                theme::panel_alt(),
                theme::hover(),
                theme::text(),
                theme::border_strong(),
            ),
            (ButtonStyle::Danger, _) => (
                theme::danger(),
                theme::danger_hover(),
                gpui::white().into(),
                theme::danger_border(),
            ),
        };
        let mut el = div()
            .id(self.id)
            .when(!self.grow, gpui::Styled::flex_none)
            .when(self.grow, |d| d.flex_1().min_w_0())
            .flex()
            .flex_row()
            .items_center()
            .justify_center()
            .gap_2()
            .when(self.segment, |d| {
                d.h(px(COMPACT_H - 6.)).min_w(px(COMPACT_H - 6.)).px_2p5().rounded_md()
            })
            .when(self.compact && !self.segment, |d| {
                d.h(px(COMPACT_H)).px_3().rounded_lg()
            })
            .when(!self.compact && !self.segment, |d| {
                d.h(px(CONTROL_H)).px_3p5().rounded_lg()
            })
            .border_1()
            .border_color(border)
            .bg(bg)
            .text_color(fg)
            .text_body()
            .whitespace_nowrap()
            .when_some(self.swatch, |d, c| {
                d.child(
                    div()
                        .size(px(12.))
                        .rounded_sm()
                        .bg(c)
                        .border_1()
                        .border_color(theme::border()),
                )
            })
            .when_some(self.light, |d, c| {
                d.child(
                    div()
                        .size(px(8.))
                        .flex_none()
                        // Icons have a little room around their artwork; the
                        // dot doesn't, so it gets that room here to look as
                        // far from the label as they do.
                        .mr(px(2.))
                        .rounded_full()
                        .bg(c)
                        .shadow(vec![gpui::BoxShadow {
                            color: c.opacity(0.55),
                            blur_radius: px(5.),
                            spread_radius: px(0.),
                            inset: false,
                            offset: gpui::point(px(0.), px(0.)),
                        }]),
                )
            })
            // Icon-only buttons are square, with a larger icon.
            .when(self.label.is_empty() && self.icon.is_some() && !self.segment, |d| {
                let side = if self.compact { COMPACT_H } else { CONTROL_H };
                d.w(px(side)).px_0()
            })
            .when_some(self.icon, |d, i| d.child(icon(i, 16., fg)))
            .when(!self.label.is_empty(), |d| d.child(self.label))
            .when_some(self.trailing, |d, i| d.child(icon(i, 14., fg)));
        let enabled = !self.disabled;
        if self.disabled {
            el = el.opacity(0.45);
        } else {
            let segment = self.segment && !self.selected;
            el = el.cursor_pointer().hover(move |s| {
                let s = s.bg(hover);
                if segment { s.text_color(theme::text()) } else { s }
            });
            if let Some(f) = self.on_click {
                el = el.on_click(move |e, w, cx| f(e, w, cx));
            }
        }
        if let Some(t) = self.tooltip {
            el = el.tooltip(move |_, cx| tooltip(t.clone(), cx));
        }
        if let Some(handle) = &self.focus {
            el = el
                .track_focus(handle)
                .on_mouse_down(MouseButton::Left, |_, window, _| window.prevent_default())
                .when(focus_visible(), |d| d.focus(focus_ring));
        } else if enabled {
            el = tab_stop(el);
        }
        el
    }
}

/// Height of buttons, dropdowns and steppers.
pub const CONTROL_H: f32 = 34.;
/// Height of every control in the section rows.
pub const COMPACT_H: f32 = 32.;

/// Mutually exclusive choices joined in one frame. Wraps when narrow.
pub fn segmented(buttons: impl IntoIterator<Item = Button>) -> gpui::Div {
    div()
        .flex_none()
        .flex()
        .flex_row()
        .flex_wrap()
        .items_center()
        .gap(px(2.))
        .p(px(2.))
        .min_h(px(COMPACT_H))
        .rounded_lg()
        .border_1()
        .border_color(theme::border_strong())
        .bg(theme::panel_alt())
        .children(buttons.into_iter().map(|b| b.compact().segment()))
}

pub fn divider() -> gpui::Div {
    div().flex_none().w(px(1.)).h(px(20.)).bg(theme::border())
}

/// A fixed-width label in front of a row, so a section's rows line up.
pub fn row_label(text: impl Into<SharedString>) -> gpui::Div {
    label(text).w(px(76.))
}

/// A small warning chip, as tall as the controls around it.
pub fn warning_chip(id: &'static str, i: Icon, text: &'static str, height: f32) -> gpui::Stateful<gpui::Div> {
    div()
        .id(id)
        .flex_none()
        .h(px(height))
        .px_2p5()
        .rounded_lg()
        .bg(theme::warning_soft())
        .border_1()
        .border_color(theme::warning_edge())
        .text_label()
        .text_color(theme::warning())
        .flex()
        .flex_row()
        .items_center()
        .gap_1p5()
        .child(icon(i, 14., theme::warning()))
        .child(text)
}

struct Tooltip(SharedString);

impl Render for Tooltip {
    fn render(&mut self, _: &mut Window, _: &mut gpui::Context<Self>) -> impl IntoElement {
        div()
            .px_2p5()
            .py_1p5()
            .rounded_lg()
            .bg(theme::panel())
            .border_1()
            .border_color(theme::border_strong())
            .shadow(theme::popover_shadow())
            .text_label()
            .text_color(theme::text())
            .max_w(px(360.))
            .child(self.0.clone())
    }
}

pub fn tooltip(text: SharedString, cx: &mut App) -> AnyView {
    cx.new(|_| Tooltip(text)).into()
}

/// A checkbox with a label. `on_toggle` receives the new value.
pub fn checkbox(
    id: impl Into<ElementId>,
    label: impl Into<SharedString>,
    checked: bool,
    on_toggle: impl Fn(bool, &mut Window, &mut App) + 'static,
) -> gpui::Stateful<gpui::Div> {
    let label: SharedString = label.into();
    tab_stop(div().id(id.into()))
        .group("checkbox")
        .flex_none()
        .h(px(COMPACT_H))
        .flex()
        .flex_row()
        .items_center()
        .gap_2()
        .cursor_pointer()
        .text_body()
        .text_color(theme::text())
        .child(
            div()
                .flex_none()
                .size(px(18.))
                .rounded(px(5.))
                .border_1()
                .flex()
                .items_center()
                .justify_center()
                .text_label()
                .when(checked, |d| {
                    d.bg(theme::accent())
                        .border_color(theme::accent())
                        .child(icon(Icon::Check, 14., gpui::white()))
                })
                .when(!checked, |d| {
                    d.bg(theme::panel_alt())
                        .border_color(theme::border_strong())
                        .group_hover("checkbox", |s| s.border_color(theme::border_focus()))
                }),
        )
        .when(!label.is_empty(), |d| d.child(label))
        .on_click(move |_, w, cx| on_toggle(!checked, w, cx))
}

pub fn stepper(
    id: &'static str,
    value: i64,
    min: i64,
    max: i64,
    suffix: &'static str,
    height: f32,
    on_change: impl Fn(i64, &mut Window, &mut App) + 'static,
) -> AnyElement {
    let on_change = Rc::new(on_change);
    let dec = on_change.clone();
    let at_min = value <= min;
    let at_max = value >= max;
    div()
        .flex_none()
        .flex()
        .flex_row()
        .items_center()
        .h(px(height))
        .overflow_hidden()
        .rounded_lg()
        .border_1()
        .border_color(theme::border_strong())
        .bg(theme::panel_alt())
        .child(
            tab_stop(div().id(SharedString::from(format!("{id}-dec"))))
                .w(px(28.))
                .h_full()
                .flex()
                .items_center()
                .justify_center()
                .when(!at_min, |d| d.cursor_pointer().hover(|s| s.bg(theme::hover())))
                .child(icon(
                    Icon::Minus,
                    14.,
                    if at_min { theme::text_dim() } else { theme::text() },
                ))
                .on_click(move |_, w, cx| dec((value - 1).max(min), w, cx)),
        )
        .child(
            div()
                .min_w(px(40.))
                .h_full()
                .px_1p5()
                .flex()
                .items_center()
                .justify_center()
                .border_x_1()
                .border_color(theme::border_strong())
                .text_body()
                .child(format!("{value}{suffix}")),
        )
        .child(
            tab_stop(div().id(SharedString::from(format!("{id}-inc"))))
                .w(px(28.))
                .h_full()
                .flex()
                .items_center()
                .justify_center()
                .when(!at_max, |d| d.cursor_pointer().hover(|s| s.bg(theme::hover())))
                .child(icon(
                    Icon::Plus,
                    14.,
                    if at_max { theme::text_dim() } else { theme::text() },
                ))
                .on_click(move |_, w, cx| on_change((value + 1).min(max), w, cx)),
        )
        .into_any_element()
}

pub fn label(text: impl Into<SharedString>) -> gpui::Div {
    div()
        .flex_none()
        .whitespace_nowrap()
        .text_label()
        .text_color(theme::text_muted())
        .child(text.into())
}

/// A card with a title.
pub fn section(title: impl Into<SharedString>) -> gpui::Div {
    card().child(div().text_title().text_color(theme::text()).child(title.into()))
}

/// A card: a rounded panel for one part of the window.
pub fn card() -> gpui::Div {
    div()
        .flex()
        .flex_col()
        .gap_3()
        .p_4()
        .rounded_xl()
        .bg(theme::panel())
        .border_1()
        .border_color(theme::border())
        .shadow(theme::card_shadow())
}

/// A small heading inside a card or sidebar.
pub fn subheading(text: impl Into<SharedString>) -> gpui::Div {
    div()
        .text_label()
        .font_weight(gpui::FontWeight::SEMIBOLD)
        .text_color(theme::text_muted())
        .child(text.into())
}

/// Height of a menu row.
pub const MENU_ROW_H: f32 = 32.;
/// The tallest a menu gets before it scrolls: enough for the whole File
/// menu (430 px on Linux), so only long lists such as the fonts scroll.
pub const MENU_MAX_H: f32 = 480.;

pub struct MenuItem {
    pub label: SharedString,
    pub selected: bool,
    pub on_select: WindowAction,
    /// Keyboard shortcut shown on the right.
    pub hint: Option<SharedString>,
    /// A divider above this item.
    pub separator: bool,
    /// Shown dimmed and does nothing when clicked.
    pub disabled: bool,
    /// Opens a submenu: shows an arrow on the right.
    pub submenu: bool,
    /// Shown on hover.
    pub tooltip: Option<SharedString>,
    /// A group heading: small, muted, can't be chosen.
    pub header: bool,
}

pub fn menu_item(
    label: impl Into<SharedString>,
    selected: bool,
    f: impl Fn(&mut Window, &mut App) + 'static,
) -> MenuItem {
    MenuItem {
        label: label.into(),
        selected,
        on_select: Rc::new(f),
        hint: None,
        separator: false,
        disabled: false,
        submenu: false,
        tooltip: None,
        header: false,
    }
}

impl MenuItem {
    pub fn hint(mut self, h: impl Into<SharedString>) -> Self {
        self.hint = Some(h.into());
        self
    }

    pub fn separator(mut self) -> Self {
        self.separator = true;
        self
    }

    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }

    pub fn submenu(mut self) -> Self {
        self.submenu = true;
        self
    }

    pub fn tooltip(mut self, text: impl Into<SharedString>) -> Self {
        self.tooltip = Some(text.into());
        self
    }

    /// Makes this a group heading (skipped by the arrow keys).
    pub fn header(mut self) -> Self {
        self.header = true;
        self.disabled = true;
        self
    }
}

/// A dropdown menu with row `highlight` marked (the keyboard position).
pub fn menu_with_highlight(
    id: &'static str,
    items: Vec<MenuItem>,
    min_width: f32,
    highlight: Option<usize>,
) -> gpui::Stateful<gpui::Div> {
    // Menus with a current choice get a check-mark column.
    // (A submenu item is "selected" while its submenu is open: that's a
    // highlight, not a choice.)
    let checks = items.iter().any(|i| i.selected && !i.header && !i.submenu);
    div()
        .id(id)
        .occlude()
        .flex()
        .flex_col()
        .min_w(px(min_width))
        .max_h(px(MENU_MAX_H))
        .overflow_y_scroll()
        .p_1()
        .rounded_lg()
        .bg(theme::panel())
        .border_1()
        .border_color(theme::border_strong())
        .shadow(theme::popover_shadow())
        .children(items.into_iter().enumerate().flat_map(|(i, item)| {
            let f = item.on_select.clone();
            let sep = item.separator.then(|| {
                div()
                    .flex_none()
                    .h(px(1.))
                    .my_1()
                    .mx_1p5()
                    .bg(theme::border())
                    .into_any_element()
            });
            let disabled = item.disabled;
            if item.header {
                let heading = div()
                    .id(i)
                    .flex_none()
                    .h(px(28.))
                    .px_2p5()
                    .when(checks, |d| d.pl(px(10. + 16. + 8.)))
                    .flex()
                    .items_end()
                    .pb_1()
                    .text_caption()
                    .font_weight(gpui::FontWeight::SEMIBOLD)
                    .text_color(theme::text_dim())
                    .child(item.label)
                    .into_any_element();
                return sep.into_iter().chain(std::iter::once(heading));
            }
            let row = div()
                .id(i)
                .flex_none()
                .h(px(MENU_ROW_H))
                .px_2p5()
                .rounded_md()
                .flex()
                .flex_row()
                .items_center()
                .justify_between()
                .gap_6()
                .whitespace_nowrap()
                .text_body()
                .text_color(if disabled { theme::text_dim() } else { theme::text() })
                .when(item.selected && !item.submenu, |d| {
                    d.font_weight(gpui::FontWeight::MEDIUM)
                })
                .when(item.selected && item.submenu, |d| d.bg(theme::hover()))
                .when(highlight == Some(i), |d| d.bg(theme::hover()))
                .when(!disabled, |d| d.cursor_pointer().hover(|s| s.bg(theme::hover())))
                .child(
                    div()
                        .flex()
                        .flex_row()
                        .items_center()
                        .gap_2()
                        .when(checks, |d| {
                            d.child(div().flex_none().w(px(16.)).when(item.selected && !item.submenu, |d| {
                                d.child(icon(Icon::Check, 16., theme::accent_hover()))
                            }))
                        })
                        .child(item.label),
                )
                .when_some(item.hint, |d, h| {
                    d.child(div().text_label().text_color(theme::text_dim()).child(h))
                })
                .when(item.submenu, |d| {
                    d.child(icon(Icon::ChevronRight, 14., theme::text_muted()))
                })
                .when_some(item.tooltip, |d, t| d.tooltip(move |_, cx| tooltip(t.clone(), cx)))
                .on_mouse_down(MouseButton::Left, move |_, w, cx| {
                    cx.stop_propagation();
                    if !disabled {
                        f(w, cx);
                    }
                })
                .into_any_element();
            sep.into_iter().chain(std::iter::once(row))
        }))
}

/// A centered dialog over a dimmed backdrop. The buttons get the `focus`
/// handles in order; `default` is the one Enter presses while none of them
/// is focused.
pub fn modal(
    title: impl Into<SharedString>,
    body: impl IntoElement,
    buttons: Vec<Button>,
    focus: &[gpui::FocusHandle],
    dialog: &gpui::FocusHandle,
    default: usize,
    width: f32,
) -> gpui::Div {
    use gpui::{Animation, AnimationExt};
    let open = Animation::new(std::time::Duration::from_millis(160)).with_easing(gpui::ease_out_quint());
    let default_action = buttons
        .get(default)
        .filter(|b| !b.disabled)
        .and_then(|b| b.on_click.clone());
    let dialog = dialog.clone();
    div()
        .on_action(move |_: &crate::app::ModalConfirm, window, cx| {
            if !dialog.is_focused(window) {
                cx.propagate();
                return;
            }
            if let Some(f) = &default_action {
                let click = ClickEvent::Keyboard(gpui::KeyboardClickEvent {
                    button: gpui::KeyboardButton::Enter,
                    bounds: gpui::Bounds::default(),
                });
                f(&click, window, cx);
            }
        })
        .absolute()
        .top_0()
        .left_0()
        .size_full()
        .flex()
        .items_center()
        .justify_center()
        .bg(theme::backdrop())
        .occlude()
        .child(
            div()
                .w(px(width))
                .max_h(gpui::relative(0.9))
                .flex()
                .flex_col()
                .rounded_xl()
                .bg(theme::panel())
                .border_1()
                .border_color(theme::border_strong())
                .shadow(theme::popover_shadow())
                .text_color(theme::text())
                .overflow_hidden()
                .child(div().px_6().pt_6().pb_3().text_heading().child(title.into()))
                .child(div().px_6().pb_6().flex().flex_col().gap_3().text_body().child(body))
                .child(
                    div()
                        .flex()
                        .flex_row()
                        .justify_end()
                        .gap_2()
                        .px_6()
                        .py_4()
                        .bg(theme::panel_alt())
                        .border_t_1()
                        .border_color(theme::border())
                        .children(buttons.into_iter().zip(focus).map(|(b, f)| b.focus(f))),
                )
                // Fades in, rising a few pixels.
                .with_animation("dialog-open", open, |d, t| d.mt(px(10. * (1. - t))).opacity(t)),
        )
}

/// A horizontal slider for `min..=max`; `on_change` fires while dragging.
/// The `snap` value, marked on the track, catches the handle nearby.
pub fn slider(
    id: &'static str,
    value: i64,
    min: i64,
    max: i64,
    width: f32,
    snap: Option<i64>,
    on_change: impl Fn(i64, &mut Window, &mut App) + 'static,
) -> AnyElement {
    use std::cell::Cell;
    let bounds: Rc<Cell<Option<gpui::Bounds<gpui::Pixels>>>> = Rc::new(Cell::new(None));
    let on_change = Rc::new(on_change);
    let value_at = {
        let bounds = bounds.clone();
        move |x: gpui::Pixels| -> Option<i64> {
            let b = bounds.get()?;
            let t = (f32::from(x - b.left()) / f32::from(b.size.width)).clamp(0.0, 1.0);
            let v = min + ((max - min) as f32 * t).round() as i64;
            // Within 4 px of the snap point.
            let near = |s: i64| ((s - v) as f32).abs() / (max - min).max(1) as f32 * f32::from(b.size.width) <= 4.;
            Some(snap.filter(|&s| near(s)).unwrap_or(v))
        }
    };
    let frac = if max > min {
        (value - min) as f32 / (max - min) as f32
    } else {
        0.0
    };
    let (down_value, move_value) = (value_at.clone(), value_at);
    let (down_change, move_change, key_change) = (on_change.clone(), on_change.clone(), on_change);
    tab_stop(div().id(id))
        .on_key_down(move |e: &gpui::KeyDownEvent, window, cx| {
            let step = if e.keystroke.modifiers.shift { 10 } else { 1 };
            let to = match e.keystroke.key.as_str() {
                "left" | "down" => value - step,
                "right" | "up" => value + step,
                "home" => min,
                "end" => max,
                _ => return,
            };
            cx.stop_propagation();
            key_change(to.clamp(min, max), window, cx);
        })
        .flex_none()
        .w(px(width))
        .h(px(24.))
        .flex()
        .items_center()
        .cursor_pointer()
        .child(
            gpui::canvas(move |b, _, _| bounds.set(Some(b)), |_, _, _, _| {})
                .absolute()
                .size_full(),
        )
        .child(
            div()
                .relative()
                .w_full()
                .h(px(6.))
                .rounded_full()
                .bg(theme::panel_alt())
                .border_1()
                .border_color(theme::border())
                .child(
                    div()
                        .h_full()
                        .rounded_full()
                        .bg(theme::accent())
                        .w(gpui::relative(frac)),
                )
                .when_some(snap.filter(|_| max > min), |d, s| {
                    let at = (s - min) as f32 / (max - min) as f32;
                    d.child(
                        div()
                            .absolute()
                            .top(px(-5.))
                            .left(gpui::relative(at))
                            .ml(px(-0.5))
                            .w(px(1.))
                            .h(px(14.))
                            .bg(theme::text_dim()),
                    )
                })
                .child(
                    div()
                        .absolute()
                        .top(px(-6.))
                        .left(gpui::relative(frac))
                        .ml(px(-8.))
                        .size(px(16.))
                        .rounded_full()
                        .bg(gpui::white())
                        .border_2()
                        .border_color(theme::accent()),
                ),
        )
        .on_mouse_down(MouseButton::Left, move |e, w, cx| {
            w.prevent_default();
            if let Some(v) = down_value(e.position.x) {
                down_change(v, w, cx);
            }
        })
        .on_mouse_move(move |e, w, cx| {
            if e.pressed_button == Some(MouseButton::Left)
                && let Some(v) = move_value(e.position.x)
            {
                move_change(v, w, cx);
            }
        })
        .into_any_element()
}
