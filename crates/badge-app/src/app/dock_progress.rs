//! A progress bar on the app's Dock icon while sending (macOS), so a long
//! upload can be followed from another app. Only built on macOS.
//!
//! Each update puts in a new `NSProgressIndicator` already set to the new
//! value: changing the value of one that's shown animates toward it, and in
//! the Dock (outside any window) that animation never runs, so the bar
//! stayed at its first value.

use std::cell::Cell;

use objc::runtime::{NO, Object, YES};
use objc::{class, msg_send, sel, sel_impl};

type Id = *mut Object;

#[repr(C)]
#[derive(Clone, Copy)]
struct Size {
    width: f64,
    height: f64,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct Rect {
    x: f64,
    y: f64,
    size: Size,
}

thread_local! {
    /// The Dock tile's view and its bar while they're shown; the tile owns
    /// the view and the view the bar.
    static SHOWN: Cell<Option<(Id, Id)>> = const { Cell::new(None) };
}

/// Shows `fraction` (0 to 1) on the Dock icon, or the plain icon again with
/// `None`. Must be called on the main thread.
#[allow(unexpected_cfgs)] // from the objc macros
pub(super) fn set(fraction: Option<f64>) {
    // SAFETY: AppKit calls on the main thread, where the UI runs. The view
    // and bar are kept alive by the tile and the view, and forgotten when
    // those let them go.
    unsafe {
        let app: Id = msg_send![class!(NSApplication), sharedApplication];
        let tile: Id = msg_send![app, dockTile];
        let Some(fraction) = fraction else {
            if SHOWN.take().is_some() {
                let _: () = msg_send![tile, setContentView: std::ptr::null_mut::<Object>()];
                let _: () = msg_send![tile, display];
            }
            return;
        };
        let size: Size = msg_send![tile, size];
        let view = match SHOWN.get() {
            Some((view, old_bar)) => {
                let _: () = msg_send![old_bar, removeFromSuperview];
                view
            }
            None => {
                // The app's icon, with the bar across its lower part.
                let view: Id = msg_send![class!(NSImageView), alloc];
                let view: Id = msg_send![view, initWithFrame: Rect { x: 0.0, y: 0.0, size }];
                let icon: Id = msg_send![app, applicationIconImage];
                let _: () = msg_send![view, setImage: icon];
                let _: () = msg_send![tile, setContentView: view];
                let _: () = msg_send![view, release]; // the tile holds it
                view
            }
        };
        let frame = Rect {
            x: size.width * 0.1,
            y: size.height * 0.08,
            size: Size {
                width: size.width * 0.8,
                height: size.height * 0.15,
            },
        };
        let bar: Id = msg_send![class!(NSProgressIndicator), alloc];
        let bar: Id = msg_send![bar, initWithFrame: frame];
        let _: () = msg_send![bar, setStyle: 0usize]; // NSProgressIndicatorStyleBar
        let _: () = msg_send![bar, setIndeterminate: NO];
        let _: () = msg_send![bar, setBezeled: YES];
        let _: () = msg_send![bar, setMinValue: 0.0f64];
        let _: () = msg_send![bar, setMaxValue: 1.0f64];
        let _: () = msg_send![bar, setDoubleValue: fraction.clamp(0.0, 1.0)];
        let _: () = msg_send![view, addSubview: bar];
        let _: () = msg_send![bar, release]; // the view holds it
        SHOWN.set(Some((view, bar)));
        let _: () = msg_send![tile, display];
    }
}
