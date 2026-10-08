//! The message list and the message being edited.

use super::dialogs::sentence_case;
use super::menus::popup;
use super::*;

impl BadgeApp {
    pub(super) fn sync_input(&mut self, cx: &mut Context<Self>) {
        let color = self.project.card.is_color();
        let m = &self.project.messages[self.current];
        let text = m.text.clone();
        let colors = color.then(|| (0..text.chars().count()).map(|i| m.color_at(i)).collect::<Vec<_>>());
        let tint = self.led_tint();
        self.input.update(cx, |input, cx| {
            input.set_mono_tint(tint);
            input.set_text(&text, colors, cx);
        });
    }

    pub(super) fn on_input_event(
        &mut self,
        input: &Entity<TextInput>,
        ev: &InputEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let InputEvent::Changed = ev;
        let (text, colors) = {
            let i = input.read(cx);
            (i.text().to_string(), i.colors().map(<[LedColor]>::to_vec))
        };
        let m = self.msg();
        let has_drawing = m.source == Source::Bitmap && m.bitmap.used_width() > 0;
        if has_drawing {
            if self.dialog.is_some() {
                return;
            }
            let n = self.current + 1;
            self.ask(
                "Replace your drawing with text?",
                &format!(
                    "Message {n} shows a drawing or image. Typing switches it back to text and the drawing will \
                     be lost. To keep a copy, choose Keep Drawing and use \u{201c}Save image\u{2026}\u{201d} first."
                ),
                "Use Text",
                true,
                window,
                cx,
                move |t, _, cx| t.apply_text(text, colors, cx),
            );
            if let Some(Dialog::Confirm { cancel_label, .. }) = self.dialog.as_mut() {
                *cancel_label = "Keep Drawing".into();
            }
            // Keeping the drawing undoes the keystroke.
            self.pending_cancel = Some(Box::new(|t, _, cx| t.sync_input(cx)));
            return;
        }
        self.apply_text(text, colors, cx);
    }

    /// Empties the current message, asking first if it holds a drawing.
    pub(super) fn clear_message(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let m = self.msg();
        let do_clear = |t: &mut Self, _: &mut Window, cx: &mut Context<Self>| {
            let m = t.msg_mut();
            m.text.clear();
            m.colors.clear();
            m.bitmap.clear();
            m.source = Source::Text;
            t.sync_input(cx);
            t.changed(cx);
            t.set_status(format!("Message {} cleared.", t.current + 1), Severity::Info);
        };
        if m.source == Source::Bitmap && m.bitmap.used_width() > 0 {
            let n = self.current + 1;
            self.ask(
                &format!("Clear message {n}?"),
                "This removes the message's text and its drawing or image. The drawing can't be recovered \
                 unless you saved it with \u{201c}Save image\u{2026}\u{201d}.",
                "Clear Message",
                true,
                window,
                cx,
                do_clear,
            );
        } else {
            do_clear(self, window, cx);
        }
    }

    /// How wide the current message may be: as wide as a message can be,
    /// or as much as the badge's memory has left beside the other ticked
    /// messages, whichever is less.
    pub(super) fn room_for_current(&self) -> usize {
        self.room_for(self.current)
    }

    /// How wide message `i` may be, ticked: as wide as a message can be, or
    /// as much as the badge's memory has left beside the other ticked
    /// messages, whichever is less.
    pub(super) fn room_for(&self, i: usize) -> usize {
        let others: usize = (self.project.messages.iter().enumerate())
            .filter(|&(k, o)| k != i && o.selected)
            .map(|(_, o)| o.bitmap.used_width() / 8)
            .sum();
        let memory = badge_core::protocol::MAX_TOTAL_COLUMNS.saturating_sub(others) * 8;
        memory.min(badge_core::MAX_WIDTH)
    }

    /// Ticks or unticks message `i`; ticking one the badge's memory has no
    /// room for is refused.
    pub(super) fn set_ticked(&mut self, i: usize, ticked: bool, cx: &mut Context<Self>) {
        if ticked && self.project.messages[i].bitmap.used_width() > self.room_for(i) {
            self.set_status(Self::memory_full_message(), Severity::Error);
        } else {
            self.project.messages[i].selected = ticked;
        }
        cx.notify();
    }

    /// The status line for an edit the badge's memory has no room for.
    pub(super) fn memory_full_message() -> String {
        format!(
            "The badge's memory is full: the ticked messages can be {} columns wide together. \
             Untick or shorten another message to make room.",
            badge_core::protocol::MAX_TOTAL_COLUMNS * 8
        )
    }

    /// Why `text` can't become the current message's text, if adding it
    /// would make the message wider than a message can be, or the ticked
    /// messages together more than the badge holds. Removing text is always
    /// allowed, even over a limit (a project from elsewhere can be).
    fn text_overflow(&mut self, text: &str, colors: Option<&[LedColor]>) -> Option<String> {
        let old = self.msg();
        let grows = old.source == Source::Bitmap || text.chars().count() > old.text.chars().count();
        if !grows {
            return None;
        }
        let mut m = old.clone();
        m.text = text.to_string();
        if let Some(c) = colors {
            m.colors = c.to_vec();
        }
        m.source = Source::Text;
        // Rendering stops at the widest a message can be, so a message that
        // reaches it counts as full.
        let width = self.renderer.render_text(&m, self.project.card).used_width();
        if width >= badge_core::MAX_WIDTH {
            return Some(format!(
                "Message {} is full: a message can be at most {} columns wide.",
                self.current + 1,
                badge_core::MAX_WIDTH
            ));
        }
        (width > self.room_for_current()).then(Self::memory_full_message)
    }

    pub(super) fn apply_text(&mut self, text: String, colors: Option<Vec<LedColor>>, cx: &mut Context<Self>) {
        if let Some(why) = self.text_overflow(&text, colors.as_deref()) {
            self.set_status(why, Severity::Error);
            // Puts the text box back to the message's text.
            self.sync_input(cx);
            cx.notify();
            return;
        }
        let was_bitmap = self.msg().source == Source::Bitmap;
        // Switching a drawing back to text is its own step; plain typing groups.
        self.edit_hint = Some(if was_bitmap { EditKind::Other } else { EditKind::Text });
        let m = self.msg_mut();
        m.text = text;
        if let Some(c) = colors {
            m.colors = c;
        }
        m.source = Source::Text;
        if !m.text.is_empty() {
            m.selected = true;
        }
        if was_bitmap {
            self.set_status(
                "Back to text: the message is rendered from its text again.",
                Severity::Info,
            );
        }
        self.sync_input(cx);
        self.changed(cx);
    }

    pub(super) fn select_message(&mut self, i: usize, window: &mut Window, cx: &mut Context<Self>) {
        self.current = i;
        self.preview_step = 0;
        self.sync_input(cx);
        // The keyboard follows the selection only when the list has it.
        if self.message_focus.iter().any(|h| h.is_focused(window)) {
            window.focus(&self.message_focus[i], cx);
        }
        cx.notify();
    }

    /// Moves message `from` to slot `to`, shifting the ones in between.
    pub(super) fn move_message(&mut self, from: usize, to: usize, cx: &mut Context<Self>) {
        let count = self.project.messages.len();
        if from == to || from >= count || to >= count {
            return;
        }
        let m = self.project.messages.remove(from);
        self.project.messages.insert(to, m);
        // The selection stays on the same message.
        self.current = if self.current == from {
            to
        } else if from < self.current && self.current <= to {
            self.current - 1
        } else if to <= self.current && self.current < from {
            self.current + 1
        } else {
            self.current
        };
        self.set_status(format!("Moved message {} to {}", from + 1, to + 1), Severity::Info);
        self.sync_input(cx);
        self.changed(cx);
    }

    /// Copies message `i` into the next empty slot after it.
    pub(super) fn duplicate_message(&mut self, i: usize, cx: &mut Context<Self>) {
        let count = self.project.messages.len();
        let Some(slot) = (1..count)
            .map(|k| (i + k) % count)
            .find(|&k| self.project.messages[k].is_blank())
        else {
            self.set_status("No empty message slot to duplicate into", Severity::Error);
            cx.notify();
            return;
        };
        let copy = self.project.messages[i].clone();
        if copy.selected && copy.bitmap.used_width() > self.room_for(slot) {
            self.set_status(Self::memory_full_message(), Severity::Error);
            cx.notify();
            return;
        }
        self.project.messages[slot] = copy;
        self.current = slot;
        self.set_status(
            format!("Duplicated message {} as message {}", i + 1, slot + 1),
            Severity::Success,
        );
        self.sync_input(cx);
        self.changed(cx);
    }

    /// Puts message `i` on the clipboard (as JSON, so it can be pasted into
    /// another project or another copy of the app).
    pub(super) fn copy_message(&mut self, i: usize, cx: &mut Context<Self>) {
        let wrapped = serde_json::json!({ MESSAGE_CLIPBOARD_KEY: &self.project.messages[i] });
        cx.write_to_clipboard(gpui::ClipboardItem::new_string(wrapped.to_string()));
        self.set_status(format!("Copied message {}", i + 1), Severity::Info);
        cx.notify();
    }

    /// Replaces message `i` with a message from the clipboard.
    pub(super) fn paste_message(&mut self, i: usize, cx: &mut Context<Self>) {
        let pasted = cx
            .read_from_clipboard()
            .and_then(|item| item.text())
            .and_then(|text| serde_json::from_str::<serde_json::Value>(&text).ok())
            .and_then(|mut v| v.get_mut(MESSAGE_CLIPBOARD_KEY).map(serde_json::Value::take))
            .and_then(|v| serde_json::from_value::<badge_core::Message>(v).ok());
        let Some(mut m) = pasted else {
            self.set_status("The clipboard doesn't hold a copied message", Severity::Error);
            cx.notify();
            return;
        };
        m.bitmap.set_height(self.project.card.height());
        let before = self.project.clone();
        self.project.messages[i] = m;
        self.project.normalize();
        self.rerender(i);
        // Kept if it fits, or at least uses no more than what it replaced.
        let cap = badge_core::protocol::MAX_TOTAL_COLUMNS;
        let used = self.project.used_columns();
        if used > cap && used > before.used_columns() {
            self.project = before;
            self.set_status(Self::memory_full_message(), Severity::Error);
            cx.notify();
            return;
        }
        self.current = i;
        self.set_status(format!("Pasted into message {}", i + 1), Severity::Success);
        self.sync_input(cx);
        self.changed(cx);
    }

    /// Empties message `i` (text, drawing and settings).
    pub(super) fn reset_message(&mut self, i: usize, cx: &mut Context<Self>) {
        let mut fresh = badge_core::Message::default();
        fresh.bitmap.set_height(self.project.card.height());
        self.project.messages[i] = fresh;
        self.rerender(i);
        self.set_status(format!("Cleared message {}", i + 1), Severity::Info);
        self.sync_input(cx);
        self.changed(cx);
    }

    /// The ⋯ menu of message `i`.
    fn message_menu_items(&self, i: usize, cx: &mut Context<Self>) -> Vec<MenuItem> {
        let last = self.project.messages.len() - 1;
        let m = if cfg!(target_os = "macos") { "\u{2318}" } else { "Ctrl+" };
        let blank = self.project.messages[i].is_blank();
        vec![
            menu_item(
                "Duplicate",
                false,
                self.act(cx, move |t, _, cx| t.duplicate_message(i, cx)),
            )
            .hint(format!("{m}D"))
            .disabled(blank),
            menu_item("Copy", false, self.act(cx, move |t, _, cx| t.copy_message(i, cx)))
                .hint(format!("{m}C"))
                .disabled(blank),
            menu_item("Paste", false, self.act(cx, move |t, _, cx| t.paste_message(i, cx))).hint(format!("{m}V")),
            menu_item(
                "Move Up",
                false,
                self.act(cx, move |t, _, cx| t.move_message(i, i.saturating_sub(1), cx)),
            )
            .hint(if cfg!(target_os = "macos") {
                "\u{2325}Up"
            } else {
                "Alt+Up"
            })
            .disabled(i == 0)
            .separator(),
            menu_item(
                "Move Down",
                false,
                self.act(cx, move |t, _, cx| t.move_message(i, i + 1, cx)),
            )
            .hint(if cfg!(target_os = "macos") {
                "\u{2325}Down"
            } else {
                "Alt+Down"
            })
            .disabled(i == last),
            menu_item("Clear", false, self.act(cx, move |t, _, cx| t.reset_message(i, cx)))
                .disabled(blank)
                .separator(),
        ]
    }

    /// Messages (index, font name) whose font isn't installed or doesn't fit
    /// this badge.
    pub(super) fn unavailable_fonts(&self) -> Vec<(usize, String)> {
        let height = self.project.card.height();
        self.project
            .messages
            .iter()
            .enumerate()
            .filter(|(_, m)| !m.text.is_empty())
            .filter_map(|(i, m)| match &m.font {
                FontChoice::System { family, .. }
                    if !family.is_empty() && !self.renderer.system.is_installed(family) =>
                {
                    Some((i, family.clone()))
                }
                FontChoice::Pixel(font) if !font.fits(height) => Some((i, font.label().to_string())),
                _ => None,
            })
            .collect()
    }

    pub(super) fn use_default_font(&mut self, cx: &mut Context<Self>) {
        let fixed = self.unavailable_fonts();
        for &(i, _) in &fixed {
            self.project.messages[i].font = FontChoice::Builtin;
            self.rerender(i);
        }
        self.sync_input(cx);
        self.set_status(
            format!(
                "Switched {} to the default font.",
                messages_phrase(fixed.iter().map(|f| f.0))
            ),
            Severity::Success,
        );
        cx.notify();
    }

    /// Rescans the installed fonts in the background and redraws the text if
    /// they changed.
    pub(super) fn refresh_fonts(&mut self, cx: &mut Context<Self>) {
        if self.fonts_refreshing {
            return;
        }
        self.fonts_refreshing = true;
        cx.spawn(async move |this: WeakEntity<Self>, cx| {
            let fonts = cx
                .background_executor()
                .spawn(async { badge_render::SystemFonts::load() })
                .await;
            this.update(cx, |t, cx| {
                t.fonts_refreshing = false;
                if fonts.fingerprint() == t.renderer.system.fingerprint() {
                    return;
                }
                log::info!("installed fonts changed; reloading them");
                t.renderer.system = fonts;
                // Redrawn text isn't an edit or an unsaved change.
                let unchanged = t.project == t.history.baseline.project;
                let was_saved = !t.is_dirty();
                t.rerender_all();
                if unchanged {
                    t.history.baseline.project = t.project.clone();
                }
                if was_saved {
                    t.saved = Some(t.project.clone());
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    /// Under the text box: how wide the message is of the most it can be,
    /// and roughly how many more characters fit (in the message, and in the
    /// badge's memory, whichever runs out first), going by its own average
    /// character width.
    fn render_space_left(&self, m: &badge_core::Message) -> gpui::Div {
        let max = badge_core::MAX_WIDTH;
        let width = m.bitmap.used_width();
        let memory_left = (badge_core::protocol::MAX_TOTAL_COLUMNS * 8).saturating_sub(self.project.used_columns() * 8);
        let room = (max - width.min(max)).min(memory_left);
        let chars = m.text.chars().count();
        let more = (m.source == Source::Text && chars > 0 && width > 0).then(|| {
            let per_char = width as f32 / chars as f32;
            (room as f32 / per_char).floor() as usize
        });
        let text = match more {
            Some(0) => format!("{width} of {max} columns \u{b7} full"),
            Some(n) => format!("{width} of {max} columns \u{b7} about {n} more characters fit"),
            None => format!("{width} of {max} columns"),
        };
        div().text_caption().text_color(theme::text_dim()).child(text)
    }

    pub(super) fn render_message_list(&mut self, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        // In LED columns; the badge stores them 8 to a byte.
        let used = self.project.used_columns() * 8;
        let cap = badge_core::protocol::MAX_TOTAL_COLUMNS * 8;
        let pct = (used as f32 / cap as f32).min(1.0);
        let style = LedStyle {
            look: LedLook {
                cell: 2.5,
                round: false,
                color_card: self.project.card.is_color(),
                tint: self.led_tint(),
                glow: false,
                level: 1.,
                dim: false,
            },
            guides: false,
            display_width: None,
            draw_off: false,
        };
        let rows: Vec<gpui::AnyElement> = (0..self.project.messages.len())
            .map(|i| self.message_row(i, style, cx))
            .collect();
        div()
            .flex()
            .flex_col()
            .w(px(MESSAGE_LIST_W))
            .flex_none()
            .h_full()
            .bg(theme::panel())
            .border_r_1()
            .border_color(theme::border())
            .child(
                div()
                    .px_5()
                    .pt_5()
                    .pb_3()
                    .flex()
                    .flex_col()
                    .gap_0p5()
                    .child(div().text_title().child("Messages"))
                    .child(
                        div()
                            .text_caption()
                            .text_color(theme::text_dim())
                            .child("Ticked messages are sent \u{b7} drag to reorder"),
                    ),
            )
            .child(
                div()
                    .id("message-list")
                    .flex()
                    .flex_col()
                    .flex_1()
                    .overflow_y_scroll()
                    .px_3()
                    .pb_3()
                    .gap_1()
                    .children(rows),
            )
            .child(
                div()
                    .px_5()
                    .py_4()
                    .flex()
                    .flex_col()
                    .gap_2()
                    .border_t_1()
                    .border_color(theme::border())
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .justify_between()
                            .text_label()
                            .child(div().text_color(theme::text_muted()).child("Badge memory"))
                            .child(
                                div()
                                    .text_color(if used > cap {
                                        theme::danger()
                                    } else {
                                        theme::text_muted()
                                    })
                                    .child(format!("{used} / {cap} columns")),
                            ),
                    )
                    .child(
                        div()
                            .h(px(6.))
                            .w_full()
                            .rounded_full()
                            .bg(theme::panel_alt())
                            .border_1()
                            .border_color(theme::border())
                            .child(div().h_full().rounded_full().w(gpui::relative(pct)).bg(if used > cap {
                                theme::danger()
                            } else {
                                theme::accent()
                            })),
                    )
                    .when(used > cap, |d| {
                        d.child(
                            div()
                                .text_caption()
                                .text_color(theme::danger())
                                .child("Too long: the end will be cut off"),
                        )
                    })
                    .when(used <= cap, |d| {
                        d.child(
                            div()
                                .text_caption()
                                .text_color(theme::text_dim())
                                .child(format!("{} columns free", cap - used)),
                        )
                    }),
            )
    }

    /// One message in the list: its number, text and pixels, and a ⋯ menu.
    fn message_row(&mut self, i: usize, style: LedStyle, cx: &mut Context<Self>) -> gpui::AnyElement {
        let m = &self.project.messages[i];
        let current = i == self.current;
        let blank = m.is_blank();
        let is_drawing = !blank && m.source == Source::Bitmap;
        let summary: SharedString = if blank || (m.text.is_empty() && m.source == Source::Text) {
            "Empty".into()
        } else if m.source == Source::Bitmap {
            format!("Drawing ({} px)", m.bitmap.used_width()).into()
        } else {
            m.text.chars().take(40).collect::<String>().into()
        };
        let thumb_w = m.bitmap.used_width().clamp(8, 60);
        let bitmap = m.bitmap.clone();
        let (selected, mode, speed) = (m.selected, m.mode, m.speed);
        let menu_open = self.menu == Some(Menu::MessageActions(i));
        let ghost: SharedString = format!("Message {}: {summary}", i + 1).into();
        let anchors = self.menu_anchors.clone();
        let more = div()
            .relative()
            .flex_none()
            .child(
                gpui::canvas(
                    move |bounds, _, _| {
                        anchors.borrow_mut().insert(Menu::MessageActions(i), bounds);
                    },
                    |_, _, _, _| {},
                )
                .absolute()
                .size_full(),
            )
            .child(
                div()
                    .id(("msg-more", i))
                    .size(px(26.))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded_md()
                    .cursor_pointer()
                    .when(!current && !menu_open, |d| {
                        d.invisible().group_hover("msg-row", |s| s.visible())
                    })
                    .when(menu_open, |d| d.bg(theme::hover()))
                    .hover(|s| s.bg(theme::hover()))
                    .tooltip(|_, cx| widgets::tooltip("Duplicate, copy, paste, move or clear".into(), cx))
                    .child(icon(Icon::More, 16., theme::text_muted()))
                    .on_mouse_down(MouseButton::Left, |_, w, cx| {
                        w.prevent_default();
                        cx.stop_propagation();
                    })
                    .on_click(cx.listener(move |t, _: &ClickEvent, w, cx| {
                        cx.stop_propagation();
                        t.toggle_menu(Menu::MessageActions(i), w, cx);
                    })),
            )
            .when(menu_open, |d| {
                let items = self.message_menu_items(i, cx);
                *self.menu_actions.borrow_mut() = items
                    .iter()
                    .map(|it| (it.disabled, it.submenu, it.on_select.clone()))
                    .collect();
                let rows = items.len() as f32 * widgets::MENU_ROW_H
                    + items.iter().filter(|it| it.separator).count() as f32 * 9.
                    + 10.;
                let (up, room) = self.menu_room(Menu::MessageActions(i), rows);
                let panel = menu_with_highlight("message-menu", items, 220., self.menu_highlight).max_h(px(room));
                d.child(popup(self.menu_keys(panel, cx), true, 26., up))
            });
        div()
            .id(("msg", i))
            .group("msg-row")
            .track_focus(&self.message_focus[i].clone().tab_stop(current))
            .when(widgets::focus_visible(), |d| d.focus(widgets::focus_ring))
            .on_key_down(cx.listener(move |t, e: &gpui::KeyDownEvent, w, cx| {
                let last = t.project.messages.len().saturating_sub(1);
                let mods = e.keystroke.modifiers;
                let key = e.keystroke.key.as_str();
                // The usual keys for a context menu.
                if key == "menu" || (mods.shift && key == "f10") {
                    cx.stop_propagation();
                    widgets::set_focus_visible(true);
                    t.toggle_menu(Menu::MessageActions(i), w, cx);
                    return;
                }
                if mods.secondary() {
                    match key {
                        "c" => t.copy_message(i, cx),
                        "v" => t.paste_message(i, cx),
                        "d" => t.duplicate_message(i, cx),
                        _ => return,
                    }
                    cx.stop_propagation();
                    return;
                }
                if mods.alt && (key == "up" || key == "down") {
                    let to = if key == "up" {
                        i.saturating_sub(1)
                    } else {
                        (i + 1).min(last)
                    };
                    cx.stop_propagation();
                    widgets::set_focus_visible(true);
                    t.move_message(i, to, cx);
                    w.focus(&t.message_focus[to], cx);
                    return;
                }
                let to = match key {
                    "up" => i.saturating_sub(1),
                    "down" => (i + 1).min(last),
                    "home" => 0,
                    "end" => last,
                    _ => return,
                };
                cx.stop_propagation();
                widgets::set_focus_visible(true);
                if to != i {
                    t.select_message(to, w, cx);
                }
            }))
            .flex()
            .flex_col()
            .gap_2()
            .px_2p5()
            .py_2p5()
            .rounded_lg()
            .cursor_pointer()
            .border_1()
            .border_color(if current {
                theme::border_focus()
            } else {
                gpui::transparent_black().into()
            })
            .when(current, |d| d.bg(theme::selected()))
            .when(!current, |d| d.hover(|s| s.bg(theme::hover())))
            .on_click(cx.listener(move |t, _: &ClickEvent, w, cx| t.select_message(i, w, cx)))
            .on_mouse_down(
                MouseButton::Right,
                cx.listener(move |t, _: &MouseDownEvent, w, cx| {
                    t.select_message(i, w, cx);
                    t.toggle_menu(Menu::MessageActions(i), w, cx);
                }),
            )
            .on_drag(DraggedMessage(i), move |_, _, _, cx| {
                cx.new(|_| DragGhost(ghost.clone()))
            })
            .drag_over::<DraggedMessage>(|s, _, _, _| s.border_color(theme::accent()).bg(theme::accent_soft()))
            .on_drop(cx.listener(move |t, d: &DraggedMessage, _, cx| t.move_message(d.0, i, cx)))
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap_2p5()
                    .child(
                        checkbox(("sel", i), "", selected, {
                            let this = cx.entity().downgrade();
                            move |v, _, cx| {
                                cx.stop_propagation();
                                this.update(cx, |t, cx| t.set_ticked(i, v, cx)).ok();
                            }
                        })
                        .mr_1p5(),
                    )
                    .child(
                        div()
                            .flex_none()
                            .min_w(px(22.))
                            .h(px(22.))
                            .px_1()
                            .rounded_full()
                            .flex()
                            .items_center()
                            .justify_center()
                            .bg(if current { theme::accent() } else { theme::panel_alt() })
                            .border_1()
                            .border_color(if current { theme::accent() } else { theme::border() })
                            .text_caption()
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .text_color(if current {
                                gpui::white().into()
                            } else {
                                theme::text_muted()
                            })
                            .child(format!("{}", i + 1)),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .text_body()
                            .overflow_hidden()
                            .whitespace_nowrap()
                            .text_ellipsis()
                            .text_color(if blank { theme::text_dim() } else { theme::text() })
                            .flex()
                            .flex_row()
                            .items_center()
                            .gap_1p5()
                            .when(is_drawing, |d| d.child(icon(Icon::Image, 14., theme::text_muted())))
                            // In a box of its own that can get narrower than
                            // the text, so the text ends in "…" instead of
                            // being cut off.
                            .child(div().min_w_0().truncate().child(summary)),
                    )
                    .child(more),
            )
            .when(!blank, |d| {
                d.child(
                    div()
                        .flex()
                        .flex_row()
                        .items_center()
                        .gap_2()
                        // Under the number: the checkbox, its margin and the gap.
                        .pl(px(18. + 6. + 10.))
                        .min_w_0()
                        // Where the card is too narrow for both, the
                        // thumbnail is cut off on the right, not the effect.
                        .child(
                            div()
                                .min_w_0()
                                .p_1()
                                .rounded_md()
                                .bg(theme::led_board())
                                .overflow_hidden()
                                .child(led_view::led_matrix(bitmap, thumb_w, style, None)),
                        )
                        .child(
                            div()
                                .flex_none()
                                .whitespace_nowrap()
                                .text_caption()
                                .text_color(theme::text_dim())
                                .flex()
                                .flex_row()
                                .items_center()
                                .gap_1()
                                .when_some(mode_icon(mode), |d, i| d.child(icon(i, 12., theme::text_dim())))
                                .child(format!("{} \u{b7} {}", short_mode(mode), speed)),
                        ),
                )
            })
            .into_any_element()
    }

    pub(super) fn render_text_section(&mut self, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let m = self.msg().clone();
        let color_card = self.project.card.is_color();
        let height = self.project.card.height();
        let default_family = self.renderer.system.default_family();

        let mut s = section(format!("Message {}", self.current + 1)).child(
            div()
                .flex()
                .flex_row()
                .gap_2()
                .items_center()
                .child(div().flex_1().child(self.input.clone()))
                // In the row rather than a banner, so the layout doesn't jump.
                .when(m.source == Source::Bitmap && m.bitmap.used_width() > 0, |row| {
                    row.child(
                        warning_chip("drawing-note", Icon::Pencil, "Drawing", CONTROL_H).tooltip(|_, cx| {
                            widgets::tooltip("This message is a drawing. Typing replaces it with text.".into(), cx)
                        }),
                    )
                })
                .child(self.render_icon_picker(cx))
                .child(
                    button("clear-message", "Clear All")
                        .disabled(m.text.is_empty() && m.bitmap.used_width() == 0)
                        .tooltip("Empty this message")
                        .on_click(cx.listener(|t, _: &ClickEvent, w, cx| t.clear_message(w, cx))),
                ),
        );
        s = s.child(self.render_space_left(&m));
        let is_builtin = m.font == FontChoice::Builtin;
        let builtin_name = self.renderer.builtin.display_name(height);
        let current_family = match &m.font {
            FontChoice::System { family, .. } if !family.is_empty() => family.clone(),
            _ => default_family,
        };
        // Built only while open: there can be hundreds of families.
        let font_items: Vec<MenuItem> = if self.menu == Some(Menu::FontFamily) {
            let mut items = vec![
                menu_item("Built-in", false, |_, _| {}).header(),
                menu_item(
                    builtin_name.clone(),
                    is_builtin,
                    self.act(cx, |t, _, cx| {
                        t.msg_mut().font = FontChoice::Builtin;
                        t.changed(cx);
                    }),
                )
                .hint("Default")
                .tooltip(
                    "IBM VGA letters, with other characters from Fusion Pixel (11 and 12 rows) or Unifont (16 rows)",
                ),
            ];
            items.extend(badge_core::PixelFont::ALL.into_iter().map(|font| {
                let fits = font.fits(height);
                menu_item(
                    font.label(),
                    m.font == FontChoice::Pixel(font),
                    self.act(cx, move |t, _, cx| {
                        t.msg_mut().font = FontChoice::Pixel(font);
                        t.changed(cx);
                    }),
                )
                .disabled(!fits)
                .tooltip(if fits {
                    "Every character in this font"
                } else {
                    "For 16-row badges"
                })
            }));
            let groups = [
                (None, self.renderer.system.app_families()),
                (Some("Computer fonts"), self.renderer.system.families()),
            ];
            for (heading, list) in groups.into_iter().filter(|(_, list)| !list.is_empty()) {
                if let Some(heading) = heading {
                    items.push(menu_item(heading, false, |_, _| {}).header().separator());
                }
                items.extend(list.iter().map(|f| {
                    let chosen = f.clone();
                    menu_item(
                        f.clone(),
                        matches!(m.font, FontChoice::System { .. }) && *f == current_family,
                        self.act(cx, move |t, _, cx| {
                            let font = &mut t.msg_mut().font;
                            match font {
                                FontChoice::System { family, .. } => family.clone_from(&chosen),
                                FontChoice::Builtin | FontChoice::Pixel(_) => {
                                    *font = FontChoice::System {
                                        family: chosen.clone(),
                                        size_px: height as u32,
                                        bold: false,
                                        italic: false,
                                    };
                                }
                            }
                            t.changed(cx);
                        }),
                    )
                }));
            }
            items
        } else {
            Vec::new()
        };
        let font_label: SharedString = match &m.font {
            FontChoice::Builtin => builtin_name.into(),
            FontChoice::Pixel(font) => font.label().into(),
            FontChoice::System { .. } => current_family.clone().into(),
        };
        let mut font_row = div()
            .flex()
            .flex_row()
            .flex_wrap()
            .items_center()
            .gap_x(px(SET_GAP))
            .gap_y_2()
            .child(labeled(
                "Font",
                self.dropdown("font-family", Menu::FontFamily, font_label, 260., || font_items, cx),
            ));
        match &m.font {
            FontChoice::Builtin | FontChoice::Pixel(_) => {
                if let FontChoice::Pixel(font) = m.font
                    && !font.fits(height)
                {
                    font_row = font_row.child(
                        warning_chip("font-too-tall", Icon::Warning, "16 rows only", COMPACT_H).tooltip(|_, cx| {
                            widgets::tooltip("This font needs a 16-row badge, so the default font is used; Use Default Font (above) switches to it".into(), cx)
                        }),
                    );
                }
                font_row = font_row.child(checkbox("vertical", "Vertical (Fall)", m.vertical, {
                    let this = cx.entity().downgrade();
                    move |v, _, cx| {
                        this.update(cx, |t, cx| {
                            t.msg_mut().vertical = v;
                            t.changed(cx);
                        })
                        .ok();
                    }
                })
                .tooltip(|_, cx| {
                    widgets::tooltip(
                        "Turns characters 90 degrees for a badge worn upright. Plain English letters, digits and punctuation stay as they are."
                            .into(),
                        cx,
                    )
                }));
            }
            FontChoice::System {
                size_px, bold, italic, ..
            } => {
                let (bold, italic, size_px) = (*bold, *italic, *size_px);
                let missing = !self.renderer.system.is_installed(&current_family);
                let missing_tip: SharedString = format!(
                    "\u{201c}{current_family}\u{201d} isn't installed, so a stand-in font is used; Use Default Font (above) switches to the default font"
                )
                .into();
                font_row = font_row
                    .when(missing, |row| {
                        row.child(
                            warning_chip("font-missing", Icon::Warning, "Not installed", COMPACT_H)
                                .tooltip(move |_, cx| widgets::tooltip(missing_tip.clone(), cx)),
                        )
                    })
                    .child(stepper(
                        "font-size",
                        size_px as i64,
                        badge_core::FONT_SIZE_RANGE.0 as i64,
                        badge_core::FONT_SIZE_RANGE.1 as i64,
                        " px",
                        COMPACT_H,
                        {
                            let this = cx.entity().downgrade();
                            move |v, _, cx| {
                                this.update(cx, |t, cx| {
                                    if let FontChoice::System { size_px, .. } = &mut t.msg_mut().font {
                                        *size_px = v as u32;
                                    }
                                    t.changed(cx);
                                })
                                .ok();
                            }
                        },
                    ))
                    .child(segmented([
                        button("bold", "B").selected(bold).tooltip("Bold").on_click(cx.listener(
                            |t, _: &ClickEvent, _, cx| {
                                if let FontChoice::System { bold, .. } = &mut t.msg_mut().font {
                                    *bold = !*bold;
                                }
                                t.changed(cx);
                            },
                        )),
                        button("italic", "I")
                            .selected(italic)
                            .tooltip("Italic")
                            .on_click(cx.listener(|t, _: &ClickEvent, _, cx| {
                                if let FontChoice::System { italic, .. } = &mut t.msg_mut().font {
                                    *italic = !*italic;
                                }
                                t.changed(cx);
                            })),
                    ]));
            }
        }
        font_row = font_row.child(labeled(
            "Shift up",
            stepper(
                "offset",
                m.top_offset as i64,
                badge_core::TOP_OFFSET_RANGE.0 as i64,
                badge_core::TOP_OFFSET_RANGE.1 as i64,
                "",
                COMPACT_H,
                {
                    let this = cx.entity().downgrade();
                    move |v, _, cx| {
                        this.update(cx, |t, cx| {
                            t.msg_mut().top_offset = v as i32;
                            t.changed(cx);
                        })
                        .ok();
                    }
                },
            ),
        ));
        font_row = font_row.child(labeled(
            "Spacing",
            stepper(
                "spacing",
                m.letter_spacing as i64,
                badge_core::LETTER_SPACING_RANGE.0 as i64,
                badge_core::LETTER_SPACING_RANGE.1 as i64,
                " px",
                COMPACT_H,
                {
                    let this = cx.entity().downgrade();
                    move |v, _, cx| {
                        this.update(cx, |t, cx| {
                            t.msg_mut().letter_spacing = v as i32;
                            t.changed(cx);
                        })
                        .ok();
                    }
                },
            ),
        ));
        s = s.child(font_row);

        if color_card {
            let current = self.input.read(cx).current_color();
            s = s.child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap_1p5()
                    .child(label("Text color"))
                    .child(segmented(LedColor::PALETTE.iter().map(|&c| {
                        button(SharedString::from(format!("tc-{}", c.index())), c.name())
                            .selected(current == c)
                            .swatch(theme::swatch(c))
                            .tooltip("Color for new text, or for selected text")
                            .on_click(cx.listener(move |t, _: &ClickEvent, _, cx| {
                                t.input.update(cx, |i, cx| i.set_current_color(c, cx));
                                cx.notify();
                            }))
                    }))),
            );
        }
        s
    }

    pub(super) fn render_icon_picker(&mut self, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let open = self.menu == Some(Menu::Icons);
        let height = self.project.card.height();
        let style = LedStyle {
            look: LedLook {
                cell: 2.5,
                round: false,
                color_card: false,
                tint: self.led_tint(),
                glow: false,
                level: 1.,
                dim: false,
            },
            guides: false,
            display_width: None,
            draw_off: false,
        };
        div()
            .relative()
            .flex_none()
            .child(
                button("icons", "Icons")
                    .trailing(Icon::ChevronDown)
                    .selected(open)
                    .tooltip("Insert a built-in symbol (written as ~0 … ~f)")
                    .on_click(cx.listener(|t, _: &ClickEvent, w, cx| t.toggle_menu(Menu::Icons, w, cx))),
            )
            .when(open, |d| {
                let grid = div()
                    .id("icon-grid")
                    .occlude()
                    .p_2()
                    .w(px(4. * 56. + 16.))
                    .flex()
                    .flex_row()
                    .flex_wrap()
                    .gap_1()
                    .rounded_md()
                    .bg(theme::panel_alt())
                    .border_1()
                    .border_color(theme::border())
                    .children((0..badge_render::icons::ICON_COUNT).map(|i| {
                        let glyph = badge_render::icons::icon(i, height);
                        let mut fb = Framebuffer::new(glyph.height);
                        for y in 0..glyph.height {
                            for x in 0..glyph.width {
                                if glyph.get(x, y) {
                                    fb.set(x, y, LedColor::RED);
                                }
                            }
                        }
                        let code = format!("~{}", char::from_digit(i as u32, 16).unwrap());
                        div()
                            .id(("icon", i))
                            .w(px(52.))
                            .flex()
                            .flex_col()
                            .items_center()
                            .gap_0p5()
                            .p_1()
                            .rounded_md()
                            .cursor_pointer()
                            .hover(|s| s.bg(theme::hover()))
                            .child(led_view::led_matrix(fb, glyph.width, style, None))
                            .child(div().text_label().text_color(theme::text_dim()).child(code.clone()))
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(move |t, _: &MouseDownEvent, w, cx| {
                                    cx.stop_propagation();
                                    t.menu = None;
                                    let code = code.clone();
                                    t.input.update(cx, |input, cx| input.insert(&code, w, cx));
                                    w.focus(&t.input.focus_handle(cx), cx);
                                    cx.notify();
                                }),
                            )
                    }));
                self.menu_actions.borrow_mut().clear();
                d.child(popup(self.menu_keys(grid, cx), true, CONTROL_H, false))
            })
    }

    /// The message's effect, speed, Flash and Marquee: in the right-hand
    /// panel, or a card of its own in a narrow window.
    /// `stacked` puts Speed and Extras one above the other (the side panel);
    /// otherwise they share a row.
    pub(super) fn render_effect_controls(&mut self, stacked: bool, cx: &mut Context<Self>) -> gpui::Div {
        let m = self.msg().clone();
        let tiles = div()
            .flex()
            .flex_row()
            .flex_wrap()
            .gap_1p5()
            .children(Mode::ALL.iter().map(|&mode| {
                let on = m.mode == mode;
                widgets::tab_stop(div().id(SharedString::from(format!("mode-{}", mode.wire()))))
                    .w(px(EFFECT_TILE_W))
                    .h(px(COMPACT_H))
                    .px_2p5()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap_2()
                    .rounded_lg()
                    .border_1()
                    .text_body()
                    .whitespace_nowrap()
                    .overflow_hidden()
                    .cursor_pointer()
                    .when(on, |d| {
                        d.bg(theme::selected())
                            .border_color(theme::border_focus())
                            .text_color(theme::text())
                    })
                    .when(!on, |d| {
                        d.bg(theme::panel_alt())
                            .border_color(theme::border())
                            .text_color(theme::text_muted())
                            .hover(|s| s.bg(theme::hover()).text_color(theme::text()))
                    })
                    .child(match mode_icon(mode) {
                        Some(i) => {
                            icon(i, 14., if on { theme::accent_hover() } else { theme::text_dim() }).into_any_element()
                        }
                        None => div().flex_none().size(px(14.)).into_any_element(),
                    })
                    .child(mode.label())
                    .on_click(cx.listener(move |t, _: &ClickEvent, _, cx| {
                        t.msg_mut().mode = mode;
                        t.changed(cx);
                    }))
            }));
        let speed = div()
            .flex()
            .flex_col()
            .gap_1p5()
            .child(
                segmented((1..=8u8).map(|s| {
                    button(SharedString::from(format!("speed-{s}")), s.to_string())
                        .grow()
                        .selected(m.speed == s)
                        .tooltip(match s {
                            1 => "Slowest",
                            8 => "Fastest",
                            _ => "Speed (1 is slowest, 8 fastest)",
                        })
                        .on_click(cx.listener(move |t, _: &ClickEvent, _, cx| {
                            t.msg_mut().speed = s;
                            t.changed(cx);
                        }))
                }))
                .w(px(2. * EFFECT_TILE_W + 6.)),
            )
            .child(
                div()
                    .w(px(2. * EFFECT_TILE_W + 6.))
                    .flex()
                    .flex_row()
                    .justify_between()
                    .text_caption()
                    .text_color(theme::text_dim())
                    .child("Slower")
                    .child("Faster"),
            );
        let extras = div()
            .flex()
            .flex_row()
            .flex_wrap()
            .gap_x_5()
            .gap_y_1()
            .child(checkbox("flash", "Flash", m.flash, {
                let this = cx.entity().downgrade();
                move |v, _, cx| {
                    this.update(cx, |t, cx| {
                        t.msg_mut().flash = v;
                        t.changed(cx);
                    })
                    .ok();
                }
            }))
            .child(checkbox("marquee", "Marquee border", m.marquee, {
                let this = cx.entity().downgrade();
                move |v, _, cx| {
                    this.update(cx, |t, cx| {
                        t.msg_mut().marquee = v;
                        t.changed(cx);
                    })
                    .ok();
                }
            }));
        let speed = div().flex().flex_col().gap_2().child(subheading("Speed")).child(speed);
        let extras = div()
            .flex()
            .flex_col()
            .gap_1()
            .child(subheading("Extras"))
            .child(extras);
        let rest = if stacked {
            div().flex().flex_col().gap_4().child(speed).child(extras)
        } else {
            div()
                .flex()
                .flex_row()
                .flex_wrap()
                .items_start()
                .gap_x_10()
                .gap_y_4()
                .child(speed)
                .child(extras)
        };
        div()
            .flex()
            .flex_col()
            .gap_4()
            .child(div().flex().flex_col().gap_2().child(subheading("Effect")).child(tiles))
            .child(rest)
    }
}

/// The message list's width.
const MESSAGE_LIST_W: f32 = 296.;

/// The key a copied message sits under on the clipboard.
const MESSAGE_CLIPBOARD_KEY: &str = "led-badge-studio-message";

/// A message being dragged in the message list.
#[derive(Clone, Copy)]
pub(super) struct DraggedMessage(pub usize);

/// What follows the pointer while a message is dragged.
struct DragGhost(SharedString);

impl Render for DragGhost {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .px_3()
            .py_1p5()
            .rounded_lg()
            .bg(theme::panel())
            .border_1()
            .border_color(theme::border_focus())
            .shadow(theme::popover_shadow())
            .text_body()
            .text_color(theme::text())
            .child(self.0.clone())
    }
}

/// Width of an effect button: two fit side by side in the right-hand panel.
const EFFECT_TILE_W: f32 = 135.;

/// The arrow shown before a scrolling mode's name in the message list.
pub(super) fn mode_icon(m: Mode) -> Option<Icon> {
    match m {
        Mode::Left => Some(Icon::ArrowLeft),
        Mode::Right => Some(Icon::ArrowRight),
        Mode::Up => Some(Icon::ArrowUp),
        Mode::Down => Some(Icon::ArrowDown),
        _ => None,
    }
}

pub(super) fn short_mode(m: Mode) -> &'static str {
    match m {
        Mode::Left => "left",
        Mode::Right => "right",
        Mode::Up => "up",
        Mode::Down => "down",
        Mode::Fixed => "fixed",
        Mode::Animation => "animation",
        Mode::Snow => "snow",
        Mode::Curtain => "curtain",
        Mode::Laser => "laser",
    }
}

/// "message 2", "messages 2 and 5", "messages 1, 3 and 4".
pub(super) fn messages_phrase(indices: impl Iterator<Item = usize>) -> String {
    let n: Vec<String> = indices.map(|i| (i + 1).to_string()).collect();
    match n.as_slice() {
        [one] => format!("message {one}"),
        [rest @ .., last] => format!("messages {} and {last}", rest.join(", ")),
        [] => "no messages".into(),
    }
}

pub(super) fn font_notice_text(fonts: &[(usize, String)]) -> String {
    let which = sentence_case(&messages_phrase(fonts.iter().map(|f| f.0)), false);
    let mut names: Vec<&str> = fonts.iter().map(|f| f.1.as_str()).collect();
    names.sort_unstable();
    names.dedup();
    if names.len() == 1 && fonts.len() == 1 {
        format!(
            "{which} uses \u{201c}{}\u{201d}, which isn't available here, so it's shown in a stand-in font.",
            names[0]
        )
    } else {
        format!(
            "{which} use fonts that aren't available here ({}), so they're shown in a stand-in font.",
            names
                .iter()
                .map(|n| format!("\u{201c}{n}\u{201d}"))
                .collect::<Vec<_>>()
                .join(", ")
        )
    }
}
