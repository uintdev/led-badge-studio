//! The Preview section: the timer, LED look, Pop Out and Export.

use super::*;

impl BadgeApp {
    pub(super) fn start_preview_timer(window: &mut Window, cx: &mut Context<Self>) {
        // Advance by elapsed real time rather than per tick, so the preview
        // keeps the badge's pace however long a redraw takes.
        let frame = Duration::from_millis(if crate::gpu::is_software() { 60 } else { 16 });
        // While nothing plays (paused, the window in the background, a
        // dialog open), check back less often, so the app doesn't keep
        // waking the computer for nothing.
        let idle = Duration::from_millis(100);
        let mut last = std::time::Instant::now();
        let mut pending_us: u64 = 0;
        let mut was_playing = false;
        cx.spawn_in(window, async move |this, cx| {
            loop {
                let now = std::time::Instant::now();
                // Time spent idle doesn't count, so resuming doesn't jump ahead.
                let elapsed_us = if was_playing {
                    now.duration_since(last).as_micros() as u64
                } else {
                    0
                };
                last = now;
                let Ok(playing) = this.update_in(cx, |this, window, cx| {
                    let target = this.preview_target();
                    let step_us = preview::step_interval_us(&this.preview_params(target));
                    // Paused while a dialog is open (each tick redraws the
                    // window), the app's own or the system's.
                    let pop_out_active = this
                        .preview_window
                        .is_some_and(|w| cx.active_window() == Some(w.into()));
                    if !(this.playing && (window.is_window_active() || pop_out_active))
                        || this.dialog.is_some()
                        || this.system_dialogs > 0
                    {
                        pending_us = 0;
                        return false;
                    }
                    // Cap the catch-up after a stall.
                    pending_us = (pending_us + elapsed_us).min(step_us * 64);
                    let steps = (pending_us / step_us) as usize;
                    pending_us %= step_us;
                    if steps == 0 {
                        return true;
                    }
                    this.preview_msg = target;
                    this.preview_step = this.preview_step.wrapping_add(steps);
                    let list = this.preview_playlist();
                    if list.len() > 1 && this.preview_step >= this.preview_cycle(target) {
                        let pos = list.iter().position(|&i| i == target).unwrap_or(0);
                        this.preview_msg = list[(pos + 1) % list.len()];
                        this.preview_step = 0;
                    }
                    // Redraw only when the picture changes; many steps look the same.
                    let shown = (this.preview_target(), this.preview_frame().0);
                    if this.last_preview.as_ref() != Some(&shown) {
                        this.last_preview = Some(shown);
                        cx.notify();
                    }
                    true
                }) else {
                    break;
                };
                was_playing = playing;
                cx.background_executor().timer(if playing { frame } else { idle }).await;
            }
        })
        .detach();
    }

    pub(super) fn preview_playlist(&self) -> Vec<usize> {
        if self.settings.preview_all {
            let list: Vec<usize> = (0..MESSAGE_COUNT)
                .filter(|&i| self.project.messages[i].has_content())
                .collect();
            if !list.is_empty() {
                return list;
            }
        }
        vec![self.current]
    }

    pub(super) fn preview_target(&self) -> usize {
        let list = self.preview_playlist();
        if list.contains(&self.preview_msg) {
            self.preview_msg
        } else {
            list[0]
        }
    }

    pub(super) fn preview_params(&self, i: usize) -> PreviewParams {
        let m = &self.project.messages[i];
        PreviewParams {
            mode: m.mode,
            flash: m.flash,
            marquee: m.marquee,
            width: self.display_width(),
            speed: m.speed,
        }
    }

    pub(super) fn preview_cycle(&self, i: usize) -> usize {
        preview::cycle_len(&self.project.messages[i].bitmap, &self.preview_params(i))
    }

    /// The LED color of mono badges: the project's, else the default.
    pub(super) fn led_tint(&self) -> LedColor {
        self.project.led_color.unwrap_or(self.settings.mono_tint)
    }

    /// Gives a mono project without an LED color the default one.
    pub(super) fn fill_led_color(&mut self) {
        if !self.project.card.is_color() && self.project.led_color.is_none() {
            self.project.led_color = Some(self.settings.mono_tint);
        }
    }

    /// How LEDs look everywhere they're drawn (preview, Pixels canvas,
    /// exports), with `cell` px LEDs.
    pub(super) fn led_look(&self, cell: f32) -> LedLook {
        LedLook {
            cell,
            round: self.settings.round_leds,
            color_card: self.project.card.is_color(),
            tint: self.led_tint(),
            glow: self.settings.preview_glow,
            level: self.brightness_level(),
            dim: self.settings.preview_brightness,
        }
    }

    pub(super) fn brightness_level(&self) -> f32 {
        self.project.brightness.fraction()
    }

    pub(super) fn display_width(&self) -> usize {
        if self.settings.preview_width > 0 {
            let (min, max) = crate::settings::PREVIEW_WIDTHS;
            self.settings.preview_width.clamp(min, max)
        } else {
            preview::display_width(self.project.card)
        }
    }

    /// Opens the preview in its own window, or brings it to the front.
    pub(super) fn pop_out_preview(&mut self, cx: &mut Context<Self>) {
        let existing = self.preview_window;
        let app = cx.entity();
        // Deferred: the new window's first draw reads this entity, which
        // can't happen while it's being updated.
        cx.defer(move |cx| {
            let mut bounds = None;
            if let Some(w) = existing {
                // Wayland ignores requests to raise a window, so on Linux it's
                // reopened in the same place instead.
                if cfg!(target_os = "linux") {
                    bounds = w
                        .update(cx, |_, window, _| {
                            let b = window.window_bounds();
                            window.remove_window();
                            b
                        })
                        .ok();
                } else if w.update(cx, |_, window, _| window.activate_window()).is_ok() {
                    return;
                }
            }
            let handle = crate::preview_window::PreviewWindow::open(app.clone(), bounds, cx);
            app.update(cx, |t, cx| {
                t.preview_window = handle;
                cx.notify();
            });
        });
    }

    /// The export format chosen last.
    pub(super) fn export_format(&self) -> ExportFormat {
        ExportFormat::ALL
            .into_iter()
            .find(|f| f.extension() == self.settings.export_format)
            .unwrap_or(ExportFormat::Gif)
    }

    /// Preview → Export: keeps the current frame in case a still is chosen.
    pub(super) fn open_export(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let (frame, cols, _) = self.preview_frame();
        self.export_frame = Some((frame, cols));
        self.dialog = Some(Dialog::Export);
        self.focus_dialog(window, cx);
        cx.notify();
    }

    /// Saves one full cycle of the preview, or the frame kept by `open_export`.
    pub(super) fn export_preview(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        use badge_render::preview_export::{Look, Part, save_animation, save_still};
        let format = self.export_format();
        let still = self.settings.export_still;
        let messages: Vec<usize> = if self.settings.preview_all {
            self.preview_playlist()
        } else {
            vec![self.current]
        };
        let parts: Vec<(Framebuffer, preview::PreviewParams)> = messages
            .iter()
            .map(|&i| (self.project.messages[i].bitmap.clone(), self.preview_params(i)))
            .filter(|(fb, _)| fb.used_width() > 0)
            .collect();
        let frame = self.export_frame.take();
        if !still && parts.is_empty() {
            self.set_status("Nothing to save: the message has no pixels.", Severity::Error);
            cx.notify();
            return;
        }
        let look: Look = self.led_look(10.);
        let ext = format.extension();
        let name = match (still, messages.len() > 1) {
            (true, _) => format!("message{}-frame.{ext}", self.preview_target() + 1),
            (false, true) => format!("messages.{ext}"),
            (false, false) => format!("message{}.{ext}", messages[0] + 1),
        };
        let rx = cx.prompt_for_new_path(&self.default_dir(), Some(&name));
        let rx = self.during_system_dialog(rx, cx);
        cx.spawn_in(window, async move |this, cx| {
            let Ok(Ok(Some(mut path))) = rx.await else { return };
            if path.extension().is_none() {
                path.set_extension(ext);
            }
            // A typed extension of another format picks that format.
            let format = ExportFormat::from_path(&path).unwrap_or(format);
            let what = format!("{}{}", format.label(), if still { " image" } else { " animation" });
            this.update(cx, |t, cx| {
                t.set_status(format!("Saving the {what}\u{2026}"), Severity::Info);
                cx.notify();
            })
            .ok();
            let target = path.clone();
            let result = cx
                .background_executor()
                .spawn(async move {
                    match frame {
                        Some((fb, cols)) if still => save_still(&fb, cols, &look, format, &target),
                        _ => {
                            let refs: Vec<Part> = parts
                                .iter()
                                .map(|(bitmap, params)| Part {
                                    bitmap,
                                    params: *params,
                                })
                                .collect();
                            save_animation(&refs, &look, format, &target)
                        }
                    }
                })
                .await;
            this.update(cx, |t, cx| {
                match result {
                    Ok(()) => t.set_status(format!("Saved {}", path.display()), Severity::Success),
                    Err(e) => t.set_status(format!("Couldn't save the {what}: {e:#}"), Severity::Error),
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    pub(super) fn close_pop_out(&mut self, cx: &mut Context<Self>) {
        if let Some(w) = self.preview_window {
            // Deferred: closing the window updates this entity.
            cx.defer(move |cx| {
                w.update(cx, |_, window, _| window.remove_window()).ok();
            });
        }
    }

    /// The Preview card; `with_options` adds the display options, which are
    /// otherwise in the right-hand panel.
    pub(super) fn render_preview_section(
        &mut self,
        window: &Window,
        with_options: bool,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let target = self.preview_target();
        let playlist = self.preview_playlist();
        let (frame, width, mut style) = self.preview_frame();
        // Wide displays get smaller LEDs; below the smallest size it scrolls.
        let rows = self.project.card.height();
        if let Some(room) = self.preview_room.get() {
            style.look.cell =
                led_view::fit_cell(PREVIEW_CELL, (room, f32::INFINITY), width, rows, window.scale_factor());
        }
        let scrolls = self
            .preview_room
            .get()
            .is_some_and(|room| width as f32 * style.look.cell > room + 0.5);
        let measure = {
            let room = self.preview_room.clone();
            move |bounds: Bounds<Pixels>, _: &mut Window, _: &mut App| {
                let w = (f32::from(bounds.size.width) - 2. * BOARD_PAD).max(1.);
                (w, room.clone())
            }
        };
        let popped_out = self.preview_window.is_some();
        let preview_all = self.settings.preview_all;
        let playing_label = if preview_all && playlist.len() > 1 {
            let pos = playlist.iter().position(|&i| i == target).unwrap_or(0) + 1;
            format!("Message {} \u{b7} {} of {}", target + 1, pos, playlist.len())
        } else if preview_all {
            "Only one ticked message".to_string()
        } else {
            format!("Message {}", target + 1)
        };
        let set_all = |all: bool| {
            move |t: &mut Self, _: &ClickEvent, _: &mut Window, cx: &mut Context<Self>| {
                t.settings.preview_all = all;
                t.settings.save();
                t.preview_step = 0;
                t.preview_msg = t.current;
                cx.notify();
            }
        };
        let group = || div().flex().flex_row().flex_none().items_center().gap_2();
        // A margin rather than a gap: with `ml_auto`, Taffy leaves a trailing
        // gap after the right group.
        let split_row = || div().flex().flex_row().flex_wrap().items_center().gap_y_2();
        let board_h = PREVIEW_CELL * rows as f32 + 2. * BOARD_PAD;
        let mut card =
            widgets::card()
                .child(
                    split_row()
                        .child(
                            group()
                                .mr_4()
                                .gap_3()
                                .child(div().text_title().child("Preview"))
                                .child(div().text_label().text_color(theme::text_muted()).child(playing_label)),
                        )
                        .child(
                            group().ml_auto().child(segmented([
                                button("preview-one", "This message")
                                    .selected(!preview_all)
                                    .tooltip("Preview the message you're editing")
                                    .on_click(cx.listener(set_all(false))),
                                button("preview-all", "All ticked")
                                    .selected(preview_all)
                                    .tooltip("Play every ticked message in turn")
                                    .on_click(cx.listener(set_all(true))),
                            ])),
                        ),
                )
                .child(
                    // The badge: its plastic body around the LED board.
                    div()
                        .w_full()
                        .p(px(BADGE_RIM))
                        .rounded(px(BADGE_RIM + 8.))
                        .bg(theme::badge_body())
                        .border_1()
                        .border_color(theme::badge_edge())
                        .shadow(vec![gpui::BoxShadow {
                            color: theme::shadow().opacity(0.6),
                            offset: gpui::point(px(0.), px(1.)),
                            blur_radius: px(3.),
                            spread_radius: px(0.),
                            inset: false,
                        }])
                        .child(
                            div()
                                .id("preview-box")
                                .relative()
                                .w_full()
                                .min_w_0()
                                // Fixed height, so the layout doesn't move with the width.
                                .h(px(board_h))
                                .p(px(BOARD_PAD))
                                .rounded_lg()
                                .bg(rgb_black())
                                .flex()
                                .items_center()
                                // Not centered when it scrolls, or the left part would be
                                // out of reach.
                                .when(!scrolls, |d| d.justify_center())
                                .when(scrolls, |d| d.overflow_x_scroll())
                                .child(
                                    gpui::canvas(measure, |_, (w, room), window, _| {
                                        if room.get() != Some(w) {
                                            room.set(Some(w));
                                            window.refresh();
                                        }
                                    })
                                    .absolute()
                                    .top_0()
                                    .left_0()
                                    .size_full(),
                                )
                                .when(!popped_out, |d| {
                                    d.child(led_view::led_matrix(frame, width, style, None))
                                })
                                .when(popped_out, |d| {
                                    d.child(
                                        div()
                                            .flex()
                                            .flex_row()
                                            .flex_wrap()
                                            .justify_center()
                                            .items_center()
                                            .gap_3()
                                            .text_body()
                                            .text_color(gpui::rgb(0xa4a9b4))
                                            .child(icon(Icon::PopOut, 16., gpui::rgb(0xa4a9b4)))
                                            .child("The preview is playing in its own window.")
                                            .child(button("show-pop-out", "Show Window").compact().on_click(
                                                cx.listener(|t, _: &ClickEvent, _, cx| t.pop_out_preview(cx)),
                                            ))
                                            .child(
                                                button("close-pop-out", "Close Pop-Out").compact().on_click(
                                                    cx.listener(|t, _: &ClickEvent, _, cx| t.close_pop_out(cx)),
                                                ),
                                            ),
                                    )
                                }),
                        ),
                )
                .child(
                    // A note on the preview itself, under the badge.
                    div()
                        .flex()
                        .flex_row()
                        .items_center()
                        .justify_center()
                        .gap_1p5()
                        .pt_1()
                        .text_caption()
                        .text_color(theme::text_dim())
                        // A pixel down: level with the middle of the
                        // lowercase letters, not the line box, which looks high.
                        .child(
                            div()
                                .flex_none()
                                .mt(px(1.))
                                .child(icon(Icon::InfoCircle, 14., theme::text_dim())),
                        )
                        .child("An approximation of the badge's animation"),
                )
                .child(
                    split_row()
                        .child(
                            group()
                                .mr_4()
                                .child(
                                    // Fits either label, so Restart doesn't move.
                                    div().min_w(px(92.)).flex().child(
                                        button("play", if self.playing { "Pause" } else { "Play" })
                                            .icon(if self.playing { Icon::Pause } else { Icon::Play })
                                            .compact()
                                            .tooltip(if self.playing {
                                                "Pause the preview"
                                            } else {
                                                "Play the preview"
                                            })
                                            .on_click(cx.listener(|t, _: &ClickEvent, _, cx| {
                                                t.playing = !t.playing;
                                                cx.notify();
                                            })),
                                    ),
                                )
                                .child(
                                    button("restart", "Restart")
                                        .icon(Icon::Restart)
                                        .compact()
                                        .tooltip("Play from the start")
                                        .on_click(cx.listener(|t, _: &ClickEvent, _, cx| {
                                            t.preview_step = 0;
                                            cx.notify();
                                        })),
                                ),
                        )
                        .child(
                            group()
                                .ml_auto()
                                .child(
                                    button("export-preview", "Export\u{2026}")
                                        .icon(Icon::Image)
                                        .compact()
                                        .tooltip("Save the animation or the current frame as a GIF, PNG or WebP")
                                        .on_click(cx.listener(|t, _: &ClickEvent, w, cx| t.open_export(w, cx))),
                                )
                                .child(
                                    button("pop-out", if popped_out { "Close Pop-Out" } else { "Pop Out" })
                                        .icon(Icon::PopOut)
                                        .compact()
                                        .selected(popped_out)
                                        .tooltip(if popped_out {
                                            "Close the preview window".to_string()
                                        } else {
                                            format!(
                                                "Show the preview in its own window ({}: full screen)",
                                                crate::preview_window::fullscreen_key()
                                            )
                                        })
                                        .on_click(cx.listener(move |t, _: &ClickEvent, _, cx| {
                                            if popped_out {
                                                t.close_pop_out(cx);
                                            } else {
                                                t.pop_out_preview(cx);
                                            }
                                        })),
                                ),
                        ),
                );
        if with_options {
            card = card.child(
                div()
                    .pt_3()
                    .border_t_1()
                    .border_color(theme::border())
                    .child(self.render_preview_options(false, cx)),
            );
        }
        card
    }

    /// How the preview draws the badge: display width, brightness, glow,
    /// LED shape and color. `stacked` lays them out in a column (for the
    /// right-hand panel).
    pub(super) fn render_preview_options(&mut self, stacked: bool, cx: &mut Context<Self>) -> gpui::Div {
        let width = self.display_width();
        let tint = self.led_tint();
        let tint_items: Vec<MenuItem> = LedColor::PALETTE
            .iter()
            .map(|&c| {
                menu_item(
                    c.name(),
                    c == tint,
                    self.act(cx, move |t, _, cx| {
                        t.project.led_color = Some(c);
                        t.settings.mono_tint = c;
                        t.settings.save();
                        t.sync_input(cx);
                    }),
                )
            })
            .collect();
        let width_row = div()
            .flex()
            .flex_row()
            .flex_none()
            .items_center()
            .gap_2()
            .child(stepper(
                "pw",
                width as i64,
                crate::settings::PREVIEW_WIDTHS.0 as i64,
                crate::settings::PREVIEW_WIDTHS.1 as i64,
                " px",
                COMPACT_H,
                {
                    let this = cx.entity().downgrade();
                    move |v, _, cx| {
                        this.update(cx, |t, cx| {
                            let auto = preview::display_width(t.project.card);
                            t.settings.preview_width = if v as usize == auto { 0 } else { v as usize };
                            t.settings.save();
                            cx.notify();
                        })
                        .ok();
                    }
                },
            ))
            .child({
                let auto = self.settings.preview_width == 0;
                button("pw-auto", "Auto")
                    .compact()
                    .selected(auto)
                    .tooltip(if auto {
                        "The display width follows the badge type"
                    } else {
                        "Use the badge type's own width"
                    })
                    .on_click(cx.listener(|t, _: &ClickEvent, _, cx| {
                        t.settings.preview_width = 0;
                        t.settings.save();
                        cx.notify();
                    }))
            });
        let toggles = div()
            .flex()
            .when(stacked, |d| d.flex_col().gap_0p5())
            .when(!stacked, |d| d.flex_row().flex_wrap().gap_x_5())
            .child(
                checkbox(
                    "preview-brightness",
                    "Show brightness",
                    self.settings.preview_brightness,
                    {
                        let this = cx.entity().downgrade();
                        move |v, _, cx| {
                            this.update(cx, |t, cx| {
                                t.settings.preview_brightness = v;
                                t.settings.save();
                                cx.notify();
                            })
                            .ok();
                        }
                    },
                )
                .tooltip({
                    let level = self.project.brightness.label();
                    move |_, cx| {
                        widgets::tooltip(format!("Show the LEDs at the badge's brightness ({level})").into(), cx)
                    }
                }),
            )
            .child(
                checkbox("preview-glow", "Glow", self.settings.preview_glow, {
                    let this = cx.entity().downgrade();
                    move |v, _, cx| {
                        this.update(cx, |t, cx| {
                            t.settings.preview_glow = v;
                            t.settings.save();
                            cx.notify();
                        })
                        .ok();
                    }
                })
                .tooltip(|_, cx| widgets::tooltip("Lit LEDs glow onto the board around them".into(), cx)),
            )
            .child(checkbox("round", "Round LEDs", self.settings.round_leds, {
                let this = cx.entity().downgrade();
                move |v, _, cx| {
                    this.update(cx, |t, cx| {
                        t.settings.round_leds = v;
                        t.settings.save();
                        cx.notify();
                    })
                    .ok();
                }
            }));
        let color = (!self.project.card.is_color())
            .then(|| self.dropdown("tint", Menu::MonoTint, tint.name(), 140., || tint_items, cx));
        let field = |title: &'static str, control: gpui::AnyElement| {
            div()
                .flex()
                .flex_col()
                .gap_1p5()
                .child(widgets::label(title))
                .child(div().flex().flex_row().child(control))
        };
        if stacked {
            div()
                .flex()
                .flex_col()
                .gap_3()
                .child(div().text_title().child("Preview display"))
                .child(field("Display width", width_row.into_any_element()))
                .when_some(color, |d, c| d.child(field("LED color", c.into_any_element())))
                .child(toggles)
        } else {
            div()
                .flex()
                .flex_row()
                .flex_wrap()
                .items_center()
                .gap_x(px(SET_GAP))
                .gap_y_2()
                .child(labeled("Display width", width_row))
                .when_some(color, |d, c| d.child(labeled("LED color", c)))
                .child(toggles)
        }
    }

    /// The current preview frame, its width in LEDs and how to draw it. Also
    /// used by the pop-out window.
    pub(crate) fn preview_frame(&self) -> (Framebuffer, usize, LedStyle) {
        let target = self.preview_target();
        let frame = preview::frame(
            &self.project.messages[target].bitmap,
            &self.preview_params(target),
            self.preview_step,
        );
        let style = LedStyle {
            look: self.led_look(PREVIEW_CELL),
            guides: false,
            display_width: None,
            draw_off: true,
        };
        (frame, self.display_width(), style)
    }

    /// The pop-out window has closed: play the preview here again.
    pub(crate) fn pop_out_closed(&mut self, id: gpui::WindowId, cx: &mut Context<Self>) {
        // A replaced window closes after its successor opened.
        if self.preview_window.is_some_and(|w| w.window_id() == id) {
            self.preview_window = None;
            cx.notify();
        }
    }
}

/// Padding between the LED board's edge and the LEDs in the preview.
const BOARD_PAD: f32 = 10.;
/// The badge's plastic rim around the board.
const BADGE_RIM: f32 = 12.;
