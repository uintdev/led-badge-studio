//! Badges: finding them, badge access on Linux, and sending.

use super::*;

impl BadgeApp {
    /// Rescans devices on plug-in notifications, or every 2 seconds when
    /// they aren't available.
    pub(super) fn start_device_watch(cx: &mut Context<Self>) {
        use futures::StreamExt;
        let (tx, mut changes) = futures::channel::mpsc::unbounded::<()>();
        let watcher = badge_io::watch_devices(move || {
            let _ = tx.unbounded_send(());
        });
        if watcher.is_some() {
            log::info!("watching for devices being plugged in");
        } else {
            log::info!("device notifications unavailable; polling every 2 s");
        }
        cx.spawn(async move |this: WeakEntity<Self>, cx| {
            let watcher = watcher; // keeps notifications running
            let mut notified = watcher.is_some();
            loop {
                let devices = cx.background_executor().spawn(async { badge_io::list_devices() }).await;
                if this.update(cx, |this, cx| this.set_devices(devices, cx)).is_err() {
                    break;
                }
                if notified {
                    if changes.next().await.is_none() {
                        log::warn!("device notifications stopped; polling every 2 s");
                        notified = false;
                        continue;
                    }
                    // One plug-in sends a burst of events; wait for it to settle.
                    cx.background_executor().timer(Duration::from_millis(250)).await;
                    while changes.try_recv().is_ok() {}
                } else {
                    cx.background_executor().timer(Duration::from_secs(2)).await;
                }
            }
        })
        .detach();
    }

    /// Whether a USB badge is plugged in (serial ports don't count: any USB
    /// serial adapter shows up as one).
    pub(super) fn usb_badge_connected(&self) -> bool {
        self.devices.iter().any(|d| d.kind == DeviceKind::Usb)
    }

    /// Rechecks whether this account can write to the selected badge.
    pub(super) fn update_access(&mut self, cx: &mut Context<Self>) {
        let access_missing = self.selected_device().is_some_and(|d| !badge_io::can_write(d));
        if access_missing != self.access_missing {
            self.access_missing = access_missing;
            cx.notify();
        }
    }

    pub(super) fn set_devices(&mut self, devices: Vec<DeviceInfo>, cx: &mut Context<Self>) {
        if devices != self.devices {
            let had_device = self.selected_device().is_some();
            self.devices = devices;
            let still_there = self
                .device
                .as_ref()
                .is_some_and(|id| self.devices.iter().any(|d| &d.id == id));
            if !still_there {
                self.device = self
                    .devices
                    .iter()
                    .find(|d| d.kind == DeviceKind::Usb)
                    .or_else(|| self.devices.first())
                    .map(|d| d.id.clone());
            }
            if !had_device && self.selected_device().is_some() {
                self.offer_last_sent();
            }
            // A USB badge has been connected: no more getting-started card.
            if !self.settings.seen_badge && self.usb_badge_connected() {
                self.settings.seen_badge = true;
                self.settings.save();
            }
            cx.notify();
        }
        self.update_access(cx);
    }

    pub(super) fn selected_device(&self) -> Option<&DeviceInfo> {
        self.device
            .as_ref()
            .and_then(|id| self.devices.iter().find(|d| &d.id == id))
    }

    /// Offers what was last sent if it differs from the project (badges can't
    /// be read back).
    pub(super) fn offer_last_sent(&mut self) {
        self.last_sent_offer = LastSent::load().filter(|l| !l.matches(&self.project));
    }

    pub(super) fn load_last_sent(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(last) = self.last_sent_offer.take().or_else(LastSent::load) else {
            self.set_status("Nothing has been sent from this computer yet.", Severity::Info);
            cx.notify();
            return;
        };
        self.ask_to_save_first(window, cx, move |t, _, cx| {
            t.replace_project(last.project.clone(), None, cx);
            t.settings.save();
            t.set_status(
                format!("Loaded what was sent to the badge {}.", last.when_text()),
                Severity::Success,
            );
            cx.notify();
        });
    }

    pub(super) fn address_mode(&self) -> AddressMode {
        match self.address_kind {
            1 => AddressMode::Write(self.address),
            2 => AddressMode::SendTo(self.address),
            _ => AddressMode::Ignore,
        }
    }

    pub(super) fn open_device_access(&mut self) {
        self.dialog = Some(Dialog::DeviceAccess);
        self.udev_state = None;
    }

    pub(super) fn check_udev_state(&mut self, cx: &mut Context<Self>) {
        if self.udev_checking {
            return;
        }
        self.udev_checking = true;
        cx.spawn(async move |this: WeakEntity<Self>, cx| {
            let (state, can_set_up) = cx
                .background_executor()
                .spawn(async { (badge_io::udev_rule_state(), badge_io::can_set_up_access()) })
                .await;
            this.update(cx, |t, cx| {
                t.udev_checking = false;
                t.udev_state = Some(state);
                t.can_set_up_access = can_set_up;
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    pub(super) fn set_up_access(&mut self, cx: &mut Context<Self>) {
        self.change_access(true, cx);
    }

    pub(super) fn remove_access(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.ask(
            "Remove badge access rule?",
            "This deletes the udev rule that lets your account write to the badge. Sending will stop working \
             until access is set up again. A badge that is plugged in now may keep working until it is unplugged.",
            "Remove Rule",
            true,
            window,
            cx,
            |t, _, cx| t.change_access(false, cx),
        );
    }

    pub(super) fn change_access(&mut self, install: bool, cx: &mut Context<Self>) {
        if self.setting_up_access {
            return;
        }
        self.setting_up_access = true;
        self.set_status("Waiting for your password…", Severity::Info);
        cx.notify();
        cx.spawn(async move |this: WeakEntity<Self>, cx| {
            let result = cx
                .background_executor()
                .spawn(async move {
                    if install {
                        badge_io::install_udev_rule()
                    } else {
                        badge_io::remove_udev_rule()
                    }
                })
                .await;
            // Give udev a moment to apply the new permissions.
            cx.background_executor().timer(Duration::from_millis(700)).await;
            let devices = cx.background_executor().spawn(async { badge_io::list_devices() }).await;
            this.update(cx, |t, cx| {
                t.setting_up_access = false;
                t.udev_state = None;
                let what = if install { "set up" } else { "removed" };
                match result {
                    Ok(()) => {
                        t.devices.clear();
                        t.set_devices(devices, cx);
                        if install && t.access_missing {
                            t.set_status(
                                "Access rule installed. Unplug the badge and plug it back in to finish.",
                                Severity::Info,
                            );
                        } else if install {
                            t.set_status("Badge access is set up. You can send now.", Severity::Success);
                        } else {
                            t.set_status(
                                "Badge access rule removed. Unplug the badge to revoke access it already has.",
                                Severity::Success,
                            );
                        }
                    }
                    Err(badge_io::AccessSetupError::Canceled) => {
                        t.set_status(format!("Canceled; badge access was not {what}."), Severity::Info);
                    }
                    Err(e) => t.set_status(
                        format!(
                            "Couldn't {} badge access: {e}",
                            if install { "set up" } else { "remove" }
                        ),
                        Severity::Error,
                    ),
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    pub(super) fn send(&mut self, _: &SendToBadge, _window: &mut Window, cx: &mut Context<Self>) {
        log::info!(
            "send requested (device {:?}, {} devices known)",
            self.device,
            self.devices.len()
        );
        if matches!(self.send, SendState::Sending { .. }) {
            log::info!("send ignored: already sending");
            return;
        }
        let Some(device) = self
            .device
            .as_ref()
            .and_then(|id| self.devices.iter().find(|d| &d.id == id))
            .cloned()
        else {
            self.set_status(
                "No badge connected. Plug the badge in with its USB cable.",
                Severity::Error,
            );
            cx.notify();
            return;
        };
        if badge_core::protocol::is_empty(&self.project) {
            self.set_status(
                "Nothing to send: tick at least one message that has content.",
                Severity::Error,
            );
            cx.notify();
            return;
        }
        let project = self.project.clone();
        let sent_copy = self.project.clone();
        let address = self.address_mode();
        let (tx, mut rx) = futures::channel::mpsc::unbounded::<SendMsg>();
        log::info!(
            "sending {} columns to {} ({:?})",
            project.used_columns(),
            device.id,
            device.kind
        );
        let is_usb = device.kind == DeviceKind::Usb;
        std::thread::spawn(move || {
            let progress_tx = tx.clone();
            let r = badge_io::send(&project, address, &device, &mut |d, t| {
                progress_tx.unbounded_send(SendMsg::Progress(d, t)).ok();
                true
            });
            let r = r.map_err(|e| {
                let perm = matches!(e, badge_io::SendError::PermissionDenied);
                (e.to_string(), perm)
            });
            tx.unbounded_send(SendMsg::Done(r)).ok();
        });
        self.send = SendState::Sending { done: 0, total: 1 };
        show_send_progress(Some(0.0));
        let status = if is_usb {
            let reports = badge_core::protocol::encode_usb(&self.project, address, badge_io::now())
                .len()
                .div_ceil(badge_core::protocol::REPORT_SIZE);
            let secs = badge_io::Pacing::from_env().estimate(reports).as_secs_f32().ceil() as u32;
            format!("Sending… (about {secs} s - keep the badge plugged in)")
        } else {
            "Sending…".to_string()
        };
        self.set_status(status, Severity::Info);
        self.settings.last_device = self.device.clone();
        self.settings.save();
        cx.notify();
        cx.spawn(async move |this: WeakEntity<Self>, cx| {
            while let Some(msg) = rx.next().await {
                let stop = this
                    .update(cx, |this, cx| {
                        let stop = match msg {
                            SendMsg::Progress(done, total) => {
                                this.send = SendState::Sending { done, total };
                                show_send_progress(Some(done as f64 / total.max(1) as f64));
                                false
                            }
                            SendMsg::Done(r) => {
                                log::info!("send finished: {:?}", r.as_ref().map_err(|e| &e.0));
                                show_send_progress(None);
                                this.send = SendState::Idle;
                                match r {
                                    Ok(()) => {
                                        this.set_status(
                                            "Sent! The badge should restart its display with the new messages. \
                                             If it keeps showing the old ones, unplug it, plug it back in and send again.",
                                            Severity::Success,
                                        );
                                        LastSent::record(&sent_copy);
                                        this.last_sent_offer = None;
                                        this.send = SendState::Sent;
                                        cx.spawn(async move |this: WeakEntity<Self>, cx| {
                                            cx.background_executor().timer(Duration::from_millis(2500)).await;
                                            this.update(cx, |t, cx| {
                                                if matches!(t.send, SendState::Sent) {
                                                    t.send = SendState::Idle;
                                                    cx.notify();
                                                }
                                            })
                                            .ok();
                                        })
                                        .detach();
                                    }
                                    Err((e, perm)) => {
                                        this.set_status(e, Severity::Error);
                                        if perm && cfg!(target_os = "linux") {
                                            this.open_device_access();
                                        }
                                    }
                                }
                                true
                            }
                        };
                        cx.notify();
                        stop
                    })
                    .unwrap_or(true);
                if stop {
                    break;
                }
            }
            // Also when the window went away mid-send.
            show_send_progress(None);
        })
        .detach();
    }

    /// The title bar light: green when the badge is ready, amber when it can't
    /// be written to yet, red when none is connected.
    pub(crate) fn status_light(&self) -> crate::widgets::StatusLight {
        let (color, tip) = match self.selected_device() {
            None => (theme::danger(), "No badge connected".to_string()),
            Some(d) if self.access_missing => (
                theme::warning(),
                format!(
                    "{} is connected but can't be written to yet (see Badge Access)",
                    d.label
                ),
            ),
            Some(d) => (theme::success(), format!("{} is connected and ready", d.label)),
        };
        crate::widgets::StatusLight {
            color: color.into(),
            tip: tip.into(),
        }
    }
}

/// Shows how far a send is on the app's Dock icon (macOS) or taskbar button
/// (Windows), or takes it away with `None`.
fn show_send_progress(_fraction: Option<f64>) {
    #[cfg(all(target_os = "macos", not(test)))]
    dock_progress::set(_fraction);
    #[cfg(all(windows, not(test)))]
    taskbar::set_progress(_fraction);
}
