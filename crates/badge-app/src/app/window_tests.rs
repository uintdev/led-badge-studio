//! Tests of the main window's behavior, run in GPUI's test mode (no real
//! window, badge or settings file).

use gpui::{TestAppContext, VisualTestContext};

use super::*;

/// Opens the main window with a new project.
fn open(cx: &mut TestAppContext) -> (Entity<BadgeApp>, &mut VisualTestContext) {
    cx.update(|cx| cx.bind_keys(bindings()));
    cx.add_window_view(|window, cx| BadgeApp::new(None, window, cx))
}

/// Sets the current message's text, as typing in the message box does, and
/// records it as an edit.
fn type_text(app: &Entity<BadgeApp>, cx: &mut VisualTestContext, text: &str) {
    app.update(cx, |app, cx| {
        app.apply_text(text.to_string(), None, cx);
        app.commit_history();
    });
}

/// A folder of its own for a test's files.
fn temp_dir(test: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("led-badge-studio-{test}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// Saves the open project to `path`.
fn save(app: &Entity<BadgeApp>, cx: &mut VisualTestContext, path: &Path) {
    app.update(cx, |app, cx| app.save_to(path.to_path_buf(), cx));
}

/// Opens `path` as File → Open does.
fn open_file(app: &Entity<BadgeApp>, cx: &mut VisualTestContext, path: &Path) {
    app.update_in(cx, |app, window, cx| app.open_requested(path.to_path_buf(), window, cx));
}

#[gpui::test]
fn a_new_project_is_not_modified(cx: &mut TestAppContext) {
    let (app, cx) = open(cx);
    cx.run_until_parked();
    app.read_with(cx, |app, _| {
        assert!(!app.is_dirty());
        assert!(!app.title().starts_with('\u{2022}'));
        assert!(!app.title().contains("Edited"));
    });
}

#[gpui::test]
fn undoing_every_change_clears_modified(cx: &mut TestAppContext) {
    let (app, cx) = open(cx);
    let before = app.read_with(cx, |app, _| app.msg().text.clone());
    type_text(&app, cx, "Hi there");
    app.read_with(cx, |app, _| {
        assert!(app.is_dirty());
        // On macOS the title's "Edited" is macOS's (native_document.rs).
        assert_eq!(
            app.title().starts_with('\u{2022}'),
            !cfg!(target_os = "macos"),
            "the title shows it"
        );
    });
    cx.dispatch_action(Undo);
    app.read_with(cx, |app, _| {
        assert_eq!(app.msg().text, before);
        assert!(!app.is_dirty(), "back to the saved project");
    });
    cx.dispatch_action(Redo);
    app.read_with(cx, |app, _| {
        assert_eq!(app.msg().text, "Hi there");
        assert!(app.is_dirty());
    });
}

#[gpui::test]
fn typing_in_one_go_is_one_undo_step(cx: &mut TestAppContext) {
    let (app, cx) = open(cx);
    let before = app.read_with(cx, |app, _| app.msg().text.clone());
    for text in ["H", "Hi", "Hi!"] {
        type_text(&app, cx, text);
    }
    cx.dispatch_action(Undo);
    app.read_with(cx, |app, _| assert_eq!(app.msg().text, before));
}

#[gpui::test]
fn saving_clears_modified_and_reopening_restores_the_project(cx: &mut TestAppContext) {
    let dir = temp_dir("save");
    let path = dir.join("test.badge");
    let (app, cx) = open(cx);
    type_text(&app, cx, "Saved text");
    app.update(cx, |app, _| app.project.led_color = Some(LedColor::BLUE));
    save(&app, cx, &path);
    app.read_with(cx, |app, _| {
        assert!(!app.is_dirty());
        assert_eq!(app.path.as_deref(), Some(path.as_path()));
        assert_eq!(app.settings.recent_files.first(), Some(&path), "listed in Open Recent");
    });
    cx.run_until_parked();
    let title = cx.window_title().unwrap();
    if cfg!(target_os = "macos") {
        assert_eq!(title, "test.badge");
    } else {
        assert_eq!(title, format!("test.badge - {}", env!("BRAND_NAME")));
    }

    // A new project, then the saved one again.
    app.update_in(cx, |app, window, cx| app.new_project(&NewProject, window, cx));
    app.read_with(cx, |app, _| assert_ne!(app.msg().text, "Saved text"));
    open_file(&app, cx, &path);
    app.read_with(cx, |app, _| {
        assert_eq!(app.msg().text, "Saved text");
        assert_eq!(
            app.project.led_color,
            Some(LedColor::BLUE),
            "the LED color is saved too"
        );
        assert!(!app.is_dirty());
        assert!(app.dialog.is_none());
    });
    let _ = std::fs::remove_dir_all(dir);
}

#[gpui::test]
fn opening_over_unsaved_changes_asks_first(cx: &mut TestAppContext) {
    let dir = temp_dir("discard");
    let path = dir.join("other.badge");
    let (app, cx) = open(cx);
    type_text(&app, cx, "In the file");
    save(&app, cx, &path);
    // A new project, which has no file to save itself into (on macOS).
    app.update_in(cx, |app, window, cx| app.new_project(&NewProject, window, cx));
    type_text(&app, cx, "Not saved");

    open_file(&app, cx, &path);
    app.read_with(cx, |app, _| {
        assert!(matches!(app.dialog, Some(Dialog::Confirm { .. })));
        assert_eq!(app.msg().text, "Not saved", "nothing changes until confirmed");
    });
    // Cancel keeps the work.
    cx.dispatch_action(ModalCancel);
    app.read_with(cx, |app, _| {
        assert!(app.dialog.is_none());
        assert_eq!(app.msg().text, "Not saved");
    });
    // Don't Save opens the file.
    open_file(&app, cx, &path);
    app.update_in(cx, |app, window, cx| app.accept_alternate(window, cx));
    app.read_with(cx, |app, _| {
        assert_eq!(app.msg().text, "In the file");
        assert!(!app.is_dirty());
    });
    let _ = std::fs::remove_dir_all(dir);
}

/// Unsaved changes to a project with a file: Save saves them first, Don't
/// Save leaves the file as it was.
#[gpui::test]
fn opening_over_changes_asks_whether_to_save_them(cx: &mut TestAppContext) {
    let dir = temp_dir("ask-to-save");
    let (mine, other) = (dir.join("mine.badge"), dir.join("other.badge"));
    let (app, cx) = open(cx);
    type_text(&app, cx, "Other");
    save(&app, cx, &other);
    type_text(&app, cx, "Mine");
    save(&app, cx, &mine);

    // Save.
    type_text(&app, cx, "Mine, changed");
    open_file(&app, cx, &other);
    app.read_with(cx, |app, _| {
        assert!(matches!(
            app.dialog,
            Some(Dialog::Confirm {
                alternate_label: Some(_),
                ..
            })
        ));
        assert_eq!(app.msg().text, "Mine, changed", "nothing changes until answered");
    });
    app.update_in(cx, |app, window, cx| app.accept_dialog(window, cx));
    app.read_with(cx, |app, _| assert_eq!(app.msg().text, "Other"));
    assert_eq!(badge_formats::open(&mine).unwrap().messages[0].text, "Mine, changed");

    // Don't Save.
    open_file(&app, cx, &mine);
    type_text(&app, cx, "Not kept");
    open_file(&app, cx, &other);
    app.update_in(cx, |app, window, cx| app.accept_alternate(window, cx));
    app.read_with(cx, |app, _| assert_eq!(app.msg().text, "Other"));
    assert_eq!(badge_formats::open(&mine).unwrap().messages[0].text, "Mine, changed");
    let _ = std::fs::remove_dir_all(dir);
}

/// Closing with unsaved changes asks; nothing is saved without asking.
#[gpui::test]
fn closing_with_unsaved_changes_asks_whether_to_save_them(cx: &mut TestAppContext) {
    let dir = temp_dir("close-ask");
    let path = dir.join("mine.badge");
    let (app, cx) = open(cx);
    type_text(&app, cx, "Saved");
    save(&app, cx, &path);
    type_text(&app, cx, "Changed");
    app.update_in(cx, |app, window, cx| app.request_close(window, cx));
    app.read_with(cx, |app, _| assert!(app.dialog.is_some(), "asked"));
    assert_eq!(
        badge_formats::open(&path).unwrap().messages[0].text,
        "Saved",
        "not saved yet"
    );
    app.update_in(cx, |app, window, cx| app.accept_dialog(window, cx));
    assert_eq!(
        badge_formats::open(&path).unwrap().messages[0].text,
        "Changed",
        "Save saved"
    );
    let _ = std::fs::remove_dir_all(dir);
}

/// Quitting or closing mid-send would leave the badge stuck, so it's asked
/// about first, before any unsaved changes.
#[gpui::test]
fn closing_while_sending_asks_first(cx: &mut TestAppContext) {
    let (app, cx) = open(cx);
    let sending_asked = |app: &BadgeApp| matches!(&app.dialog, Some(Dialog::Confirm { title, .. }) if title.contains("still being sent"));
    app.update(cx, |app, _| app.send = SendState::Sending { done: 1, total: 10 });

    // With nothing unsaved too: Quit asks, and Cancel keeps the app open.
    cx.dispatch_action(Quit);
    app.read_with(cx, |app, _| assert!(sending_asked(app), "Quit asked"));
    cx.dispatch_action(ModalCancel);
    app.read_with(cx, |app, _| assert!(app.dialog.is_none()));

    // Quit Anyway goes on to the unsaved changes.
    type_text(&app, cx, "Changed");
    app.update_in(cx, |app, window, cx| app.request_close(window, cx));
    app.read_with(cx, |app, _| assert!(sending_asked(app), "closing asked"));
    app.update_in(cx, |app, window, cx| app.accept_dialog(window, cx));
    app.read_with(cx, |app, _| {
        assert!(
            matches!(
                app.dialog,
                Some(Dialog::Confirm {
                    alternate_label: Some(_),
                    ..
                })
            ),
            "then asked whether to save"
        );
    });

    // Once the send is over, closing only asks about unsaved changes.
    cx.dispatch_action(ModalCancel);
    app.update(cx, |app, _| app.send = SendState::Idle);
    app.update_in(cx, |app, window, cx| app.request_close(window, cx));
    app.read_with(cx, |app, _| {
        assert!(app.dialog.is_some() && !sending_asked(app));
    });
}

#[gpui::test]
fn a_file_with_unsupported_values_is_repaired_only_when_asked(cx: &mut TestAppContext) {
    let dir = temp_dir("repair");
    let path = dir.join("bad.badge");
    let (app, cx) = open(cx);
    save(&app, cx, &path);
    // Speed 9 doesn't exist on the badge.
    let mut json: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    json["project"]["messages"][0]["speed"] = 9.into();
    let original = serde_json::to_string_pretty(&json).unwrap();
    std::fs::write(&path, &original).unwrap();

    open_file(&app, cx, &path);
    app.read_with(cx, |app, _| {
        let Some(Dialog::Repair { fixes, opening, .. }) = &app.dialog else {
            panic!(
                "expected the Repair dialog, got {:?}",
                app.dialog.as_ref().map(std::mem::discriminant)
            );
        };
        assert!(*opening);
        assert_eq!(fixes.len(), 1);
    });
    // Open Repaired: the project is fixed, the file isn't, so it's modified.
    app.update_in(cx, |app, window, cx| app.finish_repair(true, false, window, cx));
    app.read_with(cx, |app, _| {
        assert_eq!(app.project.messages[0].speed, 8);
        assert!(app.is_dirty());
    });
    assert_eq!(
        std::fs::read_to_string(&path).unwrap(),
        original,
        "the file is left as it was"
    );
    let _ = std::fs::remove_dir_all(dir);
}

/// Check Project File's Repair and save keeps the file as it was, and
/// saving the open file over it afterwards isn't taken for another
/// program's change.
#[gpui::test]
fn repairing_and_saving_keeps_the_file_as_it_was(cx: &mut TestAppContext) {
    let dir = temp_dir("repair-save");
    let path = dir.join("bad.badge");
    let (app, cx) = open(cx);
    app.update(cx, |app, _| app.versions_dir = Some(dir.join("versions")));
    save(&app, cx, &path);
    let mut json: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    json["project"]["messages"][0]["speed"] = 9.into();
    let original = serde_json::to_string_pretty(&json).unwrap();
    std::fs::write(&path, &original).unwrap();
    // The open project is that file, as a repaired copy.
    open_file(&app, cx, &path);
    app.update_in(cx, |app, window, cx| app.finish_repair(true, false, window, cx));

    app.update_in(cx, |app, window, cx| app.check_file(path.clone(), window, cx));
    app.update_in(cx, |app, window, cx| app.finish_repair(false, true, window, cx));
    assert_eq!(badge_formats::open(&path).unwrap().messages[0].speed, 8, "repaired");
    app.read_with(cx, |app, _| {
        assert_eq!(app.file_versions.len(), 1, "the file as it was is kept");
        let kept = &app.file_versions[0].1;
        assert_eq!(std::fs::read_to_string(kept).unwrap(), original);
        assert!(!app.changed_elsewhere(), "the app saved it, not another program");
    });
    let _ = std::fs::remove_dir_all(dir);
}

#[gpui::test]
fn a_file_that_cant_be_read_leaves_the_project_alone(cx: &mut TestAppContext) {
    let dir = temp_dir("broken");
    let path = dir.join("broken.badge");
    std::fs::write(&path, "{ \"format\": \"led-badge-project\", ").unwrap();
    let (app, cx) = open(cx);
    type_text(&app, cx, "My work");

    open_file(&app, cx, &path);
    app.read_with(cx, |app, _| {
        assert!(matches!(app.dialog, Some(Dialog::Notice { error: true, .. })));
        assert_eq!(app.msg().text, "My work");
        assert!(app.is_dirty(), "unsaved work is still unsaved, not lost");
    });
    let _ = std::fs::remove_dir_all(dir);
}

#[gpui::test]
fn a_missing_font_is_reported_and_can_be_switched_to_the_default(cx: &mut TestAppContext) {
    let (app, cx) = open(cx);
    app.update(cx, |app, cx| {
        app.project.messages[1].text = "Two".into();
        app.project.messages[1].font = FontChoice::System {
            family: "No Such Font Anywhere".into(),
            size_px: 11,
            bold: false,
            italic: false,
        };
        app.commit_history();
        assert_eq!(app.unavailable_fonts(), vec![(1, "No Such Font Anywhere".to_string())]);
        app.use_default_font(cx);
        app.commit_history();
        assert!(app.unavailable_fonts().is_empty());
        assert_eq!(app.project.messages[1].font, FontChoice::Builtin);
    });
    cx.dispatch_action(Undo);
    app.read_with(cx, |app, _| {
        assert_eq!(app.unavailable_fonts().len(), 1, "Undo brings the font back");
    });
}

#[gpui::test]
fn a_built_in_font_too_tall_for_the_badge_is_reported(cx: &mut TestAppContext) {
    let (app, cx) = open(cx);
    app.update(cx, |app, _| {
        assert!(!app.project.card.is_color() && app.project.card.height() < 16);
        app.project.messages[0].font = FontChoice::Pixel(badge_core::PixelFont::Unifont);
        assert_eq!(app.unavailable_fonts().len(), 1, "GNU Unifont needs 16 rows");
    });
}

#[gpui::test]
fn typing_over_a_drawing_asks_first(cx: &mut TestAppContext) {
    let (app, cx) = open(cx);
    app.update(cx, |app, _| {
        let m = app.msg_mut();
        m.source = Source::Bitmap;
        m.bitmap.set(0, 0, LedColor::RED);
        app.commit_history();
    });
    let input = app.read_with(cx, |app, _| app.input.clone());
    input.update_in(cx, |input, window, cx| input.insert("x", window, cx));
    cx.run_until_parked();
    app.read_with(cx, |app, _| {
        assert!(matches!(app.dialog, Some(Dialog::Confirm { .. })));
        assert_eq!(app.msg().source, Source::Bitmap, "the drawing stays until confirmed");
    });
    // Typing on while it asks doesn't reach the message, nor stay in the box.
    input.update_in(cx, |input, window, cx| input.insert("y", window, cx));
    cx.run_until_parked();
    app.read_with(cx, |app, cx| {
        assert_eq!(app.msg().source, Source::Bitmap);
        assert_eq!(app.input.read(cx).text(), app.msg().text);
    });
    // Another question taking its place counts as Keep Drawing.
    cx.dispatch_action(ModalCancel);
    input.update_in(cx, |input, window, cx| input.insert("z", window, cx));
    cx.run_until_parked();
    app.read_with(cx, |app, cx| assert_ne!(app.input.read(cx).text(), app.msg().text));
    app.update_in(cx, |app, window, cx| {
        app.notice("Elsewhere", "", Vec::new(), false, window, cx)
    });
    app.read_with(cx, |app, cx| {
        assert_eq!(app.msg().source, Source::Bitmap);
        assert_eq!(app.input.read(cx).text(), app.msg().text);
    });
    app.update_in(cx, |app, window, cx| app.close_dialog(window, cx));
    input.update_in(cx, |input, window, cx| input.insert("x", window, cx));
    cx.run_until_parked();
    // Keep Drawing: the keystroke is undone in the message box.
    cx.dispatch_action(ModalCancel);
    cx.run_until_parked();
    app.read_with(cx, |app, cx| {
        assert_eq!(app.msg().source, Source::Bitmap);
        assert_eq!(app.input.read(cx).text(), app.msg().text);
    });
}

#[gpui::test]
fn switching_badge_type_keeps_the_led_color(cx: &mut TestAppContext) {
    let (app, cx) = open(cx);
    app.update(cx, |app, cx| {
        app.project.led_color = Some(LedColor::GREEN);
        app.set_card(CardType::Rgb12, cx);
        assert_eq!(app.project.card, CardType::Rgb12);
        app.set_card(CardType::Mono11, cx);
        assert_eq!(app.project.led_color, Some(LedColor::GREEN));
    });
}

#[gpui::test]
fn all_ticked_previews_each_message_with_content(cx: &mut TestAppContext) {
    let (app, cx) = open(cx);
    app.update(cx, |app, _| {
        app.project.messages[2].text = "Three".into();
        app.project.messages[2].selected = true;
        app.rerender_all();
        app.current = 0;
        app.settings.preview_all = false;
        assert_eq!(app.preview_playlist(), vec![0], "only the message being edited");
        app.settings.preview_all = true;
        assert_eq!(app.preview_playlist(), vec![0, 2]);
        // Nothing ticked: the message being edited.
        for m in &mut app.project.messages {
            m.selected = false;
        }
        assert_eq!(app.preview_playlist(), vec![0]);
    });
}

#[gpui::test]
fn menus_open_upward_only_when_there_is_more_room_above(cx: &mut TestAppContext) {
    let (app, cx) = open(cx);
    app.update(cx, |app, _| {
        app.viewport_h = 600.;
        let at = |top: f32| Bounds::new(gpui::point(px(0.), px(top)), gpui::size(px(100.), px(30.)));
        app.menu_anchors.borrow_mut().insert(Menu::File, at(40.));
        assert!(!app.menu_room(Menu::File, 300.).0, "room below");
        app.menu_anchors.borrow_mut().insert(Menu::File, at(500.));
        let (up, height) = app.menu_room(Menu::File, 300.);
        assert!(up, "no room below, more above");
        assert!(height >= 300.);
        // A menu taller than either side opens on the larger one and scrolls.
        app.menu_anchors.borrow_mut().insert(Menu::File, at(200.));
        let (up, height) = app.menu_room(Menu::File, 1000.);
        assert!(!up);
        assert!(height < 1000.);
    });
}

#[gpui::test]
fn dragging_a_message_moves_it_and_keeps_it_selected(cx: &mut TestAppContext) {
    let (app, cx) = open(cx);
    app.update(cx, |app, cx| {
        for (i, text) in ["One", "Two", "Three"].into_iter().enumerate() {
            app.project.messages[i].text = text.into();
        }
        app.current = 0;
        app.commit_history();
        app.move_message(0, 2, cx);
        let texts: Vec<&str> = app.project.messages[..3].iter().map(|m| m.text.as_str()).collect();
        assert_eq!(texts, ["Two", "Three", "One"]);
        assert_eq!(app.current, 2, "the selection follows the moved message");
    });
    cx.dispatch_action(Undo);
    app.read_with(cx, |app, _| assert_eq!(app.project.messages[0].text, "One"));
}

#[gpui::test]
fn duplicating_fills_the_next_empty_slot(cx: &mut TestAppContext) {
    let (app, cx) = open(cx);
    app.update(cx, |app, cx| {
        app.project.messages[0].text = "Hi".into();
        app.project.messages[1].text = "Taken".into();
        app.duplicate_message(0, cx);
        assert_eq!(app.project.messages[2].text, "Hi");
        assert_eq!(app.current, 2);
        // Every slot full: nothing is overwritten.
        for m in &mut app.project.messages {
            m.text = "Full".into();
        }
        let before = app.project.clone();
        app.duplicate_message(0, cx);
        assert_eq!(app.project, before);
    });
}

#[gpui::test]
fn a_copied_message_pastes_into_another_slot(cx: &mut TestAppContext) {
    let (app, cx) = open(cx);
    app.update(cx, |app, cx| {
        app.project.messages[0].text = "Copied".into();
        app.project.messages[0].mode = Mode::Laser;
        app.project.messages[0].speed = 7;
        app.copy_message(0, cx);
        app.paste_message(5, cx);
        let m = &app.project.messages[5];
        assert_eq!((m.text.as_str(), m.mode, m.speed), ("Copied", Mode::Laser, 7));
        assert_eq!(app.current, 5);
    });
    // Other clipboard text isn't a message.
    cx.update(|_, cx| cx.write_to_clipboard(gpui::ClipboardItem::new_string("plain text".into())));
    app.update(cx, |app, cx| {
        let before = app.project.clone();
        app.paste_message(6, cx);
        assert_eq!(app.project, before);
    });
}

#[gpui::test]
fn the_theme_follows_the_setting_and_falls_back_to_dark(cx: &mut TestAppContext) {
    use crate::settings::Appearance;
    let (app, cx) = open(cx);
    app.update(cx, |app, cx| {
        let mut check = |appearance, system, dark: bool| {
            app.settings.appearance = appearance;
            app.system_dark = system;
            app.apply_theme(None, cx);
            assert_eq!(theme::is_dark(), dark, "{appearance:?} with the system at {system:?}");
        };
        check(Appearance::System, None, true);
        check(Appearance::System, Some(false), false);
        check(Appearance::System, Some(true), true);
        check(Appearance::Light, Some(true), false);
        check(Appearance::Dark, Some(false), true);
    });
}

#[gpui::test]
fn show_brightness_is_on_for_new_users(cx: &mut TestAppContext) {
    let (app, cx) = open(cx);
    app.read_with(cx, |app, _| assert!(app.settings.preview_brightness));
}

#[gpui::test]
fn tab_reaches_the_message_list_then_the_message_box(cx: &mut TestAppContext) {
    let (app, cx) = open(cx);
    cx.run_until_parked();
    let input = app.read_with(cx, |app, cx| app.input.read(cx).focus_handle(cx));
    let mut order = Vec::new();
    let mut presses = 0;
    while order.len() < 4 && presses < 300 {
        cx.dispatch_action(FocusNext);
        presses += 1;
        let (row, text) = cx.update(|window, cx| {
            let app = app.read(cx);
            (
                app.message_focus[app.current].is_focused(window),
                input.is_focused(window),
            )
        });
        if row {
            order.push("message list");
        }
        if text {
            order.push("message box");
        }
    }
    // Around the window and back: the list (left) always comes before the
    // message box (middle), each reached once a round.
    assert_eq!(order, ["message list", "message box", "message list", "message box"]);
}

#[gpui::test]
fn shift_f10_opens_a_messages_actions_menu(cx: &mut TestAppContext) {
    let (app, cx) = open(cx);
    cx.run_until_parked();
    let row = app.read_with(cx, |app, _| app.message_focus[0].clone());
    cx.update(|window, cx| window.focus(&row, cx));
    cx.simulate_keystrokes("shift-f10");
    app.read_with(cx, |app, _| assert_eq!(app.menu, Some(Menu::MessageActions(0))));
    cx.simulate_keystrokes("escape");
    app.read_with(cx, |app, _| assert_eq!(app.menu, None));
}

/// Open Recent in the macOS menu bar: file names, with their folders only
/// where two files share a name; the menu actions reach the main window.
#[cfg(target_os = "macos")]
#[gpui::test]
fn menu_bar_recent_files_and_actions(cx: &mut TestAppContext) {
    let (app, cx) = open(cx);
    app.update(cx, |app, _| {
        app.settings.recent_files = vec![
            PathBuf::from("/tmp/a/notes.badge"),
            PathBuf::from("/tmp/b/notes.badge"),
            PathBuf::from("/tmp/a/hello.badge"),
        ];
        let labels: Vec<String> = app.recent_menu_entries().into_iter().map(|(l, _)| l).collect();
        assert_eq!(labels.len(), 3);
        assert!(
            labels[0].starts_with("notes.badge (") && labels[0].ends_with("a)"),
            "{labels:?}"
        );
        assert!(
            labels[1].starts_with("notes.badge (") && labels[1].ends_with("b)"),
            "{labels:?}"
        );
        assert_eq!(labels[2], "hello.badge");
    });
    cx.update(|window, cx| {
        app.update(cx, |app, cx| app.run_menu_action(&ShowSettings, window, cx));
    });
    app.read_with(cx, |app, _| assert!(matches!(app.dialog, Some(Dialog::Settings))));
    cx.update(|window, cx| {
        app.update(cx, |app, cx| app.run_menu_action(&ShowShortcuts, window, cx));
    });
    app.read_with(cx, |app, _| assert!(matches!(app.dialog, Some(Dialog::Shortcuts))));
    cx.update(|window, cx| {
        app.update(cx, |app, cx| app.run_menu_action(&ShowLicenses, window, cx));
    });
    app.read_with(cx, |app, _| assert!(matches!(app.dialog, Some(Dialog::Licenses(0)))));
    cx.update(|window, cx| {
        app.update(cx, |app, cx| app.run_menu_action(&ClearRecentFiles, window, cx));
    });
    app.read_with(cx, |app, _| assert!(app.recent_menu_entries().is_empty()));
}

/// The macOS app menu's About item sends this action.
#[cfg(target_os = "macos")]
#[gpui::test]
fn show_about_opens_the_about_dialog(cx: &mut TestAppContext) {
    let (app, cx) = open(cx);
    cx.dispatch_action(ShowAbout);
    cx.run_until_parked();
    app.read_with(cx, |app, _| assert!(matches!(app.dialog, Some(Dialog::About))));
}

#[gpui::test]
fn tab_stays_inside_the_settings_dialog(cx: &mut TestAppContext) {
    let (app, cx) = open(cx);
    cx.dispatch_action(ShowSettings);
    cx.run_until_parked();
    app.read_with(cx, |app, _| assert!(matches!(app.dialog, Some(Dialog::Settings))));
    let mut stops: Vec<gpui::FocusHandle> = Vec::new();
    for _ in 0..20 {
        cx.dispatch_action(FocusNext);
        let (inside, focused) = cx.update(|window, cx| {
            let app = app.read(cx);
            (app.modal_focus.contains_focused(window, cx), window.focused(cx))
        });
        assert!(inside, "Tab left the dialog");
        if let Some(f) = focused
            && !stops.contains(&f)
        {
            stops.push(f);
        }
    }
    // System, Light, Dark, the checkbox, Show Again and Done (Show Again is
    // greyed out for a new user, so it's skipped).
    assert!(stops.len() >= 5, "only {} controls reached", stops.len());
}

#[gpui::test]
fn the_settings_and_shortcuts_keys_work(cx: &mut TestAppContext) {
    let (app, cx) = open(cx);
    cx.run_until_parked();
    let m = if cfg!(target_os = "macos") { "cmd" } else { "ctrl" };
    for (keys, want) in [
        (format!("{m}-,"), "settings"),
        (format!("{m}-/"), "shortcuts"),
        ("f1".to_string(), "shortcuts"),
    ] {
        cx.simulate_keystrokes(&keys);
        app.read_with(cx, |app, _| {
            let got = match app.dialog {
                Some(Dialog::Settings) => "settings",
                Some(Dialog::Shortcuts) => "shortcuts",
                _ => "nothing",
            };
            assert_eq!(got, want, "{keys}");
        });
        cx.simulate_keystrokes("escape");
        app.read_with(cx, |app, _| assert!(app.dialog.is_none(), "Esc closes it"));
    }
}

/// Ticks every message, drawn as wide as a message can be: more than the
/// badge's memory holds (which editing can't get to, but a file can).
fn fill_past_memory(app: &mut BadgeApp) {
    for m in &mut app.project.messages {
        m.source = Source::Bitmap;
        m.selected = true;
        m.bitmap.set(badge_core::MAX_WIDTH - 1, 0, LedColor::RED);
    }
}

#[gpui::test]
fn opening_a_project_past_the_badges_memory_says_what_is_cut(cx: &mut TestAppContext) {
    let dir = temp_dir("past-memory");
    let path = dir.join("big.badge");
    let (app, cx) = open(cx);
    app.update(cx, |app, _| fill_past_memory(app));
    save(&app, cx, &path);
    app.update_in(cx, |app, window, cx| app.new_project(&NewProject, window, cx));
    open_file(&app, cx, &path);
    app.read_with(cx, |app, _| {
        let (status, sev) = app.status.clone().unwrap();
        assert!(matches!(sev, Severity::Error));
        assert!(status.starts_with("Opened big.badge."), "{status}");
        assert!(status.contains("leaves out messages 6, 7 and 8"), "{status}");
    });
    let _ = std::fs::remove_dir_all(dir);
}

#[gpui::test]
fn sending_past_the_badges_memory_asks_first(cx: &mut TestAppContext) {
    let (app, cx) = open(cx);
    app.update_in(cx, |app, window, cx| {
        app.set_devices(
            vec![badge_io::DeviceInfo {
                id: "test".into(),
                label: "USB badge 1".into(),
                kind: DeviceKind::Usb,
            }],
            cx,
        );
        app.device = Some("test".into());
        fill_past_memory(app);
        app.send(&SendToBadge, window, cx);
        assert!(!app.is_sending(), "not sent before it's answered");
        let Some(Dialog::Confirm {
            title, confirm_label, ..
        }) = &app.dialog
        else {
            panic!("no question");
        };
        assert!(title.contains("don't all fit"));
        assert_eq!(confirm_label.as_ref(), "Send Anyway");
    });
}

#[gpui::test]
fn getting_started_shows_until_a_badge_is_connected(cx: &mut TestAppContext) {
    let (app, cx) = open(cx);
    app.update(cx, |app, cx| {
        assert!(app.render_getting_started(cx).is_some(), "shown to a new user");
        app.set_devices(
            vec![badge_io::DeviceInfo {
                id: "test".into(),
                label: "USB badge 1".into(),
                kind: DeviceKind::Usb,
            }],
            cx,
        );
        assert!(app.settings.seen_badge);
        assert!(app.render_getting_started(cx).is_none());
        // Show Again is unavailable while the badge is plugged in, and the
        // card stays hidden even if the settings are reset.
        assert!(!app.can_show_getting_started_again());
        app.settings.seen_badge = false;
        assert!(app.render_getting_started(cx).is_none());
        // Unplugged: the card is back, and so would Show Again be once it
        // has been closed.
        app.settings.seen_badge = true;
        app.set_devices(Vec::new(), cx);
        assert!(app.can_show_getting_started_again());
        app.settings.seen_badge = false;
        assert!(app.render_getting_started(cx).is_some());
        assert!(!app.can_show_getting_started_again(), "already showing");
    });
}

/// A USB badge comes before a serial port remembered from the last send
/// (the Steam Deck has one of its own), but a port picked in the device
/// menu stays picked.
#[gpui::test]
fn a_usb_badge_is_preferred_to_a_remembered_serial_port(cx: &mut TestAppContext) {
    let device = |kind, id: &str, label: &str| badge_io::DeviceInfo {
        kind,
        id: id.into(),
        label: label.into(),
    };
    // Made-up IDs, not paths: a USB badge's path is opened to check access.
    let serial = device(DeviceKind::Serial, "test-serial", "Serial ttyACM0");
    let usb = device(DeviceKind::Usb, "test-usb-1", "USB badge 1");
    let (app, cx) = open(cx);
    app.update(cx, |app, cx| {
        // At launch, with the serial port remembered.
        app.device = Some(serial.id.clone());
        app.set_devices(vec![usb.clone(), serial.clone()], cx);
        assert_eq!(app.device.as_ref(), Some(&usb.id), "the USB badge at launch");

        // Without a USB badge, the serial port is used; plugging the badge
        // in then switches to it.
        app.set_devices(vec![serial.clone()], cx);
        assert_eq!(app.device.as_ref(), Some(&serial.id));
        app.set_devices(vec![usb.clone(), serial.clone()], cx);
        assert_eq!(app.device.as_ref(), Some(&usb.id), "the badge, once plugged in");

        // Picked in the menu: kept, also when the devices change.
        app.device = Some(serial.id.clone());
        app.device_chosen = true;
        let other = device(DeviceKind::Usb, "test-usb-2", "USB badge 2");
        app.set_devices(vec![usb.clone(), other, serial.clone()], cx);
        assert_eq!(app.device.as_ref(), Some(&serial.id), "the port picked");
    });
}

/// The preview pauses while a system file dialog is open, and starts again
/// once it's answered or canceled.
#[gpui::test]
fn system_dialogs_pause_the_preview_until_answered(cx: &mut TestAppContext) {
    let (app, cx) = open(cx);
    let (answer_tx, answer_rx) = futures::channel::oneshot::channel::<u32>();
    let (cancel_tx, cancel_rx) = futures::channel::oneshot::channel::<u32>();
    let (answered, canceled) = app.update(cx, |app, cx| {
        (
            app.during_system_dialog(answer_rx, cx),
            app.during_system_dialog(cancel_rx, cx),
        )
    });
    app.read_with(cx, |app, _| assert_eq!(app.system_dialogs, 2));

    answer_tx.send(7).unwrap();
    cx.run_until_parked();
    app.read_with(cx, |app, _| assert_eq!(app.system_dialogs, 1));
    assert_eq!(futures::executor::block_on(answered), Ok(7), "the answer is passed on");

    drop(cancel_tx);
    cx.run_until_parked();
    app.read_with(cx, |app, _| assert_eq!(app.system_dialogs, 0));
    assert!(
        futures::executor::block_on(canceled).is_err(),
        "still reads as canceled"
    );
}

/// Pixels → Center: to column 0 where the badge centers the message itself,
/// to the middle of the display elsewhere; undoable.
#[gpui::test]
fn center_moves_the_drawing_where_the_badge_shows_it_centered(cx: &mut TestAppContext) {
    let (app, cx) = open(cx);
    let status = |app: &BadgeApp| app.status.as_ref().map(|(t, _)| t.to_string()).unwrap_or_default();
    let lit = |app: &BadgeApp| (0..60).filter(|&x| app.msg().bitmap.is_on(x, 5)).collect::<Vec<_>>();
    // Each step on its own, as separate clicks would be.
    fn step(app: &Entity<BadgeApp>, cx: &mut VisualTestContext, f: impl FnOnce(&mut BadgeApp, &mut Context<BadgeApp>)) {
        app.update(cx, f);
        cx.run_until_parked();
    }
    step(&app, cx, |app, _| {
        let m = app.msg_mut();
        m.source = Source::Bitmap;
        m.mode = Mode::Fixed;
        m.bitmap.clear();
        (10..16).for_each(|x| m.bitmap.set(x, 5, LedColor::RED));
    });
    // Fixed centers it itself: from column 0, then centered by the badge.
    step(&app, cx, |app, cx| app.center_drawing(cx));
    app.read_with(cx, |app, _| assert_eq!(lit(app), (0..6).collect::<Vec<_>>()));
    assert_eq!(preview::centering_offset(Mode::Fixed, 6, 44), 19);
    step(&app, cx, |app, cx| app.center_drawing(cx));
    app.read_with(cx, |app, _| assert_eq!(status(app), "Already centered."));
    // A scroll doesn't: in the middle of the 44-wide display.
    step(&app, cx, |app, _| app.msg_mut().mode = Mode::Left);
    step(&app, cx, |app, cx| app.center_drawing(cx));
    app.read_with(cx, |app, _| assert_eq!(lit(app), (19..25).collect::<Vec<_>>()));
    cx.dispatch_action(Undo);
    cx.run_until_parked();
    app.read_with(cx, |app, _| assert_eq!(lit(app), (0..6).collect::<Vec<_>>(), "undone"));
    step(&app, cx, |app, cx| {
        app.msg_mut().bitmap.clear();
        app.center_drawing(cx);
    });
    app.read_with(cx, |app, _| {
        assert_eq!(status(app), "Nothing to center: the drawing is empty.")
    });
}

/// Text that doesn't fit, in the message or in the badge's memory, isn't
/// taken; taking text away always is.
#[gpui::test]
fn text_that_does_not_fit_is_refused(cx: &mut TestAppContext) {
    let (app, cx) = open(cx);
    let status = |app: &BadgeApp| app.status.as_ref().map(|(t, _)| t.to_string()).unwrap_or_default();
    app.update(cx, |app, cx| {
        app.apply_text("Hello".into(), None, cx);
        assert_eq!(app.msg().text, "Hello");
        // Far wider than a message can be.
        app.apply_text("W".repeat(2000), None, cx);
        assert_eq!(app.msg().text, "Hello", "refused");
        assert!(status(app).starts_with("Message 1 is full"), "{}", status(app));
        assert_eq!(app.input.read(cx).text(), "Hello", "the text box is put back");
        // Five other messages, as wide as a message can be, fill the memory.
        for i in 1..6 {
            let m = &mut app.project.messages[i];
            m.source = Source::Bitmap;
            m.selected = true;
            m.bitmap.set(badge_core::MAX_WIDTH - 1, 0, LedColor::RED);
        }
        app.apply_text("Hello there".into(), None, cx);
        assert_eq!(app.msg().text, "Hello", "no room in memory");
        assert!(status(app).starts_with("The badge's memory is full"), "{}", status(app));
        // Shorter is fine, over the limit or not.
        app.apply_text("Hell".into(), None, cx);
        assert_eq!(app.msg().text, "Hell");
    });
}

/// Drawing can't go past the badge's memory either: painting is refused,
/// Center stops where the room ends, and erasing always works.
#[gpui::test]
fn drawings_stay_within_the_badges_memory(cx: &mut TestAppContext) {
    let (app, cx) = open(cx);
    let status = |app: &BadgeApp| app.status.as_ref().map(|(t, _)| t.to_string()).unwrap_or_default();
    app.update(cx, |app, cx| {
        let m = app.msg_mut();
        m.source = Source::Bitmap;
        m.mode = Mode::Left;
        m.bitmap.clear();
        // Messages 2-6 ticked, using 2558 of the 2560 bytes: room for 16
        // columns in message 1.
        for (i, bytes) in [(1, 512), (2, 512), (3, 512), (4, 512), (5, 510)] {
            let o = &mut app.project.messages[i];
            o.source = Source::Bitmap;
            o.selected = true;
            o.bitmap.set(bytes * 8 - 1, 0, LedColor::RED);
        }
        assert_eq!(app.room_for_current(), 16);
        app.paint_at(15, 5, LedColor::RED, cx);
        assert!(app.msg().bitmap.is_on(15, 5), "within the room");
        app.paint_at(16, 5, LedColor::RED, cx);
        assert!(!app.msg().bitmap.is_on(16, 5), "refused");
        assert!(status(app).starts_with("The badge's memory is full"), "{}", status(app));
        // Center would put a 6-column drawing at 19; it stops at 10.
        app.msg_mut().bitmap.clear();
        (0..6).for_each(|x| app.paint_at(x, 5, LedColor::RED, cx));
        app.center_drawing(cx);
        let lit: Vec<usize> = (0..30).filter(|&x| app.msg().bitmap.is_on(x, 5)).collect();
        assert_eq!(lit, (10..16).collect::<Vec<_>>());
        // No room left at all: nothing past the drawing's 16 columns, but
        // inside them painting needs no new room, and erasing always works.
        app.project.messages[5].bitmap.set(512 * 8 - 1, 0, LedColor::RED);
        assert_eq!(app.room_for_current(), 0);
        app.paint_at(20, 5, LedColor::RED, cx);
        assert!(!app.msg().bitmap.is_on(20, 5), "refused");
        app.paint_at(0, 0, LedColor::RED, cx);
        assert!(app.msg().bitmap.is_on(0, 0), "inside its width");
        app.paint_at(10, 5, LedColor::OFF, cx);
        assert!(!app.msg().bitmap.is_on(10, 5), "erased");
    });
}

/// Ticking, duplicating and pasting a message the badge's memory has no room
/// for are refused; a paste that makes things no worse is kept.
#[gpui::test]
fn ticking_duplicating_and_pasting_stay_within_the_memory(cx: &mut TestAppContext) {
    let (app, cx) = open(cx);
    let status = |app: &BadgeApp| app.status.as_ref().map(|(t, _)| t.to_string()).unwrap_or_default();
    let wide = |bytes: usize| {
        let mut m = badge_core::Message {
            source: Source::Bitmap,
            ..Default::default()
        };
        m.bitmap.set_height(11);
        m.bitmap.set(bytes * 8 - 1, 0, LedColor::RED);
        m
    };
    app.update(cx, |app, cx| {
        for m in &mut app.project.messages {
            *m = badge_core::Message::default();
            m.bitmap.set_height(11);
        }
        // Messages 1-5 ticked and full (2560 bytes); message 6 unticked.
        for i in 0..5 {
            app.project.messages[i] = badge_core::Message {
                selected: true,
                ..wide(512)
            };
        }
        app.project.messages[5] = badge_core::Message {
            selected: false,
            ..wide(8)
        };
        // Ticking message 6 is refused.
        app.set_ticked(5, true, cx);
        assert!(!app.project.messages[5].selected);
        assert!(status(app).starts_with("The badge's memory is full"));
        // Unticking always works.
        app.set_ticked(0, false, cx);
        assert!(!app.project.messages[0].selected);
        app.set_ticked(0, true, cx);
        assert!(app.project.messages[0].selected, "room again for its own space");
        // Duplicating a ticked message is refused; an unticked one is fine.
        app.status = None;
        app.duplicate_message(1, cx);
        assert!(app.project.messages[6].is_blank(), "not duplicated");
        assert!(status(app).starts_with("The badge's memory is full"));
        app.duplicate_message(5, cx);
        assert!(!app.project.messages[6].is_blank(), "an unticked copy takes no room");
    });
    // Pasting: a ticked wide message over the small unticked one is refused;
    // over a full one, it uses no more, so it's kept.
    app.update(cx, |app, cx| {
        app.project.messages[7] = badge_core::Message {
            selected: true,
            ..wide(100)
        };
        app.copy_message(7, cx);
        app.project.messages[7] = badge_core::Message::default();
        app.project.messages[7].bitmap.set_height(11);
        app.paste_message(5, cx);
        assert!(!app.project.messages[5].selected, "refused: message 6 unchanged");
        assert_eq!(app.project.messages[5].bitmap.used_width(), 64);
        app.paste_message(2, cx);
        assert_eq!(
            app.project.messages[2].bitmap.used_width(),
            800,
            "kept: smaller than what it replaced"
        );
    });
}

/// Switching to a badge type the messages don't fit in, re-drawn for its
/// height, is refused; with room made, it goes ahead.
#[gpui::test]
fn switching_badge_type_stays_within_the_memory(cx: &mut TestAppContext) {
    let (app, cx) = open(cx);
    let status = |app: &BadgeApp| app.status.as_ref().map(|(t, _)| t.to_string()).unwrap_or_default();
    app.update(cx, |app, cx| {
        app.set_card(CardType::Mono11, cx);
        for m in &mut app.project.messages {
            *m = badge_core::Message::default();
            m.bitmap.set_height(11);
        }
        app.current = 0;
        // Icons are as wide as the badge is tall: 11 columns each here, 16
        // on a 16-row badge.
        app.apply_text("~f~f~f~f~f~f~f~f".into(), None, cx);
        let w11 = app.msg().bitmap.used_width();
        let msg = app.msg().clone();
        let w16 = app.renderer.render(&msg, CardType::Mono16).used_width();
        assert!(
            w16 > w11,
            "taller text is wider ({w11} vs {w16}), or this test proves nothing"
        );
        // Fill the rest of the memory exactly, with drawings in messages 2-8.
        let mut left = badge_core::protocol::MAX_TOTAL_COLUMNS - w11 / 8;
        for i in 1..8 {
            let bytes = left.min(512);
            if bytes == 0 {
                break;
            }
            let m = &mut app.project.messages[i];
            m.source = Source::Bitmap;
            m.selected = true;
            m.bitmap.set(bytes * 8 - 1, 0, LedColor::RED);
            left -= bytes;
        }
        assert_eq!(left, 0);
        app.set_card(CardType::Mono16, cx);
        assert_eq!(app.project.card, CardType::Mono11, "refused");
        assert_eq!(app.msg().bitmap.used_width(), w11, "nothing changed");
        assert!(status(app).starts_with("Can't switch to"), "{}", status(app));
        // Room made: it switches.
        app.set_ticked(1, false, cx);
        app.set_card(CardType::Mono16, cx);
        assert_eq!(app.project.card, CardType::Mono16);
        assert_eq!(app.msg().bitmap.used_width(), w16);
    });
}

/// Makes `path` look changed by another program: new contents, and a later
/// time than the app knows.
fn change_elsewhere(path: &Path, text: &str) {
    let mut project = badge_formats::open(path).unwrap();
    project.messages[0].text = text.into();
    badge_formats::save_project(&project, path).unwrap();
    let later = std::time::SystemTime::now() + Duration::from_secs(10);
    std::fs::File::options()
        .write(true)
        .open(path)
        .unwrap()
        .set_modified(later)
        .unwrap();
}

#[gpui::test]
fn saving_over_a_file_changed_elsewhere_asks_whose_to_keep(cx: &mut TestAppContext) {
    let dir = temp_dir("changed-elsewhere");
    let path = dir.join("mine.badge");
    let (app, cx) = open(cx);
    type_text(&app, cx, "Mine");
    save(&app, cx, &path);
    app.read_with(cx, |app, _| assert!(!app.changed_elsewhere(), "its own save"));
    // A later time with the same contents (as macOS's autosaving can
    // leave) isn't a change.
    let later = std::time::SystemTime::now() + Duration::from_secs(10);
    std::fs::File::options()
        .write(true)
        .open(&path)
        .unwrap()
        .set_modified(later)
        .unwrap();
    app.read_with(cx, |app, _| assert!(!app.changed_elsewhere(), "same contents"));

    // Keep Mine saves over it.
    type_text(&app, cx, "Mine again");
    change_elsewhere(&path, "Theirs");
    cx.dispatch_action(SaveProject);
    app.read_with(cx, |app, _| {
        assert!(matches!(
            app.dialog,
            Some(Dialog::Confirm {
                alternate_label: Some(_),
                ..
            })
        ));
    });
    app.update_in(cx, |app, window, cx| app.accept_dialog(window, cx));
    assert_eq!(badge_formats::open(&path).unwrap().messages[0].text, "Mine again");
    app.read_with(cx, |app, _| assert!(!app.changed_elsewhere()));

    // Use Theirs opens the file as it is.
    type_text(&app, cx, "Lost");
    change_elsewhere(&path, "Theirs");
    cx.dispatch_action(SaveProject);
    app.update_in(cx, |app, window, cx| app.accept_alternate(window, cx));
    app.read_with(cx, |app, _| {
        assert_eq!(app.msg().text, "Theirs");
        assert!(!app.is_dirty());
    });
    assert_eq!(badge_formats::open(&path).unwrap().messages[0].text, "Theirs");
    let _ = std::fs::remove_dir_all(dir);
}

#[gpui::test]
fn unsaved_work_is_kept_until_saved_and_can_be_recovered(cx: &mut TestAppContext) {
    let dir = temp_dir("recovery");
    let copy = dir.join("unsaved-work.json");
    let (app, cx) = open(cx);
    app.update(cx, |app, _| app.recovery_path = Some(copy.clone()));

    // A new project's changes are kept.
    type_text(&app, cx, "Unsaved");
    app.update(cx, |app, _| app.keep_recovery_copy());
    let kept = recovery::read(&copy).expect("a copy is kept");
    assert_eq!(kept.project.messages[0].text, "Unsaved");
    assert_eq!(kept.file, None);

    // Saving removes it.
    save(&app, cx, &dir.join("saved.badge"));
    assert!(!copy.exists(), "nothing unsaved to keep");

    // Left by an app that closed: offered, and left alone until answered,
    // while the current work still gets its own copy.
    let earlier = dir.join("earlier.json");
    app.update_in(cx, |app, window, cx| app.new_project(&NewProject, window, cx));
    type_text(&app, cx, "From before");
    app.update(cx, |app, _| {
        app.recovery_path = Some(earlier.clone());
        app.keep_recovery_copy();
        app.recovered = recovery::read(&earlier);
        app.recovery_path = Some(copy.clone());
        app.recovery_written = None;
    });
    type_text(&app, cx, "Something else");
    app.update(cx, |app, _| app.keep_recovery_copy());
    assert_eq!(
        recovery::read(&earlier).unwrap().project.messages[0].text,
        "From before",
        "not overwritten while offered"
    );
    assert_eq!(
        recovery::read(&copy).unwrap().project.messages[0].text,
        "Something else",
        "the current work is kept meanwhile"
    );

    // Recover brings it back, unsaved (the current project's changes are
    // asked about first: Don't Save).
    app.update_in(cx, |app, window, cx| app.recover_unsaved_work(window, cx));
    app.update_in(cx, |app, window, cx| app.accept_alternate(window, cx));
    app.read_with(cx, |app, _| {
        assert_eq!(app.msg().text, "From before");
        assert!(app.is_dirty());
        assert!(app.recovered.is_none());
    });
    assert!(!earlier.exists(), "the recovered copy is removed");
    assert_eq!(
        recovery::read(&copy).unwrap().project.messages[0].text,
        "From before",
        "once it's kept in this app's own"
    );
    let _ = std::fs::remove_dir_all(dir);
}

/// Quitting with nothing to keep removes the copy and the lock file; one
/// that can't be removed is left (for a later start) without trouble.
#[gpui::test]
fn quitting_removes_the_copy_and_its_lock_file(cx: &mut TestAppContext) {
    let dir = temp_dir("recovery-quit");
    let copy = dir.join("1-2-0.json");
    let lock = copy.with_extension("lock");
    std::fs::write(&lock, "").unwrap();
    let (app, cx) = open(cx);
    app.update(cx, |app, _| app.recovery_path = Some(copy.clone()));
    type_text(&app, cx, "Unsaved");
    app.update(cx, |app, _| app.keep_recovery_copy());
    assert!(copy.exists());
    // What quitting does once its question is answered (Don't Save).
    app.update(cx, |app, _| {
        app.saved = Some(app.project.clone());
        app.settle_recovery_before_quitting();
    });
    assert!(!copy.exists() && !lock.exists());

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::write(&lock, "").unwrap();
        std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o555)).unwrap();
        // Root may change the folder anyway.
        let root = std::fs::write(dir.join("probe"), "").is_ok();
        app.update(cx, |app, _| app.settle_recovery_before_quitting());
        std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o755)).unwrap();
        assert!(lock.exists() || root);
    }
    let _ = std::fs::remove_dir_all(dir);
}

#[gpui::test]
fn closing_the_window_with_nothing_to_ask_removes_the_lock_file(cx: &mut TestAppContext) {
    let dir = temp_dir("recovery-close");
    let copy = dir.join("1-2-0.json");
    let lock = copy.with_extension("lock");
    std::fs::write(&lock, "").unwrap();
    cx.update(|cx| cx.bind_keys(bindings()));
    let (app, cx) = cx.add_window_view(|window, cx| {
        BadgeApp::install_close_guard(window, cx);
        BadgeApp::new(None, window, cx)
    });
    app.update(cx, |app, _| app.recovery_path = Some(copy.clone()));
    // The system's close (the red button, Alt+F4), with nothing unsaved.
    cx.simulate_close();
    assert!(!lock.exists());
    let _ = std::fs::remove_dir_all(dir);
}

#[gpui::test]
fn quitting_from_outside_the_app_removes_the_lock_file(cx: &mut TestAppContext) {
    let dir = temp_dir("recovery-outside");
    let copy = dir.join("1-2-0.json");
    let lock = copy.with_extension("lock");
    std::fs::write(&lock, "").unwrap();
    let (app, cx) = open(cx);
    app.update(cx, |app, _| app.recovery_path = Some(copy.clone()));
    // As macOS's quit from the Dock does when there's nothing to ask.
    cx.cx.update(|cx| cx.shutdown());
    assert!(!lock.exists());
    let _ = std::fs::remove_dir_all(dir);
}

#[gpui::test]
fn recovered_work_stays_on_disk_when_it_cant_be_kept_again(cx: &mut TestAppContext) {
    let dir = temp_dir("recovery-unkept");
    let earlier = dir.join("earlier.json");
    let (app, cx) = open(cx);
    type_text(&app, cx, "From before");
    app.update(cx, |app, _| {
        app.recovery_path = Some(earlier.clone());
        app.keep_recovery_copy();
        app.recovered = recovery::read(&earlier);
        // This app has nowhere to keep a copy (its lock failed).
        app.recovery_path = None;
        app.recovery_written = None;
    });
    app.update_in(cx, |app, window, cx| app.recover_unsaved_work(window, cx));
    app.update_in(cx, |app, window, cx| app.accept_alternate(window, cx));
    app.read_with(cx, |app, _| assert_eq!(app.msg().text, "From before"));
    assert!(earlier.exists(), "still there for the next start");
    let _ = std::fs::remove_dir_all(dir);
}

#[gpui::test]
fn discarding_recovered_work_keeps_only_the_current_work(cx: &mut TestAppContext) {
    let dir = temp_dir("recovery-discard");
    let copy = dir.join("unsaved-work.json");
    let (app, cx) = open(cx);
    app.update(cx, |app, _| app.recovery_path = Some(copy.clone()));
    type_text(&app, cx, "From before");
    app.update(cx, |app, _| {
        app.keep_recovery_copy();
        app.recovered = recovery::read(&copy);
    });
    type_text(&app, cx, "Current");
    app.update(cx, |app, cx| app.discard_recovered_work(cx));
    app.read_with(cx, |app, _| assert!(app.recovered.is_none()));
    // The copy is of the current unsaved work now, not the discarded work.
    assert_eq!(recovery::read(&copy).unwrap().project.messages[0].text, "Current");
    // Nothing unsaved: no copy.
    app.update_in(cx, |app, window, cx| app.new_project(&NewProject, window, cx));
    app.update_in(cx, |app, window, cx| app.accept_alternate(window, cx));
    assert!(!copy.exists());
    let _ = std::fs::remove_dir_all(dir);
}

#[cfg(target_os = "macos")]
#[gpui::test]
fn reverting_to_an_earlier_version_is_an_edit(cx: &mut TestAppContext) {
    let dir = temp_dir("revert");
    let path = dir.join("mine.badge");
    let (app, cx) = open(cx);
    type_text(&app, cx, "Saved");
    save(&app, cx, &path);
    // Last Saved Version.
    type_text(&app, cx, "Changed");
    app.read_with(cx, |app, _| {
        let menu = app.file_menu();
        assert!(menu.can_revert);
        assert_eq!(menu.revert_label, "Last Saved Version");
    });
    app.update(cx, |app, cx| app.revert_to_saved(cx));
    app.read_with(cx, |app, _| {
        assert_eq!(app.msg().text, "Saved");
        assert!(!app.is_dirty());
        assert!(!app.file_menu().can_revert);
    });
    // A version macOS kept: shown, unsaved until saved.
    let version = dir.join("version.badge");
    let mut older = badge_formats::open(&path).unwrap();
    older.messages[0].text = "Older".into();
    badge_formats::save_project(&older, &version).unwrap();
    app.update_in(cx, |app, window, cx| app.revert_to_version(version, window, cx));
    app.read_with(cx, |app, _| {
        assert_eq!(app.msg().text, "Older");
        assert!(app.is_dirty());
    });
    let _ = std::fs::remove_dir_all(dir);
}

/// Each save keeps the file as it was (the app's own store, as on Linux and
/// Windows), and Revert To brings an earlier save back as an edit.
#[gpui::test]
fn saving_keeps_earlier_versions_to_revert_to(cx: &mut TestAppContext) {
    let dir = temp_dir("versions");
    let path = dir.join("mine.badge");
    let (app, cx) = open(cx);
    app.update(cx, |app, _| app.versions_dir = Some(dir.join("versions")));
    for text in ["First", "Second", "Third"] {
        type_text(&app, cx, text);
        save(&app, cx, &path);
    }
    app.read_with(cx, |app, _| {
        assert_eq!(app.file_versions.len(), 2, "the first and second saves");
        assert!(app.file_versions[0].0.starts_with("Today at "));
        assert!(!app.can_revert(), "nothing unsaved");
    });
    let older = app.read_with(cx, |app, _| app.file_versions[1].1.clone());
    app.update_in(cx, |app, window, cx| app.revert_to_version(older, window, cx));
    app.read_with(cx, |app, _| {
        assert_eq!(app.msg().text, "First");
        assert!(app.is_dirty(), "unsaved until saved");
        assert!(app.can_revert());
    });
    assert_eq!(
        badge_formats::open(&path).unwrap().messages[0].text,
        "Third",
        "the file is unchanged"
    );
    cx.dispatch_action(Undo);
    app.read_with(cx, |app, _| assert_eq!(app.msg().text, "Third"));
    let _ = std::fs::remove_dir_all(dir);
}

#[gpui::test]
fn only_the_newest_versions_are_kept(cx: &mut TestAppContext) {
    let _ = cx;
    let dir = temp_dir("versions-kept");
    let file = dir.join("mine.badge");
    let store = dir.join("versions");
    for i in 0..15 {
        std::fs::write(&file, format!("{i}")).unwrap();
        versions::store::keep(&store, &file);
        // Versions are named after the millisecond they were kept in.
        std::thread::sleep(Duration::from_millis(2));
    }
    let kept = versions::store::list(&store, &file);
    assert_eq!(kept.len(), 10);
    assert_eq!(std::fs::read_to_string(&kept[0].1).unwrap(), "14", "newest first");
    let _ = std::fs::remove_dir_all(dir);
}

/// Coming back to the window after another program changed the file: it
/// isn't opened again, even with nothing unsaved here; the status bar says
/// so, and saving is left to ask whose changes to keep.
#[gpui::test]
fn a_file_changed_elsewhere_is_not_reloaded(cx: &mut TestAppContext) {
    let dir = temp_dir("reload");
    let path = dir.join("mine.badge");
    let (app, cx) = open(cx);
    type_text(&app, cx, "Mine");
    save(&app, cx, &path);
    change_elsewhere(&path, "Theirs");
    app.update(cx, |app, cx| app.check_file_changed_elsewhere(cx));
    app.read_with(cx, |app, _| {
        assert_eq!(app.msg().text, "Mine", "nothing unsaved, still not replaced");
        assert!(!app.is_dirty());
        assert!(app.changed_elsewhere());
        assert!(
            app.status
                .as_ref()
                .is_some_and(|(text, _)| text.contains("Another program changed"))
        );
    });

    type_text(&app, cx, "Unsaved");
    change_elsewhere(&path, "Theirs again");
    app.update(cx, |app, cx| app.check_file_changed_elsewhere(cx));
    app.read_with(cx, |app, _| {
        assert_eq!(app.msg().text, "Unsaved", "unsaved work isn't replaced");
        assert!(app.changed_elsewhere());
    });
    let _ = std::fs::remove_dir_all(dir);
}

/// An image file whose lit pixels are `lit`, `height` rows tall.
fn image_file(dir: &Path, name: &str, height: usize, lit: &[(usize, usize)]) -> PathBuf {
    let mut fb = Framebuffer::new(height);
    for &(x, y) in lit {
        fb.set(x, y, LedColor::WHITE);
    }
    let path = dir.join(name);
    badge_render::export_png(&fb, true, &path, 1).unwrap();
    path
}

/// Images are read and decoded in the background, and the latest one
/// loaded is the one that ends up in the message.
#[gpui::test]
fn images_load_in_the_background_and_the_last_one_wins(cx: &mut TestAppContext) {
    let dir = temp_dir("image-load");
    let first = image_file(&dir, "first.png", 11, &[(2, 3)]);
    let second = image_file(&dir, "second.png", 11, &[(5, 1)]);
    let (app, cx) = open(cx);
    app.update(cx, |app, cx| {
        app.import_image_from(&first, cx);
        app.import_image_from(&second, cx);
    });
    cx.run_until_parked();
    app.read_with(cx, |app, _| {
        let m = app.msg();
        assert_eq!(m.source, Source::Bitmap);
        assert!(m.bitmap.is_on(5, 1), "the second image");
        assert!(!m.bitmap.is_on(2, 3), "not the first");
        assert!(app.live_import().is_some());
    });
    let _ = std::fs::remove_dir_all(dir);
}

/// An image goes into the message it was loaded for, wherever that message
/// is moved meanwhile, and options changed meanwhile apply to it.
#[gpui::test]
fn an_image_follows_its_message_while_it_loads(cx: &mut TestAppContext) {
    let dir = temp_dir("image-follow");
    let first = image_file(&dir, "first.png", 11, &[(2, 3)]);
    let second = image_file(&dir, "second.png", 11, &[(5, 1)]);
    let (app, cx) = open(cx);
    app.update(cx, |app, cx| {
        app.import_image_from(&first, cx);
        app.move_message(0, 2, cx);
    });
    cx.run_until_parked();
    app.read_with(cx, |app, _| {
        assert!(
            app.project.messages[2].bitmap.is_on(2, 3),
            "moved, and the image with it"
        );
        assert_eq!(app.project.messages[0].source, Source::Text);
        assert!(app.live_import().is_some());
    });
    // A second image into the same message, with the threshold changed
    // (which redoes the first one) while it loads.
    app.update(cx, |app, cx| {
        app.import_image_from(&second, cx);
        app.set_threshold(100, cx);
    });
    cx.run_until_parked();
    app.read_with(cx, |app, _| assert!(app.msg().bitmap.is_on(5, 1), "the second image"));
    let _ = std::fs::remove_dir_all(dir);
}

/// An image whose message was replaced while it loaded (New, Open, Undo, a
/// paste) is dropped, not put into whatever message is there now.
#[gpui::test]
fn an_image_is_dropped_when_its_message_is_replaced(cx: &mut TestAppContext) {
    let dir = temp_dir("image-replaced");
    let image = image_file(&dir, "image.png", 11, &[(2, 3)]);
    let (app, cx) = open(cx);
    app.update_in(cx, |app, window, cx| {
        app.import_image_from(&image, cx);
        app.new_project(&NewProject, window, cx);
    });
    cx.run_until_parked();
    app.read_with(cx, |app, _| {
        assert_eq!(app.msg().source, Source::Text);
        assert!(app.status.as_ref().is_some_and(|s| s.0.contains("replaced")));
    });
    let _ = std::fs::remove_dir_all(dir);
}

/// Turning Fit to height off decodes the image again (in the background)
/// at its own size.
#[gpui::test]
fn changing_fit_to_height_decodes_the_image_again(cx: &mut TestAppContext) {
    let dir = temp_dir("image-fit");
    let all: Vec<(usize, usize)> = (0..24).flat_map(|x| (0..16).map(move |y| (x, y))).collect();
    let tall = image_file(&dir, "tall.png", 16, &all);
    let (app, cx) = open(cx);
    app.update(cx, |app, cx| app.import_image_from(&tall, cx));
    cx.run_until_parked();
    let fitted = app.read_with(cx, |app, _| app.msg().bitmap.content_width());
    assert!(fitted < 24, "shrunk to 11 rows, so narrower: {fitted}");

    app.update(cx, |app, cx| {
        app.settings.import_fit_height = false;
        app.reapply_import(EditKind::Other, cx);
    });
    let status = |app: &BadgeApp| app.status.as_ref().map(|s| s.0.to_string());
    app.read_with(cx, |app, _| {
        assert_eq!(
            status(app).as_deref(),
            Some("Loading image\u{2026}"),
            "shows it's working"
        );
    });
    cx.run_until_parked();
    app.read_with(cx, |app, _| assert_eq!(status(app), None, "and then Ready"));
    app.read_with(cx, |app, _| {
        assert_eq!(app.msg().bitmap.content_width(), 24, "at its own size");
        assert!(app.live_import().is_some());
    });
    let _ = std::fs::remove_dir_all(dir);
}
