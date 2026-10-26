//! Dialogs, their focus handling, and the Licenses and Badge Access contents.

use super::files::file_name;
use super::*;

impl BadgeApp {
    /// Asks for confirmation; `then` runs if the user confirms.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn ask(
        &mut self,
        title: &str,
        detail: &str,
        confirm_label: &str,
        destructive: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
        then: impl FnOnce(&mut Self, &mut Window, &mut Context<Self>) + 'static,
    ) {
        self.menu = None;
        self.replace_dialog(window, cx);
        self.dialog = Some(Dialog::Confirm {
            title: title.to_string().into(),
            detail: detail.to_string().into(),
            confirm_label: confirm_label.to_string().into(),
            cancel_label: "Cancel".into(),
            alternate_label: None,
            destructive,
        });
        self.pending_confirm = Some(Box::new(then));
        self.focus_dialog(window, cx);
        cx.notify();
    }

    /// Like `ask`, with a third choice that runs `alternate`.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn ask_or(
        &mut self,
        title: &str,
        detail: &str,
        confirm_label: &str,
        alternate_label: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
        then: impl FnOnce(&mut Self, &mut Window, &mut Context<Self>) + 'static,
        alternate: impl FnOnce(&mut Self, &mut Window, &mut Context<Self>) + 'static,
    ) {
        self.ask(title, detail, confirm_label, false, window, cx, then);
        if let Some(Dialog::Confirm {
            alternate_label: label, ..
        }) = self.dialog.as_mut()
        {
            *label = Some(alternate_label.to_string().into());
        }
        self.pending_alternate = Some(Box::new(alternate));
    }

    pub(super) fn render_licenses(&mut self, selected: usize, cx: &mut Context<Self>) -> gpui::Div {
        use badge_render::licenses::{APP_LICENSE, FONT_LICENSES, third_party_libraries};
        let libraries = self
            .third_party
            .get_or_insert_with(|| Rc::new(third_party_libraries()))
            .clone();
        // The app's own license first, then the fonts', then the libraries'.
        let docs: Vec<&badge_render::licenses::LicenseDoc> =
            std::iter::once(&APP_LICENSE).chain(FONT_LICENSES).collect();
        let fonts = docs.len();
        let (title, body): (SharedString, String) = if selected < fonts {
            (docs[selected].title.into(), docs[selected].text.trim_end().to_string())
        } else {
            let lib = &libraries[(selected - fonts).min(libraries.len().saturating_sub(1))];
            use std::fmt::Write;
            // Writing to a String can't fail.
            let mut text = String::new();
            let versions = if lib.versions.len() == 1 { "Version" } else { "Versions" };
            let _ = writeln!(text, "{versions}: {}", lib.versions.join(", "));
            let licenses: Vec<&str> = lib.licenses.iter().map(|l| l.name.as_str()).collect();
            let _ = writeln!(text, "License: {}", licenses.join("; "));
            if let Some(repo) = &lib.repository {
                let _ = writeln!(text, "Source: {repo}");
            }
            if !lib.authors.is_empty() {
                text.push_str("\nAuthors:\n");
                for a in &lib.authors {
                    let _ = writeln!(text, "  {a}");
                }
            }
            if !lib.copyrights.is_empty() {
                text.push_str("\nCopyright notices:\n");
                for c in &lib.copyrights {
                    let _ = writeln!(text, "  {c}");
                }
            }
            for lic in &lib.licenses {
                let _ = writeln!(text, "\n--- {} ---\n\n{}", lic.name, lic.text);
            }
            (lib.name.clone().into(), text)
        };
        let item = |id: usize, label: String, detail: Option<String>, active: bool, cx: &mut Context<Self>| {
            div()
                .id(("lic", id))
                .flex_none()
                .flex()
                .flex_row()
                .items_center()
                .gap_2()
                .h(px(widgets::MENU_ROW_H))
                .px_2p5()
                .rounded_md()
                .text_body()
                .cursor_pointer()
                .text_color(if active { theme::text() } else { theme::text_muted() })
                .when(active, |d| d.bg(theme::selected()))
                .when(!active, |d| d.hover(|s| s.bg(theme::hover()).text_color(theme::text())))
                .child(div().flex_1().min_w_0().truncate().child(label))
                .when_some(detail, |d, detail| {
                    d.child(
                        div()
                            .flex_none()
                            .text_label()
                            .text_color(theme::text_dim())
                            .child(detail),
                    )
                })
                .on_click(cx.listener(move |t, _: &ClickEvent, _, cx| {
                    t.dialog = Some(Dialog::Licenses(id));
                    cx.notify();
                }))
        };
        let heading = |text: &'static str| {
            div()
                .flex_none()
                .px_2p5()
                .pt_3()
                .pb_1()
                .text_caption()
                .font_weight(gpui::FontWeight::SEMIBOLD)
                .text_color(theme::text_dim())
                .child(text)
        };
        let mut list = div()
            .id("license-list")
            .flex()
            .flex_col()
            .w(px(280.))
            .flex_none()
            .h_full()
            .overflow_y_scroll()
            .p_1()
            .rounded_lg()
            .bg(theme::panel_alt())
            .border_1()
            .border_color(theme::border())
            .child(heading("This app"))
            .child(item(0, APP_LICENSE.title.to_string(), None, selected == 0, cx))
            .child(heading("Fonts"));
        for (i, doc) in docs.iter().enumerate().skip(1) {
            list = list.child(item(i, doc.title.to_string(), None, i == selected, cx));
        }
        list = list.child(heading("Third-party libraries"));
        for (k, lib) in libraries.iter().enumerate() {
            let id = fonts + k;
            let detail: Vec<String> = lib.licenses.iter().map(|l| short_license_name(&l.name)).collect();
            list = list.child(item(id, lib.name.clone(), Some(detail.join(" / ")), id == selected, cx));
        }
        let mono = self.mono_font();
        let body = div().flex().flex_row().gap_4().h(px(460.)).child(list).child(
            div()
                .flex()
                .flex_col()
                .flex_1()
                .min_w_0()
                .gap_2()
                .child(div().text_title().child(title))
                .child(
                    div()
                        .id(("license-text", selected))
                        .flex_1()
                        .overflow_y_scroll()
                        .p_3()
                        .rounded_lg()
                        .bg(theme::code_bg())
                        .border_1()
                        .border_color(theme::border())
                        .font_family(mono)
                        .text_label()
                        .text_color(theme::text())
                        .child(body),
                ),
        );
        widgets::modal(
            "Licenses",
            body,
            vec![
                button("licenses-back", "About").on_click(cx.listener(|t, _: &ClickEvent, _, cx| {
                    t.dialog = Some(Dialog::About);
                    cx.notify();
                })),
                button("licenses-close", "Close")
                    .primary()
                    .on_click(cx.listener(|t, _: &ClickEvent, w, cx| t.modal_cancel(&ModalCancel, w, cx))),
            ],
            &self.dialog_buttons,
            &self.modal_focus,
            1,
            900.,
        )
    }

    /// A titled box of terminal commands with a Copy button.
    pub(super) fn command_block(
        &self,
        id: &'static str,
        title: &'static str,
        commands: String,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let copied = self.copied == Some(id);
        let text = commands.clone();
        div()
            .flex()
            .flex_col()
            .gap_1()
            .child(div().text_label().text_color(theme::text_dim()).child(title))
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_start()
                    .gap_2()
                    .p_3()
                    .rounded_lg()
                    .bg(theme::code_bg())
                    .border_1()
                    .border_color(theme::border())
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .font_family(self.mono_font())
                            .text_label()
                            .text_color(theme::text())
                            .child(commands),
                    )
                    .child(
                        button(id, if copied { "Copied" } else { "Copy" })
                            .when(copied, |b| b.icon(Icon::Check))
                            .compact()
                            .selected(copied)
                            .tooltip("Copy these commands to the clipboard")
                            .on_click(cx.listener(move |t, _: &ClickEvent, _, cx| {
                                cx.write_to_clipboard(gpui::ClipboardItem::new_string(text.clone()));
                                t.copied = Some(id);
                                t.copied_gen += 1;
                                let generation = t.copied_gen;
                                cx.spawn(async move |this: WeakEntity<Self>, cx| {
                                    cx.background_executor().timer(Duration::from_secs(2)).await;
                                    this.update(cx, |t, cx| {
                                        if t.copied_gen == generation {
                                            t.copied = None;
                                            cx.notify();
                                        }
                                    })
                                    .ok();
                                })
                                .detach();
                                cx.notify();
                            })),
                    ),
            )
    }

    /// An installed monospaced font for command snippets.
    pub(super) fn mono_font(&self) -> SharedString {
        const CANDIDATES: &[&str] = &[
            "DejaVu Sans Mono",
            "Ubuntu Mono",
            "Liberation Mono",
            "Noto Sans Mono",
            "Cascadia Mono",
            "Consolas",
            "Menlo",
            "Courier New",
        ];
        let families = self.renderer.system.families();
        CANDIDATES
            .iter()
            .find(|c| families.iter().any(|f| f == *c))
            .map_or_else(|| "monospace".into(), |c| SharedString::from(*c))
    }

    /// Makes way for another dialog: the open one's choices are forgotten,
    /// and it counts as cancelled (Keep Drawing puts the text box back).
    pub(super) fn replace_dialog(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.pending_confirm = None;
        self.pending_alternate = None;
        if let Some(cancel) = self.pending_cancel.take() {
            cancel(self, window, cx);
        }
    }

    pub(super) fn close_dialog(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.dialog = None;
        self.focused_dialog = None;
        self.pending_repair = None;
        self.pending_confirm = None;
        self.pending_cancel = None;
        self.pending_alternate = None;
        self.restore_focus(window, cx);
        cx.notify();
    }

    /// Remembers where the keyboard was, once per stack of menus and dialogs.
    pub(super) fn remember_focus(&mut self, window: &Window, cx: &App) {
        if self.return_focus.is_none() {
            self.return_focus = window.focused(cx);
        }
    }

    pub(super) fn restore_focus(&mut self, window: &mut Window, cx: &mut App) {
        match self.return_focus.take() {
            Some(h) => window.focus(&h, cx),
            None => window.focus(&self.focus, cx),
        }
    }

    /// Gives the keyboard to the open dialog without focusing a button.
    pub(super) fn focus_dialog(&mut self, window: &mut Window, cx: &mut App) {
        self.remember_focus(window, cx);
        window.focus(&self.modal_focus, cx);
    }

    pub(super) fn accept_dialog(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let action = self.pending_confirm.take();
        self.close_dialog(window, cx);
        if let Some(f) = action {
            f(self, window, cx);
        }
    }

    pub(super) fn accept_alternate(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let action = self.pending_alternate.take();
        self.close_dialog(window, cx);
        if let Some(f) = action {
            f(self, window, cx);
        }
    }

    pub(super) fn modal_cancel(&mut self, _: &ModalCancel, window: &mut Window, cx: &mut Context<Self>) {
        let action = self.pending_cancel.take();
        self.close_dialog(window, cx);
        if let Some(f) = action {
            f(self, window, cx);
        }
    }

    pub(super) fn render_dialog(
        &mut self,
        d: Dialog,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let cancel = cx.listener(|t, _: &ClickEvent, w, cx| t.modal_cancel(&ModalCancel, w, cx));
        let close_button = |label: &'static str, cx: &mut Context<Self>| {
            button("dialog-close", label)
                .primary()
                .on_click(cx.listener(|t, _: &ClickEvent, w, cx| t.modal_cancel(&ModalCancel, w, cx)))
        };
        // Enter does the main action, or Cancel where that's destructive.
        let default_button = match &d {
            Dialog::Confirm { destructive: true, .. } => 0,
            Dialog::Confirm {
                alternate_label: Some(_),
                ..
            } => 2,
            Dialog::Confirm { .. } => 1,
            Dialog::DeviceAccess => 2,
            Dialog::Licenses(_) | Dialog::About => 1,
            Dialog::Settings | Dialog::Shortcuts => 0,
            Dialog::Export => 1,
            Dialog::Notice { .. } => 0,
            // Open Repaired: changes nothing on disk.
            Dialog::Repair { .. } => 1,
        };
        if self.focused_dialog.as_ref() != Some(&d) {
            self.focused_dialog = Some(d.clone());
            self.focus_dialog(window, cx);
        }
        let m = match d {
            Dialog::Confirm {
                title,
                detail,
                confirm_label,
                cancel_label,
                alternate_label,
                destructive,
            } => {
                let confirm = button("dialog-confirm", confirm_label)
                    .on_click(cx.listener(|t, _: &ClickEvent, w, cx| t.accept_dialog(w, cx)));
                let confirm = if destructive {
                    confirm.danger()
                } else {
                    confirm.primary()
                };
                widgets::modal(
                    title,
                    div().text_color(theme::text_muted()).child(detail),
                    std::iter::once(button("dialog-cancel", cancel_label).on_click(cancel))
                        .chain(alternate_label.map(|label| {
                            button("dialog-alternate", label)
                                .on_click(cx.listener(|t, _: &ClickEvent, w, cx| t.accept_alternate(w, cx)))
                        }))
                        .chain([confirm])
                        .collect(),
                    &self.dialog_buttons,
                    &self.modal_focus,
                    default_button,
                    440.,
                )
            }
            Dialog::DeviceAccess => {
                let cmd = format!(
                    "echo '{}' | sudo tee {}\nsudo udevadm control --reload-rules && sudo udevadm trigger",
                    badge_io::UDEV_RULE.trim(),
                    badge_io::UDEV_RULE_PATH
                );
                let remove_cmd = format!(
                    "sudo rm {}\nsudo udevadm control --reload-rules && sudo udevadm trigger",
                    badge_io::UDEV_RULE_PATH
                );
                if self.udev_state.is_none() {
                    self.check_udev_state(cx);
                }
                let state = self.udev_state;
                let (state_text, state_color) = match state {
                    None => ("Checking the access rule…", theme::text_dim()),
                    Some(badge_io::UdevRuleState::Unknown) => (
                        "Access rule: can't be checked from inside this Flatpak",
                        theme::warning(),
                    ),
                    Some(badge_io::UdevRuleState::Installed) => ("Access rule: installed", theme::success()),
                    Some(badge_io::UdevRuleState::Outdated) => (
                        "Access rule: outdated (from an older version); reinstall it",
                        theme::warning(),
                    ),
                    Some(badge_io::UdevRuleState::NotInstalled) => ("Access rule: not installed", theme::warning()),
                    Some(badge_io::UdevRuleState::NotNeeded) => {
                        ("No access rule needed on this system", theme::success())
                    }
                };
                let installed = matches!(
                    state,
                    Some(badge_io::UdevRuleState::Installed | badge_io::UdevRuleState::Outdated)
                );
                let busy = self.setting_up_access || state.is_none();
                // False in a Flatpak that can't run commands on the host.
                let can_set_up = self.can_set_up_access;
                let auto_tip = if can_set_up {
                    "Asks for your password, then installs the rule"
                } else {
                    "Not possible from the Flatpak: run the commands below in a terminal"
                };
                widgets::modal(
                    "Badge Access",
                    div()
                        .flex()
                        .flex_col()
                        .gap_3()
                        .child(
                            div()
                                .flex()
                                .flex_row()
                                .items_center()
                                .gap_2()
                                .child(div().size(px(8.)).rounded_full().bg(state_color))
                                .child(div().font_weight(gpui::FontWeight::SEMIBOLD).child(state_text)),
                        )
                        .child(div().text_color(theme::text_muted()).child(
                            "On Linux, normal users can't write to USB devices until a udev rule allows it. \
                             Install Rule sets it up after asking for your password. Or do it in a terminal \
                             with the commands below, then unplug the badge and plug it back in.",
                        ))
                        .child(self.command_block("udev-install", "Install by hand", cmd, cx))
                        .child(self.command_block("udev-remove", "Remove by hand", remove_cmd, cx))
                        .child(div().text_label().text_color(theme::text_dim()).child(
                            "Older serial badges (shown as \"Serial …\") don't use this rule; they need your account \
                             in the \"dialout\" group instead.",
                        )),
                    vec![
                        button("remove-udev", "Remove Rule")
                            .disabled(busy || !can_set_up || !installed)
                            .tooltip(if can_set_up {
                                "Asks for your password, then deletes the rule"
                            } else {
                                auto_tip
                            })
                            .on_click(cx.listener(|t, _: &ClickEvent, w, cx| t.remove_access(w, cx))),
                        button(
                            "auto-udev",
                            if busy {
                                "Waiting…"
                            } else if installed {
                                "Reinstall Rule"
                            } else {
                                "Install Rule"
                            },
                        )
                        .disabled(busy || !can_set_up)
                        .tooltip(auto_tip)
                        .on_click(cx.listener(|t, _: &ClickEvent, _, cx| t.set_up_access(cx))),
                        close_button("Done", cx),
                    ],
                    &self.dialog_buttons,
                    &self.modal_focus,
                    default_button,
                    580.,
                )
            }
            // Clicking a license only changes the text; keep the focus.
            Dialog::Licenses(selected) => {
                self.focused_dialog = Some(Dialog::Licenses(selected));
                self.render_licenses(selected, cx)
            }
            Dialog::Repair { path, fixes, opening } => {
                let name = file_name(&path);
                let n = fixes.len();
                let (problems, values) = if n == 1 {
                    ("problem", "value")
                } else {
                    ("problems", "values")
                };
                let intro = if opening {
                    format!(
                        "{name} has {n} {values} that the badge doesn't support, so it can't be used as it \
                         is. It can be repaired as shown below. Nothing changes until you choose; \
                         Open Repaired leaves the file itself untouched until you save."
                    )
                } else {
                    format!("Found {n} {problems} in {name}. Each can be repaired as shown below.")
                };
                let cell = |w: f32| div().w(px(w)).flex_none().min_w_0();
                let header = div()
                    .flex()
                    .flex_row()
                    .gap_3()
                    .pb_1()
                    .border_b_1()
                    .border_color(theme::border())
                    .text_label()
                    .font_weight(gpui::FontWeight::SEMIBOLD)
                    .text_color(theme::text_dim())
                    .child(cell(80.).child("Where"))
                    .child(cell(130.).child("Setting"))
                    .child(cell(150.).child("In the file now"))
                    .child(div().flex_1().child("Will become"));
                let rows = fixes.into_iter().map(|f| {
                    div()
                        .flex()
                        .flex_row()
                        .gap_3()
                        .py_1()
                        .border_b_1()
                        .border_color(theme::panel_alt())
                        .text_label()
                        .child(cell(80.).text_color(theme::text_muted()).child(f.place()))
                        .child(cell(130.).text_color(theme::text()).child(f.setting))
                        .child(cell(150.).text_color(theme::danger()).child(f.before))
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .flex()
                                .flex_col()
                                .child(
                                    div()
                                        .flex()
                                        .flex_row()
                                        .items_center()
                                        .gap_1()
                                        .text_color(theme::success())
                                        .child(icon(Icon::ArrowRight, 11., theme::success()))
                                        .child(f.after),
                                )
                                .child(div().text_color(theme::text_dim()).child(f.reason)),
                        )
                });
                let table = div()
                    .id("repair-table")
                    .flex()
                    .flex_col()
                    .max_h(px(300.))
                    .overflow_y_scroll()
                    .child(header)
                    .children(rows);
                let save_label = if opening {
                    "Repair, Save and Open"
                } else {
                    "Repair and Save File"
                };
                let save_tip = if opening {
                    "Fix the file, then open it"
                } else {
                    "Fix the file without opening it"
                };
                widgets::modal(
                    format!("Repair {name}?"),
                    div()
                        .flex()
                        .flex_col()
                        .gap_3()
                        .child(div().text_color(theme::text_muted()).child(intro))
                        .child(table)
                        .child(
                            div()
                                .text_label()
                                .text_color(theme::text_dim())
                                .child(format!("File: {}", path.display())),
                        ),
                    vec![
                        button("repair-cancel", "Cancel").on_click(cancel),
                        button("repair-open", "Open Repaired")
                            .tooltip("Open the repaired project without changing the file")
                            .on_click(cx.listener(|t, _: &ClickEvent, w, cx| t.finish_repair(true, false, w, cx))),
                        button("repair-save", save_label).primary().tooltip(save_tip).on_click(
                            cx.listener(move |t, _: &ClickEvent, w, cx| t.finish_repair(opening, true, w, cx)),
                        ),
                    ],
                    &self.dialog_buttons,
                    &self.modal_focus,
                    default_button,
                    720.,
                )
            }
            Dialog::Notice {
                title,
                detail,
                items,
                error,
            } => {
                let list = div()
                    .id("notice-items")
                    .flex()
                    .flex_col()
                    .gap_1()
                    .max_h(px(260.))
                    .overflow_y_scroll()
                    .text_label()
                    .text_color(theme::text())
                    .children(items.into_iter().map(|(label, value)| {
                        div()
                            .flex()
                            .flex_row()
                            .gap_3()
                            .child(
                                div()
                                    .w(px(96.))
                                    .flex_none()
                                    .font_weight(gpui::FontWeight::SEMIBOLD)
                                    .text_color(theme::text_muted())
                                    .child(label),
                            )
                            .child(div().flex_1().min_w_0().child(value))
                    }));
                widgets::modal(
                    title,
                    div()
                        .flex()
                        .flex_col()
                        .gap_3()
                        .child(
                            div()
                                .when(error, |d| d.font_weight(gpui::FontWeight::SEMIBOLD))
                                .text_color(if error { theme::danger() } else { theme::text_muted() })
                                .child(detail),
                        )
                        .child(list),
                    vec![close_button("OK", cx)],
                    &self.dialog_buttons,
                    &self.modal_focus,
                    default_button,
                    520.,
                )
            }
            Dialog::Export => {
                let format = self.export_format();
                let still = self.settings.export_still;
                let row = |name: &'static str, control: gpui::Div| {
                    div()
                        .flex()
                        .flex_row()
                        .items_center()
                        .gap_3()
                        .child(row_label(name))
                        .child(control)
                };
                let note = if still {
                    "The frame the preview showed when you clicked Export (pause the preview to pick one)."
                } else if self.settings.preview_all {
                    "One full cycle of every ticked message, at the badge's speed, looping."
                } else {
                    "One full cycle of this message, at the badge's speed, looping."
                };
                widgets::modal(
                    "Export Preview",
                    div()
                        .flex()
                        .flex_col()
                        .gap_3()
                        .child(row(
                            "Format",
                            segmented(ExportFormat::ALL.map(|f| {
                                button(SharedString::from(format!("export-{}", f.extension())), f.label())
                                    .selected(f == format)
                                    .on_click(cx.listener(move |t, _: &ClickEvent, _, cx| {
                                        t.settings.export_format = f.extension().into();
                                        t.settings.save();
                                        cx.notify();
                                    }))
                            })),
                        ))
                        .child(row(
                            "Save",
                            segmented([(false, "Animation"), (true, "Current frame")].map(|(s, label)| {
                                button(SharedString::from(format!("export-still-{s}")), label)
                                    .selected(s == still)
                                    .on_click(cx.listener(move |t, _: &ClickEvent, _, cx| {
                                        t.settings.export_still = s;
                                        t.settings.save();
                                        cx.notify();
                                    }))
                            })),
                        ))
                        .child(div().text_label().text_color(theme::text_muted()).child(note)),
                    vec![
                        button("export-cancel", "Cancel").on_click(cancel),
                        button("export-save", "Save\u{2026}").primary().on_click(cx.listener(
                            |t, _: &ClickEvent, w, cx| {
                                t.close_dialog(w, cx);
                                t.export_preview(w, cx);
                            },
                        )),
                    ],
                    &self.dialog_buttons,
                    &self.modal_focus,
                    default_button,
                    420.,
                )
            }
            Dialog::Settings => widgets::modal(
                "Settings",
                self.render_settings(cx),
                vec![close_button("Done", cx)],
                &self.dialog_buttons,
                &self.modal_focus,
                default_button,
                520.,
            ),
            Dialog::Shortcuts => widgets::modal(
                "Keyboard shortcuts",
                render_shortcuts(),
                vec![close_button("Close", cx)],
                &self.dialog_buttons,
                &self.modal_focus,
                default_button,
                640.,
            ),
            Dialog::About => widgets::modal(
                concat!("About ", env!("BRAND_NAME")),
                div()
                    .flex()
                    .flex_col()
                    .gap_2()
                    .child(
                        div()
                            .text_color(theme::text_muted())
                            .child(format!("Version {}", env!("CARGO_PKG_VERSION"))),
                    )
                    .child(div().text_color(theme::text_muted()).child(env!("BRAND_COPYRIGHT")))
                    .child(div().text_color(theme::text_muted()).child(env!("BRAND_HOMEPAGE")))
                    .child(div().text_color(theme::text_muted()).child(
                        "An open re-implementation of the vendor's \"BMP Badge LED V9.18\" software for USB LED \
                         name badges (0416:5020), based on reverse-engineering its \"LED Badge 9.18 (English \
                         Version)\" package. Works on Linux, Windows and macOS.",
                    ))
                    .child(
                        div()
                            .text_label()
                            .text_color(theme::text_dim())
                            .child(format!("Built-in font: {}", self.renderer.builtin.description())),
                    ),
                vec![
                    button("show-licenses", "Licenses…").on_click(cx.listener(|t, _: &ClickEvent, _, cx| {
                        t.dialog = Some(Dialog::Licenses(0));
                        cx.notify();
                    })),
                    close_button("Close", cx),
                ],
                &self.dialog_buttons,
                &self.modal_focus,
                default_button,
                480.,
            ),
        };
        m.key_context("Modal")
            .track_focus(&self.modal_focus)
            .on_action(cx.listener(Self::modal_cancel))
            .on_action(cx.listener(|t, _: &FocusNext, window, cx| t.dialog_tab(true, window, cx)))
            .on_action(cx.listener(|t, _: &FocusPrev, window, cx| t.dialog_tab(false, window, cx)))
    }

    /// Passes on the answer of a system file dialog (Open, Save…), keeping the
    /// preview paused until it comes. The dialog takes a moment to open, and
    /// the window counts as active until it has, so the animation would
    /// stall and then jump ahead. GPUI opens the dialog after the current
    /// handler returns, so the pause starts first.
    pub(super) fn during_system_dialog<T: 'static>(
        &mut self,
        answer: futures::channel::oneshot::Receiver<T>,
        cx: &mut Context<Self>,
    ) -> futures::channel::oneshot::Receiver<T> {
        self.system_dialogs += 1;
        let (tx, rx) = futures::channel::oneshot::channel();
        cx.spawn(async move |this: WeakEntity<Self>, cx| {
            let answer = answer.await;
            this.update(cx, |t, _| t.system_dialogs = t.system_dialogs.saturating_sub(1))
                .ok();
            // A dialog that never answered drops `tx`: canceled, as before.
            if let Ok(answer) = answer {
                let _ = tx.send(answer);
            }
        })
        .detach();
        rx
    }

    /// Opens `dialog` over everything else.
    pub(super) fn open_dialog(&mut self, dialog: Dialog, window: &mut Window, cx: &mut Context<Self>) {
        self.menu = None;
        self.dialog = Some(dialog);
        self.focus_dialog(window, cx);
        cx.notify();
    }

    /// The Settings dialog's contents.
    fn render_settings(&mut self, cx: &mut Context<Self>) -> gpui::Div {
        use crate::settings::Appearance;
        let chosen = self.settings.appearance;
        let system = match self.system_dark {
            Some(true) => "Your system is set to dark.",
            Some(false) => "Your system is set to light.",
            None => "Your system doesn't say, so the app is dark.",
        };
        let row = |title: &'static str, caption: SharedString, control: gpui::AnyElement| {
            div()
                .flex()
                .flex_row()
                .items_start()
                .justify_between()
                .gap_6()
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap_0p5()
                        .min_w_0()
                        .child(div().text_body().font_weight(gpui::FontWeight::MEDIUM).child(title))
                        .child(div().text_label().text_color(theme::text_muted()).child(caption)),
                )
                .child(div().flex_none().child(control))
        };
        div()
            .flex()
            .flex_col()
            .gap_5()
            .child(row(
                "Appearance",
                if chosen == Appearance::System {
                    format!("Follows the system. {system}").into()
                } else {
                    "Light or dark, whatever the system is set to.".into()
                },
                segmented(Appearance::ALL.iter().map(|&a| {
                    button(SharedString::from(format!("appearance-{}", a.label())), a.label())
                        .selected(chosen == a)
                        .on_click(cx.listener(move |t, _: &ClickEvent, w, cx| {
                            t.settings.appearance = a;
                            t.settings.save();
                            t.apply_theme(Some(w), cx);
                        }))
                }))
                .into_any_element(),
            ))
            .child(div().h(px(1.)).bg(theme::border()))
            .child(row(
                "Reopen the last project",
                "Opens the project you had open when the app starts.".into(),
                checkbox("reopen-last", "", self.settings.reopen_last, {
                    let this = cx.entity().downgrade();
                    move |v, _, cx| {
                        this.update(cx, |t, cx| {
                            t.settings.reopen_last = v;
                            t.settings.save();
                            cx.notify();
                        })
                        .ok();
                    }
                })
                .into_any_element(),
            ))
            .child(row(
                "Getting started",
                if self.usb_badge_connected() {
                    "The steps shown above the message until a badge has been connected. \
                     Hidden while a badge is connected."
                        .into()
                } else {
                    "The steps shown above the message until a badge has been connected.".into()
                },
                button("show-getting-started", "Show Again")
                    .compact()
                    .disabled(!self.can_show_getting_started_again())
                    .on_click(cx.listener(|t, _: &ClickEvent, _, cx| {
                        t.settings.hide_getting_started = false;
                        t.settings.seen_badge = false;
                        t.settings.save();
                        cx.notify();
                    }))
                    .into_any_element(),
            ))
    }

    /// Tab within a dialog's own controls.
    pub(super) fn dialog_tab(&mut self, forward: bool, window: &mut Window, cx: &mut Context<Self>) {
        widgets::set_focus_visible(true);
        for _ in 0..512 {
            if forward {
                window.focus_next(cx)
            } else {
                window.focus_prev(cx)
            }
            if self.modal_focus.contains_focused(window, cx) && !self.modal_focus.is_focused(window) {
                break;
            }
        }
        cx.notify();
    }
}

/// `text` capitalized, with a closing period when `period` is set.
pub(super) fn sentence_case(text: &str, period: bool) -> String {
    let mut chars = text.trim().chars();
    let mut out: String = match chars.next() {
        Some(c) => c.to_uppercase().chain(chars).collect(),
        None => String::new(),
    };
    if period && !out.is_empty() && !out.ends_with(['.', '!', '?']) {
        out.push('.');
    }
    out
}

/// A compact form of a license name for the Licenses list.
pub(super) fn short_license_name(name: &str) -> String {
    const SHORT: &[(&str, &str)] = &[
        ("Apache License 2.0", "Apache-2.0"),
        ("MIT License", "MIT"),
        ("Mozilla Public License 2.0", "MPL-2.0"),
        ("BSD 3-Clause \"New\" or \"Revised\" License", "BSD-3-Clause"),
        ("BSD 2-Clause \"Simplified\" License", "BSD-2-Clause"),
        ("ISC License", "ISC"),
        ("zlib License", "Zlib"),
        ("Unicode License v3", "Unicode-3.0"),
        ("Boost Software License 1.0", "BSL-1.0"),
        ("Creative Commons Zero v1.0 Universal", "CC0-1.0"),
        ("The Unlicense", "Unlicense"),
    ];
    SHORT
        .iter()
        .find(|(long, _)| *long == name)
        .map_or_else(|| name.to_string(), |(_, short)| short.to_string())
}

/// The Keyboard shortcuts dialog's contents, in two columns of groups.
fn render_shortcuts() -> gpui::Div {
    let m = if cfg!(target_os = "macos") { "\u{2318}" } else { "Ctrl+" };
    let shift = if cfg!(target_os = "macos") {
        "\u{21e7}"
    } else {
        "Shift+"
    };
    let alt = if cfg!(target_os = "macos") { "\u{2325}" } else { "Alt+" };
    let groups: Vec<(&'static str, Vec<(String, &'static str)>)> = vec![
        (
            "Projects",
            vec![
                (format!("{m}N"), "New project"),
                (format!("{m}O"), "Open"),
                (format!("{m}S"), "Save"),
                (format!("{m}{shift}S"), "Save as"),
                (format!("{m}Q"), "Quit"),
            ],
        ),
        (
            "Editing",
            vec![
                (format!("{m}Z"), "Undo"),
                (format!("{m}{shift}Z / {m}Y"), "Redo"),
                (format!("{m}Return"), "Send to the badge"),
                ("Right-click".into(), "Cut, copy and paste text"),
            ],
        ),
        (
            "Message list",
            vec![
                ("Up / Down".into(), "Select a message"),
                (format!("{alt}Up / {alt}Down"), "Move it up or down"),
                (format!("{m}D"), "Duplicate it"),
                (format!("{m}C / {m}V"), "Copy it, paste over it"),
                ("Shift+F10".into(), "Its actions menu"),
            ],
        ),
        (
            "Pixels",
            vec![
                ("Left-click".into(), "Paint (or erase)"),
                ("Right-click".into(), "Erase"),
            ],
        ),
        (
            "Preview window",
            vec![
                (crate::preview_window::fullscreen_key().into(), "Full screen"),
                ("Esc".into(), "Leave full screen, or close"),
            ],
        ),
        (
            "Everywhere",
            vec![
                ("Tab".into(), "Next control"),
                ("Return / Space".into(), "Press the focused control"),
                ("Esc".into(), "Close a menu or dialog"),
                (format!("{m},"), "Settings"),
                (format!("{m}/ or F1"), "These shortcuts"),
            ],
        ),
    ];
    let key = |k: String| {
        div()
            .flex_none()
            .px_1p5()
            .py_0p5()
            .rounded_md()
            .bg(theme::panel_alt())
            .border_1()
            .border_color(theme::border_strong())
            .text_label()
            .text_color(theme::text())
            .child(k)
    };
    let group = |(title, rows): (&'static str, Vec<(String, &'static str)>)| {
        div()
            .flex()
            .flex_col()
            .gap_1p5()
            .child(widgets::subheading(title.to_string()))
            .children(rows.into_iter().map(|(k, what)| {
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .justify_between()
                    .gap_4()
                    .child(div().text_body().text_color(theme::text()).child(what))
                    .child(key(k))
            }))
    };
    let mut left = div().flex().flex_col().flex_1().gap_5();
    let mut right = div().flex().flex_col().flex_1().gap_5();
    for (n, g) in groups.into_iter().enumerate() {
        if n % 2 == 0 {
            left = left.child(group(g))
        } else {
            right = right.child(group(g))
        }
    }
    div().flex().flex_row().gap_8().child(left).child(right)
}
