//! The main window in two views, so that a new preview frame redraws only the
//! preview's LEDs: GPUI redraws a view when it's notified, and keeps a cached
//! view's last drawing until then.
//!
//! - [`Content`]: everything in the window, cached. The preview's LEDs only
//!   have room kept for them there (`led_view::matrix_slot`).
//! - [`PreviewLeds`]: the preview's LEDs, painted over that room, and
//!   notified for each new frame.
//! - [`PopOutFrames`]: notified for each new frame instead while Pop Out
//!   has the LEDs, so only Pop Out redraws.

use std::cell::Cell;
use std::rc::Rc;

use gpui::{
    App, ContentMask, Context, Entity, IntoElement, Render, Styled, Subscription, WeakEntity, Window, canvas, div,
};

use super::BadgeApp;
use crate::led_view::{self, Slot};

pub struct Content {
    app: WeakEntity<BadgeApp>,
    _observe: Subscription,
    #[cfg(test)]
    pub renders: usize,
}

impl Content {
    /// Redraws whenever `app` changes.
    pub fn new(app: &Entity<BadgeApp>, cx: &mut Context<Self>) -> Self {
        Self {
            app: app.downgrade(),
            _observe: cx.observe(app, |_, _, cx| cx.notify()),
            #[cfg(test)]
            renders: 0,
        }
    }
}

impl Render for Content {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        #[cfg(test)]
        {
            self.renders += 1;
        }
        self.app
            .update(cx, |app, cx| app.render_window(window, cx).into_any_element())
            .unwrap_or_else(|_| div().into_any_element())
    }
}

/// No window draws it, so notifying it redraws only what observes it.
pub struct PopOutFrames;

pub struct PreviewLeds {
    app: WeakEntity<BadgeApp>,
    slot: Rc<Cell<Option<Slot>>>,
    #[cfg(test)]
    pub renders: usize,
}

impl PreviewLeds {
    pub fn new(app: WeakEntity<BadgeApp>, slot: Rc<Cell<Option<Slot>>>) -> Self {
        Self {
            app,
            slot,
            #[cfg(test)]
            renders: 0,
        }
    }
}

impl Render for PreviewLeds {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        #[cfg(test)]
        {
            self.renders += 1;
        }
        let matrix = self.app.upgrade().map(|app| app.read(cx).preview_frame());
        let slot = self.slot.clone();
        // Painted after `Content`, whose layout notes where the slot is.
        canvas(
            |_, _, _| {},
            move |_, _, window: &mut Window, _: &mut App| {
                let (Some(slot), Some((frame, cols, mut style))) = (slot.get(), matrix) else {
                    return;
                };
                // The size `Content` laid out, which may be a frame behind a
                // change of the room the preview has.
                style.look.cell = slot.cell;
                window.with_content_mask(Some(ContentMask { bounds: slot.clip }), |window| {
                    led_view::paint_matrix(&frame, cols, &style, slot.bounds, window);
                });
            },
        )
        .absolute()
        .top_0()
        .left_0()
        .size_full()
    }
}
