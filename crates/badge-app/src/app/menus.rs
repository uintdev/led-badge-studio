//! Dropdown menus and their keyboard handling.

use super::*;

impl BadgeApp {
    pub(super) fn toggle_menu(&mut self, m: Menu, window: &mut Window, cx: &mut Context<Self>) {
        self.menu = if self.menu == Some(m) { None } else { Some(m) };
        // Opened from the keyboard, the first item starts highlighted.
        self.menu_highlight = widgets::focus_visible().then_some(0);
        if self.menu.is_some() {
            self.remember_focus(window, cx);
            window.focus(&self.menu_focus, cx);
        } else {
            self.restore_focus(window, cx);
        }
        if self.menu == Some(Menu::FontFamily) {
            self.refresh_fonts(cx);
        }
        cx.notify();
    }

    /// Closes any menu and returns the keyboard to the message box.
    pub(super) fn close_menu(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.menu = None;
        self.menu_highlight = None;
        self.restore_focus(window, cx);
        cx.notify();
    }

    pub(super) fn menu_move(&mut self, down: bool, cx: &mut Context<Self>) {
        let enabled: Vec<usize> = self
            .menu_actions
            .borrow()
            .iter()
            .enumerate()
            .filter(|(_, (disabled, _, _))| !disabled)
            .map(|(i, _)| i)
            .collect();
        let (Some(&first), Some(&last)) = (enabled.first(), enabled.last()) else {
            return;
        };
        widgets::set_focus_visible(true);
        self.menu_highlight = Some(match self.menu_highlight {
            None => {
                if down {
                    first
                } else {
                    last
                }
            }
            Some(i) if down => enabled.iter().copied().find(|&j| j > i).unwrap_or(first),
            Some(i) => enabled.iter().rev().copied().find(|&j| j < i).unwrap_or(last),
        });
        cx.notify();
    }

    pub(super) fn menu_activate(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let action = self.menu_highlight.and_then(|i| {
            self.menu_actions
                .borrow()
                .get(i)
                .filter(|(disabled, _, _)| !disabled)
                .map(|(_, _, f)| f.clone())
        });
        if let Some(f) = action {
            // Deferred: the action updates this entity too.
            window.defer(cx, move |window, cx| f(window, cx));
        }
    }

    pub(super) fn menu_right(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let submenu = self
            .menu_highlight
            .and_then(|i| self.menu_actions.borrow().get(i).map(|(_, sub, _)| *sub))
            .unwrap_or(false);
        if submenu {
            self.menu_activate(window, cx);
        }
    }

    /// Left or Esc: back from a submenu (Open Recent, Revert To), or close
    /// the menu.
    pub(super) fn menu_back(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(row) = self.menu.and_then(submenu_row) {
            self.menu = Some(Menu::File);
            self.menu_highlight = Some(row);
            cx.notify();
        } else {
            self.close_menu(window, cx);
        }
    }

    /// A dropdown trigger plus (when open) its floating menu.
    pub(super) fn dropdown(
        &self,
        id: &'static str,
        which: Menu,
        text: impl Into<SharedString>,
        width: f32,
        items: impl FnOnce() -> Vec<MenuItem>,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        self.dropdown_with(id, which, text, width, items, |b| b, cx)
    }

    /// `dropdown`, with `button` adding to its button (the device's status
    /// light).
    #[allow(clippy::too_many_arguments)]
    pub(super) fn dropdown_with(
        &self,
        id: &'static str,
        which: Menu,
        text: impl Into<SharedString>,
        width: f32,
        items: impl FnOnce() -> Vec<MenuItem>,
        button_extra: impl FnOnce(widgets::Button) -> widgets::Button,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        // The File menu stays open while a submenu of it is shown.
        let sub = (which == Menu::File)
            .then_some(self.menu)
            .flatten()
            .and_then(|m| submenu_row(m).map(|row| (m, row)));
        let recent = sub.is_some();
        let open = self.menu == Some(which) || recent;
        let text: SharedString = text.into();
        let right = matches!(which, Menu::Device | Menu::Address);
        let compact = matches!(which, Menu::FontFamily | Menu::MonoTint);
        let toggle = cx.listener(move |this, _: &ClickEvent, w, cx| this.toggle_menu(which, w, cx));
        let record = |items: &[MenuItem]| {
            *self.menu_actions.borrow_mut() = items
                .iter()
                .map(|i| (i.disabled, i.submenu, i.on_select.clone()))
                .collect();
        };
        let anchor_h = if compact { COMPACT_H } else { CONTROL_H };
        let mut opens_up = false;
        let panel = |cx: &mut Context<Self>| {
            let main_items = items();
            if !recent {
                record(&main_items);
            }
            // The submenu's row: the rows above it, and the separators above
            // it and them (each 9 px: 1 px and its margins).
            let row_top = sub.map_or(0., |(_, row)| {
                row as f32 * widgets::MENU_ROW_H
                    + main_items.iter().take(row + 1).filter(|i| i.separator).count() as f32 * 9.
            });
            // Below if it fits, else on the roomier side, scrolling if needed.
            let rows = main_items.len() as f32 * widgets::MENU_ROW_H
                + main_items.iter().filter(|i| i.separator).count() as f32 * 9.
                + 2. * 4.
                + 2.;
            let (up, room) = self.menu_room(which, rows.min(MENU_MAX_H));
            opens_up = up;
            let main = menu_with_highlight(
                id,
                main_items,
                width,
                (!recent).then_some(self.menu_highlight).flatten(),
            )
            .max_h(px(room.min(MENU_MAX_H)));
            let Some((sub, _)) = sub else {
                return self.menu_keys(main, cx).into_any_element();
            };
            // Level with the submenu's row.
            div()
                .flex()
                .flex_row()
                .items_start()
                .gap_1()
                .child(main)
                .child(div().mt(px(row_top)).child({
                    let (id, sub_items) = match sub {
                        Menu::Revert => ("revert-menu", self.revert_items(cx)),
                        _ => ("recent-menu", self.recent_items(cx)),
                    };
                    record(&sub_items);
                    self.menu_keys(menu_with_highlight(id, sub_items, 240., self.menu_highlight), cx)
                }))
                .into_any_element()
        };
        let anchors = self.menu_anchors.clone();
        let panel = open.then(|| panel(cx));
        div()
            .relative()
            .flex_none()
            .child(
                gpui::canvas(
                    move |bounds, _, _| {
                        anchors.borrow_mut().insert(which, bounds);
                    },
                    |_, _, _, _| {},
                )
                .absolute()
                .size_full(),
            )
            .child(
                button_extra(button(id, text))
                    .when(which == Menu::File, |b| b.icon(Icon::Menu))
                    .trailing(Icon::ChevronDown)
                    .selected(open)
                    .when(compact, |b| b.compact())
                    .on_click(toggle),
            )
            .when_some(panel, |d, panel| d.child(popup(panel, right, anchor_h, opens_up)))
    }

    /// Where a menu about `height` tall fits next to `which`'s button:
    /// whether it opens upward, and the room it has on that side.
    pub(super) fn menu_room(&self, which: Menu, height: f32) -> (bool, f32) {
        const MARGIN: f32 = 8.;
        let Some(anchor) = self.menu_anchors.borrow().get(&which).copied() else {
            return (false, MENU_MAX_H);
        };
        let below = self.viewport_h - f32::from(anchor.bottom()) - MARGIN;
        let above = f32::from(anchor.top()) - MARGIN;
        if height <= below || below >= above {
            (false, below.max(90.))
        } else {
            (true, above.max(90.))
        }
    }

    pub(super) fn menu_keys(
        &self,
        panel: gpui::Stateful<gpui::Div>,
        cx: &mut Context<Self>,
    ) -> gpui::Stateful<gpui::Div> {
        panel
            .key_context("Menu")
            .track_focus(&self.menu_focus)
            .on_action(cx.listener(|t, _: &MenuUp, _, cx| t.menu_move(false, cx)))
            .on_action(cx.listener(|t, _: &MenuDown, _, cx| t.menu_move(true, cx)))
            .on_action(cx.listener(|t, _: &MenuActivate, w, cx| t.menu_activate(w, cx)))
            .on_action(cx.listener(|t, _: &MenuRight, w, cx| t.menu_right(w, cx)))
            .on_action(cx.listener(|t, _: &MenuLeft, w, cx| t.menu_back(w, cx)))
            .on_action(cx.listener(|t, _: &MenuClose, w, cx| t.menu_back(w, cx)))
    }

    /// Wraps a model update into a menu callback.
    pub(super) fn act(
        &self,
        cx: &mut Context<Self>,
        f: impl Fn(&mut Self, &mut Window, &mut Context<Self>) + 'static,
    ) -> impl Fn(&mut Window, &mut App) + 'static {
        let this = cx.entity().downgrade();
        move |window, cx| {
            this.update(cx, |this, cx| {
                this.menu = None;
                this.menu_highlight = None;
                this.restore_focus(window, cx);
                f(this, window, cx);
                cx.notify();
            })
            .ok();
        }
    }
}

/// The tallest a menu gets before it scrolls.
pub(super) const MENU_MAX_H: f32 = widgets::MENU_MAX_H;

/// Floats `child` just below its (relatively positioned) parent, `anchor_h`
/// tall, or just above it with `up`.
pub(super) fn popup(child: impl IntoElement, align_right: bool, anchor_h: f32, up: bool) -> impl IntoElement {
    use gpui::{Animation, AnimationExt};
    // Fades in while sliding a few pixels away from the button.
    let at = move |d: gpui::Div, shift: f32| {
        let gap = anchor_h + 4. - 6. * (1. - shift);
        d.when(!up, |d| d.top(px(gap))).when(up, |d| d.bottom(px(gap)))
    };
    deferred(
        at(div().absolute(), 1.)
            .when(align_right, gpui::Styled::right_0)
            .when(!align_right, gpui::Styled::left_0)
            .child(child)
            .with_animation(
                "popup-open",
                Animation::new(std::time::Duration::from_millis(140)).with_easing(gpui::ease_out_quint()),
                move |d, t| at(d, t).opacity(t),
            ),
    )
    .with_priority(2)
}

/// The File menu's row a submenu belongs to.
fn submenu_row(menu: Menu) -> Option<usize> {
    match menu {
        Menu::Recent => Some(OPEN_RECENT_ROW),
        Menu::Revert => Some(REVERT_ROW),
        _ => None,
    }
}
