//! The Pixels section: drawing, image import and export.

use super::*;

/// The status while an image is read or decoded.
const LOADING_IMAGE: &str = "Loading image\u{2026}";

impl BadgeApp {
    /// Sets the image threshold, re-applying it to an image just loaded.
    pub(super) fn set_threshold(&mut self, v: u8, cx: &mut Context<Self>) {
        if self.settings.import_threshold != v {
            self.settings.import_threshold = v;
            self.save_threshold_later(cx);
            self.reapply_import(EditKind::Slider, cx);
            cx.notify();
        }
    }

    /// Saves the settings once the slider has been still for a moment.
    pub(super) fn save_threshold_later(&mut self, cx: &mut Context<Self>) {
        self.threshold_save_gen += 1;
        let generation = self.threshold_save_gen;
        cx.spawn(async move |this: WeakEntity<Self>, cx| {
            cx.background_executor().timer(Duration::from_millis(500)).await;
            this.update(cx, |t, _| {
                if t.threshold_save_gen == generation {
                    t.settings.save();
                }
            })
            .ok();
        })
        .detach();
    }

    pub(super) fn load_image(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let rx = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some("Load image".into()),
        });
        let rx = self.during_system_dialog(rx, cx);
        cx.spawn_in(window, async move |this, cx| {
            if let Ok(Ok(Some(paths))) = rx.await
                && let Some(p) = paths.into_iter().next()
            {
                this.update(cx, |this, cx| this.import_image_from(&p, cx)).ok();
            }
        })
        .detach();
    }

    pub(super) fn import_options(&self) -> ImportOptions {
        ImportOptions {
            fit_height: self.settings.import_fit_height,
            invert: self.settings.import_invert,
            threshold: self.settings.import_threshold,
            ..ImportOptions::default()
        }
    }

    /// Loads an image into the current message. It's read and decoded in
    /// the background, so a large one doesn't freeze the window, and goes
    /// into that message when it's ready.
    pub(super) fn import_image_from(&mut self, path: &Path, cx: &mut Context<Self>) {
        use badge_render::image_import::decode_image;
        self.import_gen += 1;
        let generation = self.import_gen;
        let (card, fit) = (self.project.card, self.settings.import_fit_height);
        self.import_into = Some(self.current);
        let path = path.to_path_buf();
        self.set_status(LOADING_IMAGE, Severity::Info);
        cx.notify();
        cx.spawn(async move |this: WeakEntity<Self>, cx| {
            let read = path.clone();
            let result = cx
                .background_executor()
                .spawn(async move {
                    let bytes = std::fs::read(&read)?;
                    let decoded = decode_image(&bytes, card, fit)?;
                    anyhow::Ok((bytes, decoded))
                })
                .await;
            this.update(cx, |t, cx| {
                // Another image was loaded since.
                if t.import_gen != generation {
                    return;
                }
                match (result, t.import_into.take()) {
                    // Decoded for another badge type's height.
                    (Ok(_), _) if t.project.card != card => t.set_status(
                        "The badge type changed while the image was loading; load it again.",
                        Severity::Error,
                    ),
                    (Ok(_), None) => t.set_status(
                        "The message was replaced while the image was loading; load it again.",
                        Severity::Error,
                    ),
                    (Ok((bytes, decoded)), Some(msg)) => t.finish_import(msg, &path, bytes, (card, fit, decoded)),
                    (Err(e), _) => t.set_status(format!("Could not load image: {e:#}"), Severity::Error),
                }
                // Options changed while it loaded apply now.
                t.reapply_import(EditKind::Other, cx);
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    /// Puts a decoded image into message `msg` with the current import
    /// options.
    fn finish_import(
        &mut self,
        msg: usize,
        path: &Path,
        bytes: Vec<u8>,
        decoded: (CardType, bool, badge_render::image_import::DecodedImage),
    ) {
        use badge_render::image_import::image_to_leds;
        let opts = ImportOptions {
            fit_height: decoded.1,
            ..self.import_options()
        };
        let fb = image_to_leds(&decoded.2, decoded.0, &opts);
        let full = fb.used_width();
        let fb = cut_to(&fb, self.room_for(msg));
        let w = fb.used_width();
        self.import_source = Some(ImportSource {
            msg,
            bytes,
            result: fb.clone(),
            decoded: Some(decoded),
        });
        let m = &mut self.project.messages[msg];
        m.bitmap = fb;
        m.source = Source::Bitmap;
        m.selected = true;
        self.preview_step = 0;
        if w < full {
            self.set_status(
                format!(
                    "Loaded image, cut to {w} of its {full} columns to fit the badge's memory. \
                     Untick or shorten another message to make room."
                ),
                Severity::Error,
            );
        } else {
            self.set_status(
                format!("Loaded image: {w} columns. Adjust Threshold to change which pixels light up."),
                Severity::Success,
            );
        }
        self.settings.last_dir = path.parent().map(Path::to_path_buf);
    }

    /// Drops an image still loading into message `i`, which was replaced.
    pub(super) fn drop_import_into(&mut self, i: usize) {
        if self.import_into == Some(i) {
            self.import_into = None;
        }
    }

    pub(super) fn live_import(&self) -> Option<&ImportSource> {
        self.import_source.as_ref().filter(|src| {
            src.msg == self.current && self.msg().source == Source::Bitmap && self.msg().bitmap == src.result
        })
    }

    /// Re-runs the import with the current options while the message still
    /// shows the untouched imported image. A new badge type or Fit to height
    /// needs the image decoded again, which happens in the background.
    pub(super) fn reapply_import(&mut self, kind: EditKind, cx: &mut Context<Self>) {
        use badge_render::image_import::{decode_image, image_to_leds};
        let Some(src) = self.live_import() else {
            return;
        };
        let opts = self.import_options();
        let card = self.project.card;
        let leds = src
            .decoded
            .as_ref()
            .filter(|(c, fit, _)| *c == card && *fit == opts.fit_height)
            .map(|(_, _, decoded)| image_to_leds(decoded, card, &opts));
        let Some(leds) = leds else {
            let bytes = src.bytes.clone();
            self.decode_gen += 1;
            let (import_gen, decode_gen) = (self.import_gen, self.decode_gen);
            let fit = opts.fit_height;
            self.set_status(LOADING_IMAGE, Severity::Info);
            cx.notify();
            cx.spawn(async move |this: WeakEntity<Self>, cx| {
                let decoded = cx
                    .background_executor()
                    .spawn(async move { decode_image(&bytes, card, fit) })
                    .await;
                this.update(cx, |t, cx| {
                    // Another image was loaded, or the options changed again.
                    if t.import_gen != import_gen || t.decode_gen != decode_gen {
                        return;
                    }
                    match decoded {
                        Ok(decoded) => {
                            if let Some(src) = t.import_source.as_mut() {
                                src.decoded = Some((card, fit, decoded));
                            }
                            t.reapply_import(kind, cx);
                            if t.status.as_ref().is_some_and(|(text, _)| text == LOADING_IMAGE) {
                                t.status = None;
                            }
                        }
                        Err(e) => {
                            t.set_status(format!("Could not load the image again: {e:#}"), Severity::Error);
                            cx.notify();
                        }
                    }
                })
                .ok();
            })
            .detach();
            return;
        };
        let fb = cut_to(&leds, self.room_for_current());
        if let Some(src) = self.import_source.as_mut() {
            src.result = fb.clone();
        }
        self.edit_hint = Some(kind);
        self.msg_mut().bitmap = fb;
        cx.notify();
    }

    pub(super) fn save_image(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let fb = self.msg().bitmap.clone();
        if fb.used_width() == 0 {
            self.set_status("This message has no pixels to save.", Severity::Error);
            cx.notify();
            return;
        }
        let color = self.project.card.is_color();
        let dir = self.default_dir();
        let name = format!("message{}.png", self.current + 1);
        let rx = cx.prompt_for_new_path(&dir, Some(&name));
        let rx = self.during_system_dialog(rx, cx);
        cx.spawn_in(window, async move |this, cx| {
            if let Ok(Ok(Some(mut p))) = rx.await {
                if p.extension().is_none() {
                    p.set_extension("png");
                }
                let r = badge_render::export_png(&fb, color, &p, 1);
                this.update(cx, |this, cx| {
                    match r {
                        Ok(()) => this.set_status(format!("Saved image {}", p.display()), Severity::Success),
                        Err(e) => this.set_status(format!("{e:#}"), Severity::Error),
                    }
                    cx.notify();
                })
                .ok();
            }
        })
        .detach();
    }

    pub(super) fn editor_cell_at(&self, pos: gpui::Point<Pixels>) -> Option<(usize, usize)> {
        let bounds = self.editor_bounds.get()?;
        led_view::hit(bounds, EDITOR_CELL, pos, self.project.card.height())
    }

    /// Moves the drawing sideways so the badge shows it centered, as well as
    /// a drawing can be. The modes that center a short message themselves
    /// (Fixed, Snow, Curtain, Laser) count blank columns before it as part of
    /// it, so there it starts at column 0; in the others it's put in the
    /// middle of the display, if it's narrower than that.
    pub(super) fn center_drawing(&mut self, cx: &mut Context<Self>) {
        let width = self.display_width();
        let m = self.msg();
        let fb = &m.bitmap;
        let lit_col = |x: usize| (0..fb.height()).any(|y| fb.is_on(x, y));
        let right = fb.content_width();
        let Some(left) = (0..right).find(|&x| lit_col(x)) else {
            self.set_status("Nothing to center: the drawing is empty.", Severity::Info);
            return;
        };
        let drawn = right - left;
        let start = if preview::centers_short_messages(m.mode) || drawn >= width {
            0
        } else {
            (width - drawn) / 2
        };
        // No further right than the message has room for (a whole number of
        // bytes, so the rounded-up width fits too).
        let room = self.room_for_current().max(drawn.div_ceil(8) * 8);
        let start = start.min(room - drawn);
        let was_text = m.source == Source::Text;
        if start == left && !was_text {
            self.set_status("Already centered.", Severity::Info);
            return;
        }
        let mut moved = Framebuffer::new(fb.height());
        for x in left..right {
            for y in 0..fb.height() {
                let c = fb.get(x, y);
                if c.is_on() {
                    moved.set(x - left + start, y, c);
                }
            }
        }
        let m = self.msg_mut();
        m.bitmap = moved;
        m.source = Source::Bitmap;
        m.selected = true;
        self.set_status(
            if was_text {
                "Centered. The message now uses these pixels: type text to go back to text."
            } else {
                "Centered."
            },
            Severity::Info,
        );
        cx.notify();
    }

    pub(super) fn on_editor_down(&mut self, ev: &MouseDownEvent, _: &mut Window, cx: &mut Context<Self>) {
        let Some((x, y)) = self.editor_cell_at(ev.position) else {
            return;
        };
        let current = self.msg().bitmap.get(x, y);
        let color = if self.project.card.is_color() {
            self.paint_color
        } else {
            LedColor::RED
        };
        let value = if ev.button == MouseButton::Right || self.tool == Tool::Erase || current == color {
            LedColor::OFF
        } else {
            color
        };
        if self.msg().source == Source::Text {
            self.set_status(
                "Editing pixels: the message now uses your drawing. Type text to go back to text.",
                Severity::Info,
            );
        }
        self.stroke_id += 1;
        self.drag = Some((value, (x, y)));
        self.paint_at(x, y, value, cx);
    }

    pub(super) fn on_editor_move(&mut self, ev: &MouseMoveEvent, _: &mut Window, cx: &mut Context<Self>) {
        let Some((value, last)) = self.drag else {
            return;
        };
        if ev.pressed_button.is_none() {
            self.drag = None;
            return;
        }
        if let Some(cell) = self.editor_cell_at(ev.position)
            && cell != last
        {
            // Fast drags skip cells; fill in the line between events.
            for (x, y) in line_cells(last, cell) {
                if self.msg().bitmap.get(x, y) != value {
                    self.paint_at(x, y, value, cx);
                }
            }
            self.drag = Some((value, cell));
        }
    }

    pub(super) fn paint_at(&mut self, x: usize, y: usize, value: LedColor, cx: &mut Context<Self>) {
        // A lit LED past the end widens the message by whole bytes.
        let widens_to = (x + 1).div_ceil(8) * 8;
        if value.is_on() && widens_to > self.msg().bitmap.used_width() && widens_to > self.room_for_current() {
            self.set_status(Self::memory_full_message(), Severity::Error);
            cx.notify();
            return;
        }
        self.edit_hint = Some(EditKind::Stroke(self.stroke_id));
        let m = self.msg_mut();
        m.source = Source::Bitmap;
        m.selected = true;
        m.bitmap.set(x, y, value);
        cx.notify();
    }

    pub(super) fn render_editor_section(&mut self, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let m = self.msg();
        let color_card = self.project.card.is_color();
        let width = self.display_width();
        let cols = (m.bitmap.used_width() + 24).max(width + 8).min(badge_core::MAX_WIDTH);
        let fb = m.bitmap.clone();
        // Shown centered by the badge (the preview's rule), though drawn
        // here from column 0.
        let centered = preview::centering_offset(m.mode, m.bitmap.content_width(), width) > 0;
        let is_bitmap = m.source == Source::Bitmap;
        let style = LedStyle {
            look: self.led_look(EDITOR_CELL),
            guides: true,
            display_width: Some(width),
            draw_off: true,
        };
        // No control comes or goes with the message's state: a toolbar that
        // changed height would move the grid under the pointer.
        let group = || div().flex().flex_row().flex_none().items_center().gap_1p5();
        let mut draw_group = group().child(segmented([
            button("tool-draw", "Draw")
                .icon(Icon::Pencil)
                .selected(self.tool == Tool::Draw)
                .tooltip("Left-click paints (right-click always erases)")
                .on_click(cx.listener(|t, _: &ClickEvent, _, cx| {
                    t.tool = Tool::Draw;
                    cx.notify();
                })),
            button("tool-erase", "Erase")
                .icon(Icon::Eraser)
                .selected(self.tool == Tool::Erase)
                .tooltip("Left-click erases")
                .on_click(cx.listener(|t, _: &ClickEvent, _, cx| {
                    t.tool = Tool::Erase;
                    cx.notify();
                })),
        ]));
        if color_card {
            draw_group = draw_group.children(LedColor::PALETTE.iter().map(|&c| {
                widgets::tab_stop(div().id(("pc", c.index() as usize)))
                    .flex_none()
                    .size(px(22.))
                    .rounded_md()
                    .cursor_pointer()
                    .bg(theme::swatch(c))
                    .border_2()
                    .border_color(if self.paint_color == c && self.tool == Tool::Draw {
                        gpui::white().into()
                    } else {
                        theme::border()
                    })
                    .on_click(cx.listener(move |t, _: &ClickEvent, _, cx| {
                        t.paint_color = c;
                        t.tool = Tool::Draw;
                        cx.notify();
                    }))
            }));
        }
        let can_go_back = is_bitmap && !self.msg().text.is_empty();
        draw_group = draw_group.child(
            button("back-to-text", "Back to text")
                .icon(Icon::Return)
                .compact()
                .disabled(!can_go_back)
                .tooltip(if can_go_back {
                    "Discard the drawing and show the text again"
                } else {
                    "Available after drawing over a text message"
                })
                .on_click(cx.listener(move |t, _: &ClickEvent, _, cx| {
                    if can_go_back {
                        t.msg_mut().source = Source::Text;
                        t.changed(cx);
                    }
                })),
        );
        let file_group = group()
            .child(
                button("center-drawing", "Center")
                    .compact()
                    .tooltip("Move the drawing sideways so the badge shows it centered")
                    .on_click(cx.listener(|t, _: &ClickEvent, _, cx| t.center_drawing(cx))),
            )
            .child(
                button("clear", "Clear")
                    .compact()
                    .on_click(cx.listener(|t, _: &ClickEvent, _, cx| {
                        let m = t.msg_mut();
                        m.bitmap.clear();
                        m.source = Source::Bitmap;
                        t.set_status("Cleared. Type text to go back to a text message.", Severity::Info);
                        cx.notify();
                    })),
            )
            .child(
                button("save-image", "Save image…")
                    .compact()
                    .tooltip("Save these pixels as a PNG or BMP image")
                    .on_click(cx.listener(|t, _: &ClickEvent, w, cx| t.save_image(w, cx))),
            );
        let threshold = self.settings.import_threshold;
        let default_threshold = crate::settings::Settings::default().import_threshold;
        let import_group = div()
            .flex()
            .flex_row()
            .flex_wrap()
            .min_w_0()
            .max_w_full()
            .items_center()
            .gap_x_5()
            .gap_y_1p5()
            .py(px(3.))
            .pl(px(3.))
            .pr_3()
            .rounded_lg()
            .border_1()
            .border_color(theme::border())
            .bg(theme::panel_alt())
            .child(
                button("load-image", "Load image…")
                    .icon(Icon::Image)
                    .compact()
                    .tooltip("BMP, PNG, GIF, JPG or ICO (or drop a file on the window)")
                    .on_click(cx.listener(|t, _: &ClickEvent, w, cx| t.load_image(w, cx))),
            )
            .child(
                div()
                    .flex()
                    .flex_row()
                    .flex_none()
                    .items_center()
                    .gap_4()
                    .child(
                        checkbox("fit", "Fit to height", self.settings.import_fit_height, {
                            let this = cx.entity().downgrade();
                            move |v, _, cx| {
                                this.update(cx, |t, cx| {
                                    t.settings.import_fit_height = v;
                                    t.settings.save();
                                    t.reapply_import(EditKind::Other, cx);
                                    cx.notify();
                                })
                                .ok();
                            }
                        })
                        .tooltip(move |_, cx| {
                            widgets::tooltip(
                                "Shrink tall images to the badge's height"
                                .into(),
                                cx,
                            )
                        })
                    )
                    .child(
                        checkbox("invert", "Invert", self.settings.import_invert, {
                            let this = cx.entity().downgrade();
                            move |v, _, cx| {
                                this.update(cx, |t, cx| {
                                    t.settings.import_invert = v;
                                    t.settings.save();
                                    t.reapply_import(EditKind::Other, cx);
                                    cx.notify();
                                })
                                .ok();
                            }
                        })
                        .tooltip(move |_, cx| {
                            widgets::tooltip(
                                "Swap lit and unlit (for dark artwork on white)"
                                .into(),
                                cx,
                            )
                        })
                    ),
            )
            .child(
                div()
                    .id("threshold-group")
                    .flex()
                    .flex_row()
                    .flex_none()
                    .items_center()
                    .gap_2()
                    .child(label("Threshold"))
                    .child(widgets::slider(
                        "threshold",
                        threshold as i64,
                        0,
                        254,
                        140.,
                        Some(default_threshold as i64),
                        {
                            let this = cx.entity().downgrade();
                            move |v, _, cx| {
                                this.update(cx, |t, cx| t.set_threshold(v as u8, cx)).ok();
                            }
                        },
                    ))
                    .child(
                        div()
                            .min_w(px(24.))
                            .text_label()
                            .text_color(theme::text_muted())
                            .child(threshold.to_string()),
                    )
                    .tooltip(move |_, cx| {
                        widgets::tooltip(
                            format!(
                                "Which pixels light up: lower lights more, higher only the brightest. Snaps to the default ({default_threshold})."
                            )
                            .into(),
                            cx,
                        )
                    }),
            )
            .child(
                button("threshold-reset", "Reset")
                    .compact()
                    .disabled(threshold == default_threshold)
                    .tooltip(format!("Back to {default_threshold}, the default"))
                    .on_click(cx.listener(move |t, _: &ClickEvent, _, cx| t.set_threshold(default_threshold, cx))),
            );
        let tools = div()
            .flex()
            .flex_row()
            .flex_wrap()
            .items_center()
            .gap_x(px(SET_GAP))
            .gap_y_2()
            .child(draw_group)
            .child(file_group)
            .child(import_group);

        let bounds = self.editor_bounds.clone();
        section(if is_bitmap {
            "Pixels (drawing)"
        } else {
            "Pixels (from text - click to draw)"
        })
        .child(tools)
        .child(
            div()
                .id("editor-scroll")
                .w_full()
                .overflow_x_scroll()
                .rounded_lg()
                .border_1()
                .border_color(theme::border())
                .bg(theme::led_board())
                .p_1p5()
                .child(
                    div()
                        .cursor_crosshair()
                        .on_mouse_down(MouseButton::Left, cx.listener(Self::on_editor_down))
                        .on_mouse_down(MouseButton::Right, cx.listener(Self::on_editor_down))
                        .on_mouse_move(cx.listener(Self::on_editor_move))
                        .on_mouse_up(MouseButton::Left, cx.listener(|t, _, _, _| t.drag = None))
                        .child(led_view::led_matrix(fb, cols, style, Some(bounds))),
                ),
        )
        .child(div().text_caption().text_color(theme::text_dim()).child(format!(
            "{} px wide{} · yellow line = visible badge width ({width} px) · left-click paints, right-click erases",
            m.bitmap.used_width(),
            if centered { ", shown centered on the badge" } else { "" }
        )))
    }
}

/// `fb` without the columns from `width` on.
fn cut_to(fb: &Framebuffer, width: usize) -> Framebuffer {
    if fb.content_width() <= width {
        return fb.clone();
    }
    let mut out = Framebuffer::new(fb.height());
    for x in 0..width {
        for y in 0..fb.height() {
            out.set(x, y, fb.get(x, y));
        }
    }
    out
}

/// Cells on the straight line from `a` to `b` (Bresenham), excluding `a`.
pub(super) fn line_cells(a: (usize, usize), b: (usize, usize)) -> Vec<(usize, usize)> {
    let (mut x, mut y) = (a.0 as i64, a.1 as i64);
    let (x1, y1) = (b.0 as i64, b.1 as i64);
    let (dx, dy) = ((x1 - x).abs(), -(y1 - y).abs());
    let (sx, sy) = (if x < x1 { 1 } else { -1 }, if y < y1 { 1 } else { -1 });
    let mut err = dx + dy;
    let mut out = Vec::new();
    while (x, y) != (x1, y1) {
        let e2 = 2 * err;
        if e2 >= dy {
            err += dy;
            x += sx;
        }
        if e2 <= dx {
            err += dx;
            y += sy;
        }
        out.push((x as usize, y as usize));
    }
    out
}

#[cfg(test)]
mod cut_tests {
    use super::*;

    #[test]
    fn cut_to_keeps_the_left_part() {
        let mut fb = Framebuffer::new(11);
        (0..40).for_each(|x| fb.set(x, 3, LedColor::RED));
        let cut = cut_to(&fb, 16);
        assert_eq!(cut.content_width(), 16);
        assert!((0..16).all(|x| cut.is_on(x, 3)));
        assert_eq!(cut_to(&fb, 64), fb, "narrower: unchanged");
    }
}
