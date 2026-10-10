//! The toolbar and the status bar.

use super::*;

impl BadgeApp {
    pub(super) fn render_toolbar(&mut self, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let card = self.project.card;
        let brightness = self.project.brightness;
        let sending = matches!(self.send, SendState::Sending { .. });
        let device_label = self
            .device
            .as_ref()
            .and_then(|id| self.devices.iter().find(|d| &d.id == id))
            .map_or_else(|| "No badge found".into(), |d| d.label.clone());
        let has_device = self
            .device
            .as_ref()
            .is_some_and(|id| self.devices.iter().any(|d| &d.id == id));

        let file_items = {
            let m = if cfg!(target_os = "macos") { "⌘" } else { "Ctrl+" };
            let mut items = vec![
                menu_item("New", false, self.act(cx, |t, w, cx| t.new_project(&NewProject, w, cx)))
                    .hint(format!("{m}N")),
                menu_item(
                    "Open…",
                    false,
                    self.act(cx, |t, w, cx| t.open_project(&OpenProject, w, cx)),
                )
                .hint(format!("{m}O")),
                menu_item(
                    "Save",
                    false,
                    self.act(cx, |t, w, cx| t.save_project(&SaveProject, w, cx)),
                )
                .hint(format!("{m}S")),
                menu_item(
                    "Save As…",
                    false,
                    self.act(cx, |t, w, cx| t.save_project_as(&SaveProjectAs, w, cx)),
                )
                .hint(format!("{m}Shift+S"))
                .separator(),
                menu_item(
                    "Load What Was Last Sent to the Badge",
                    false,
                    self.act(cx, BadgeApp::load_last_sent),
                )
                .separator(),
                menu_item("Check Project File…", false, self.act(cx, BadgeApp::check_project_file)),
            ];
            items.insert(
                OPEN_RECENT_ROW,
                menu_item(
                    "Open Recent",
                    self.menu == Some(Menu::Recent),
                    self.act(cx, |t, w, cx| {
                        t.menu = Some(Menu::Recent);
                        t.menu_highlight = widgets::focus_visible().then_some(0);
                        w.focus(&t.menu_focus, cx);
                    }),
                )
                .submenu(),
            );
            items.insert(
                REVERT_ROW,
                menu_item(
                    "Revert To",
                    self.menu == Some(Menu::Revert),
                    self.act(cx, |t, w, cx| {
                        t.menu = Some(Menu::Revert);
                        t.menu_highlight = widgets::focus_visible().then_some(0);
                        w.focus(&t.menu_focus, cx);
                    }),
                )
                .submenu(),
            );
            items.push(
                menu_item(
                    "Settings\u{2026}",
                    false,
                    self.act(cx, |t, w, cx| t.open_dialog(Dialog::Settings, w, cx)),
                )
                .hint(format!("{m},"))
                .separator(),
            );
            items.push(
                menu_item(
                    "Keyboard Shortcuts",
                    false,
                    self.act(cx, |t, w, cx| t.open_dialog(Dialog::Shortcuts, w, cx)),
                )
                .hint(format!("{m}/")),
            );
            // USB permissions (udev) only exist on Linux.
            if cfg!(target_os = "linux") {
                items.push(
                    menu_item(
                        "Badge Access Help…",
                        false,
                        self.act(cx, |t, w, cx| {
                            t.open_device_access();
                            t.focus_dialog(w, cx);
                        }),
                    )
                    .separator(),
                );
            }
            let about = menu_item(
                concat!("About ", env!("BRAND_NAME")),
                false,
                self.act(cx, |t, w, cx| {
                    t.dialog = Some(Dialog::About);
                    t.focus_dialog(w, cx);
                }),
            );
            items.push(if cfg!(target_os = "linux") {
                about
            } else {
                about.separator()
            });
            items
        };
        let card_items: Vec<MenuItem> = CardType::ALL
            .iter()
            .map(|&c| menu_item(c.label(), c == card, self.act(cx, move |t, _, cx| t.set_card(c, cx))))
            .collect();
        let brightness_items: Vec<MenuItem> = Brightness::ALL
            .iter()
            .map(|&b| {
                menu_item(
                    b.label(),
                    b == brightness,
                    self.act(cx, move |t, _, _| {
                        t.project.brightness = b;
                    }),
                )
            })
            .collect();
        let device_items: Vec<MenuItem> = if self.devices.is_empty() {
            vec![menu_item(
                "No badge found - plug it in",
                false,
                self.act(cx, |_, _, _| {}),
            )]
        } else {
            self.devices
                .iter()
                .map(|d| {
                    let id = d.id.clone();
                    let sel = self.device.as_ref() == Some(&d.id);
                    let text = if d.kind == DeviceKind::Serial {
                        d.label.clone()
                    } else {
                        format!("{}  ({})", d.label, d.id)
                    };
                    menu_item(
                        text,
                        sel,
                        self.act(cx, move |t, _, cx| {
                            t.device = Some(id.clone());
                            t.device_chosen = true;
                            t.update_access(cx);
                            t.offer_last_sent();
                        }),
                    )
                })
                .collect()
        };
        let address_labels = ["All badges", "Set badge address", "Only badge with address"];
        let address_items: Vec<MenuItem> = (0..3u8)
            .map(|k| {
                menu_item(
                    address_labels[k as usize],
                    self.address_kind == k,
                    self.act(cx, move |t, _, _| t.address_kind = k),
                )
            })
            .collect();

        let group = || div().flex().flex_row().flex_none().items_center().gap_2();
        let device_label: String = if device_label.chars().count() > 26 {
            device_label.chars().take(25).chain(std::iter::once('…')).collect()
        } else {
            device_label
        };
        // On macOS the menu bar has all of these (see `app_menu` in main.rs).
        let left = group()
            .when(!cfg!(target_os = "macos"), |d| {
                d.child(self.dropdown("file-menu", Menu::File, "File", 260., || file_items, cx))
            })
            .child(
                button("save", "Save")
                    .icon(Icon::Save)
                    .tooltip(if cfg!(target_os = "macos") { "Save project (⌘S)" } else { "Save project (Ctrl+S)" })
                    .on_click(cx.listener(|t, _: &ClickEvent, w, cx| t.save_project(&SaveProject, w, cx))),
            )
            .child(
                button("undo", "")
                    .icon(Icon::Undo)
                    .disabled(self.history.undo.is_empty() && self.project == self.history.baseline.project)
                    .tooltip(if cfg!(target_os = "macos") { "Undo (⌘Z)" } else { "Undo (Ctrl+Z)" })
                    .on_click(cx.listener(|t, _: &ClickEvent, w, cx| t.undo(&Undo, w, cx))),
            )
            .child(
                button("redo", "")
                    .icon(Icon::Redo)
                    .disabled(self.history.redo.is_empty())
                    .tooltip(if cfg!(target_os = "macos") { "Redo (⇧⌘Z)" } else { "Redo (Ctrl+Shift+Z)" })
                    .on_click(cx.listener(|t, _: &ClickEvent, w, cx| t.redo(&Redo, w, cx))),
            )
            .child(div().mx_2().child(divider()))
            .child(label("Badge"))
            .child(self.dropdown("card", Menu::Card, card.label(), 180., || card_items, cx))
            .child(
                // An icon, since a font's "i" is never quite centered.
                div()
                    .id("card-help")
                    .group("card-help")
                    .relative()
                    .flex_none()
                    .size(px(20.))
                    .rounded_full()
                    .border_1()
                    .border_color(theme::border())
                    .cursor_default()
                    .hover(|s| s.border_color(theme::border_focus()))
                    .child(
                        div()
                            .absolute()
                            .inset_0()
                            .flex()
                            .items_center()
                            .justify_center()
                            .group_hover("card-help", |s| s.invisible())
                            .child(icon(Icon::Info, 14., theme::text_muted())),
                    )
                    .child(
                        div()
                            .absolute()
                            .inset_0()
                            .flex()
                            .items_center()
                            .justify_center()
                            .invisible()
                            .group_hover("card-help", |s| s.visible())
                            .child(icon(Icon::Info, 14., theme::text())),
                    )
                    .tooltip(|_, cx| {
                        widgets::tooltip(
                            "Count your badge's LED rows (11, 12 or 16) and check if it shows colors. Most common: Mono 11 rows."
                                .into(),
                            cx,
                        )
                    }),
            )
            // A wider gap between the Badge and Brightness sets than inside them.
            .child(label("Brightness").ml(px(SET_GAP - 8.)))
            .child(self.dropdown("brightness", Menu::Brightness, brightness.label(), 100., || brightness_items, cx));
        let right = group()
            .ml_auto()
            .child(
                button("shortcuts", "")
                    .icon(Icon::Keyboard)
                    .tooltip(if cfg!(target_os = "macos") {
                        "Keyboard shortcuts (\u{2318}/)"
                    } else {
                        "Keyboard shortcuts (Ctrl+/)"
                    })
                    .on_click(cx.listener(|t, _: &ClickEvent, w, cx| t.open_dialog(Dialog::Shortcuts, w, cx))),
            )
            .child(
                button("settings", "")
                    .icon(Icon::Settings)
                    .tooltip(if cfg!(target_os = "macos") {
                        "Settings (\u{2318},)"
                    } else {
                        "Settings (Ctrl+,)"
                    })
                    .on_click(cx.listener(|t, _: &ClickEvent, w, cx| t.open_dialog(Dialog::Settings, w, cx))),
            )
            .child(div().mx_2().child(divider()))
            .child(self.dropdown_with(
                "device",
                Menu::Device,
                device_label,
                280.,
                || device_items,
                |b| b.light(self.status_light()),
                cx,
            ))
            .child(self.dropdown(
                "address",
                Menu::Address,
                address_labels[self.address_kind as usize],
                220.,
                || address_items,
                cx,
            ))
            .when(self.address_kind != 0, |d| {
                d.child(stepper(
                    "address-num",
                    self.address as i64,
                    0,
                    MAX_ADDRESS as i64,
                    "",
                    CONTROL_H,
                    {
                        let this = cx.entity().downgrade();
                        move |v, _, cx| {
                            this.update(cx, |t, cx| {
                                t.address = v as u16;
                                cx.notify();
                            })
                            .ok();
                        }
                    },
                ))
            })
            .child(
                button(
                    "send",
                    match self.send {
                        SendState::Sending { .. } => "Sending…",
                        SendState::Sent => "Sent",
                        SendState::Idle => "Send to badge",
                    },
                )
                .when(matches!(self.send, SendState::Sent), |b| b.icon(Icon::Check))
                .when(matches!(self.send, SendState::Idle), |b| b.trailing(Icon::Play))
                .primary()
                .disabled(sending || !has_device)
                .tooltip(if cfg!(target_os = "macos") {
                    "Upload all ticked messages (\u{2318}Return)"
                } else {
                    "Upload all ticked messages (Ctrl+Enter)"
                })
                .on_click(cx.listener(|t, _: &ClickEvent, w, cx| t.send(&SendToBadge, w, cx))),
            );
        div()
            .flex()
            .flex_row()
            .flex_wrap()
            .items_center()
            // A margin rather than a gap: with `ml_auto`, Taffy leaves a
            // trailing gap after the right group.
            .gap_y_2()
            .px_4()
            .py_3()
            .min_h(px(60.))
            .flex_none()
            .bg(theme::panel())
            .border_b_1()
            .border_color(theme::border())
            .child(left.mr_4())
            .child(right)
    }

    /// Whether Settings' Show Again would bring the getting-started card
    /// back: it was closed or a badge was seen, and no badge is plugged in
    /// now (the card never shows while one is).
    pub(super) fn can_show_getting_started_again(&self) -> bool {
        (self.settings.hide_getting_started || self.settings.seen_badge) && !self.usb_badge_connected()
    }

    /// First steps, shown until a badge has been connected (or the card is
    /// closed).
    pub(super) fn render_getting_started(&mut self, cx: &mut Context<Self>) -> Option<gpui::AnyElement> {
        if self.settings.seen_badge || self.settings.hide_getting_started || self.usb_badge_connected() {
            return None;
        }
        let step = |n: usize, title: &'static str, detail: &'static str| {
            div()
                .flex()
                .flex_row()
                .items_start()
                .gap_3()
                .flex_1()
                .flex_basis(px(220.))
                .min_w(px(220.))
                .child(
                    div()
                        .flex_none()
                        .size(px(24.))
                        .rounded_full()
                        .flex()
                        .items_center()
                        .justify_center()
                        .bg(theme::accent())
                        .text_caption()
                        .font_weight(gpui::FontWeight::SEMIBOLD)
                        .text_color(gpui::white())
                        .child(n.to_string()),
                )
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .flex_1()
                        .min_w_0()
                        .gap_0p5()
                        .child(div().text_body().font_weight(gpui::FontWeight::MEDIUM).child(title))
                        .child(div().text_label().text_color(theme::text_muted()).child(detail)),
                )
        };
        let access = if cfg!(target_os = "linux") {
            "The first time, the app asks for your password to allow access."
        } else {
            "It shows up in the toolbar, where it says \u{201c}No badge found\u{201d} now."
        };
        Some(
            div()
                .flex()
                .flex_col()
                .gap_4()
                .p_5()
                .rounded_xl()
                .bg(theme::accent_soft())
                .border_1()
                .border_color(gpui::Hsla::from(theme::border_focus()).opacity(0.5))
                .child(
                    div()
                        .flex()
                        .flex_row()
                        .items_center()
                        .gap_3()
                        .child(icon(Icon::Plug, 20., theme::accent_hover()))
                        .child(div().flex_1().text_title().child("Get started"))
                        .child(
                            button("hide-getting-started", "")
                                .icon(Icon::Close)
                                .compact()
                                .tooltip("Hide (Settings can show it again)")
                                .on_click(cx.listener(|t, _: &ClickEvent, _, cx| {
                                    t.settings.hide_getting_started = true;
                                    t.settings.save();
                                    cx.notify();
                                })),
                        ),
                )
                .child(
                    div()
                        .flex()
                        .flex_row()
                        .flex_wrap()
                        .gap_x_6()
                        .gap_y_4()
                        .child(step(1, "Plug in your badge", access))
                        .child(step(
                            2,
                            "Pick its type",
                            "Under Badge in the toolbar: count its rows of LEDs, and check whether it shows colors.",
                        ))
                        .child(step(
                            3,
                            "Write and send",
                            "Type a message, pick how it moves, then press Send to badge.",
                        )),
                )
                .into_any_element(),
        )
    }

    pub(super) fn render_status(&mut self, _cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let (text, color) = match &self.status {
            Some((t, Severity::Error)) => (t.clone(), theme::danger()),
            Some((t, Severity::Success)) => (t.clone(), theme::success()),
            Some((t, Severity::Info)) => (t.clone(), theme::text_muted()),
            None => ("Ready".into(), theme::text_muted()),
        };
        let progress = match self.send {
            SendState::Sending { done, total } => Some(done as f32 / total.max(1) as f32),
            SendState::Idle | SendState::Sent => None,
        };
        div()
            .flex()
            .flex_row()
            .items_center()
            .gap_3()
            .px_5()
            .h(px(32.))
            .flex_none()
            .bg(theme::panel())
            .border_t_1()
            .border_color(theme::border())
            .text_caption()
            .child(
                div()
                    .flex_1()
                    .text_color(color)
                    .overflow_hidden()
                    .whitespace_nowrap()
                    .text_ellipsis()
                    .child(text),
            )
            .when_some(progress, |d, p| {
                d.child(
                    div()
                        .w(px(200.))
                        .h(px(8.))
                        .rounded_full()
                        .bg(theme::panel_alt())
                        .child(div().h_full().rounded_full().bg(theme::accent()).w(gpui::relative(p))),
                )
            })
            .child(
                div().text_color(theme::text_dim()).child(
                    self.path
                        .as_ref()
                        .map_or_else(|| "Unsaved project".into(), |p| p.display().to_string()),
                ),
            )
    }
}
