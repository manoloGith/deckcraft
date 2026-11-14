//! Pickers that answer later ([`Services::picked_later`]): what asked for the picker runs again
//! once the path arrives, and the host's row below the ribbon ([`Services::below_ribbon`]).
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::sync::mpsc::{Sender, channel};
use std::sync::{Arc, Mutex};

use deckcraft_model::ShapeKind;
use serde_json::json;

use super::{Services, SlideApp};
use crate::Session;

fn png() -> Vec<u8> {
    // 1×1 RGB PNG. Enough for `insert.picture` to accept it.
    const HEX: &str =
        "89504e470d0a1a0a0000000d4948445200000001000000010802000000907753de0000000c49444154789c63f8cfc0000003010100c9fe92ef0000000049454e44ae426082";
    (0..HEX.len()).step_by(2).map(|i| u8::from_str_radix(&HEX[i..i + 2], 16).expect("hex")).collect()
}

/// A host like the Android one: the pickers only note what was asked and answer `None`; the
/// test plays the system picker by sending a path.
fn setup() -> (SlideApp, Sender<String>, Arc<Mutex<Vec<String>>>) {
    let (answer, picked_later) = channel();
    let asked = Arc::new(Mutex::new(Vec::new()));
    let (open, save) = (asked.clone(), asked.clone());
    let services = Services {
        pick_open: Some(Box::new(move |purpose: &str| {
            open.lock().expect("log").push(format!("open {purpose}"));
            None
        })),
        pick_save: Some(Box::new(move |name: &str| {
            save.lock().expect("log").push(format!("save {name}"));
            None
        })),
        read: Some(Box::new(|p: &str| std::fs::read(p).map_err(|e| e.to_string()))),
        picked_later: Some(picked_later),
        ..Default::default()
    };
    (SlideApp::new(Session::with_new(), services), answer, asked)
}

fn dir(name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("deckcraft-picked-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("temp dir");
    dir
}

fn pictures(app: &SlideApp) -> usize {
    let d = app.session.active().expect("a presentation");
    d.current_slide().expect("a slide").shapes.iter().filter(|s| matches!(s.kind, ShapeKind::Picture { .. })).count()
}

#[test]
fn a_picture_is_inserted_when_the_picker_answers() {
    let (mut app, answer, asked) = setup();
    let dir = dir("picture");
    let file = dir.join("dot.png");
    std::fs::write(&file, png()).unwrap();
    app.run("app.insertPictureDialog", json!({})).unwrap();
    assert_eq!(*asked.lock().unwrap(), ["open picture"]);
    app.poll_picked();
    assert_eq!(pictures(&app), 0, "nothing happens before the picker answers");
    answer.send(file.to_string_lossy().into_owned()).unwrap();
    app.poll_picked();
    assert_eq!(pictures(&app), 1);
    assert_eq!(asked.lock().unwrap().len(), 1, "the picker is not shown a second time");
    // The answer is spent: the next request shows the picker again.
    app.run("app.insertPictureDialog", json!({})).unwrap();
    assert_eq!(asked.lock().unwrap().len(), 2);
    assert_eq!(pictures(&app), 1);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn save_as_saves_when_the_picker_answers() {
    let (mut app, answer, asked) = setup();
    let dir = dir("save");
    let file = dir.join("Talk.deckcraft");
    app.run("app.saveAsDialog", json!({})).unwrap();
    assert_eq!(asked.lock().unwrap().len(), 1);
    assert!(asked.lock().unwrap()[0].starts_with("save "));
    answer.send(file.to_string_lossy().into_owned()).unwrap();
    app.poll_picked();
    assert!(file.is_file(), "the presentation was written where the picker said");
    let d = app.session.active().unwrap();
    assert_eq!(d.path.as_deref(), Some(file.to_string_lossy().as_ref()));
    assert!(!d.is_dirty());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn an_export_keeps_its_format_and_options() {
    let (mut app, answer, asked) = setup();
    let dir = dir("export");
    let file = dir.join("Talk.pdf");
    assert!(!app.export_as("pdf", json!({"includeHidden": true})), "nothing is written before the picker answers");
    assert!(asked.lock().unwrap()[0].ends_with(".pdf"));
    answer.send(file.to_string_lossy().into_owned()).unwrap();
    app.poll_picked();
    let bytes = std::fs::read(&file).expect("the exported file");
    assert!(bytes.starts_with(b"%PDF"));
    assert!(app.session.active().unwrap().path.is_none(), "an export leaves the presentation's own file alone");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_dismissed_picker_forgets_what_asked() {
    let (mut app, answer, _asked) = setup();
    let dir = dir("dismissed");
    let file = dir.join("dot.png");
    std::fs::write(&file, png()).unwrap();
    app.run("app.insertPictureDialog", json!({})).unwrap();
    answer.send(String::new()).unwrap();
    app.poll_picked();
    // A path that comes with nothing waiting for it is dropped.
    answer.send(file.to_string_lossy().into_owned()).unwrap();
    app.poll_picked();
    assert_eq!(pictures(&app), 0);
    let _ = std::fs::remove_dir_all(&dir);
}

/// Without `picked_later` (the desktop), a `None` from the picker is a cancel and nothing waits.
#[test]
fn a_host_that_answers_at_once_is_unchanged() {
    let dir = dir("sync");
    let file = dir.join("dot.png");
    std::fs::write(&file, png()).unwrap();
    let path = file.to_string_lossy().into_owned();
    let services = Services {
        pick_open: Some(Box::new(move |_: &str| Some(path.clone()))),
        read: Some(Box::new(|p: &str| std::fs::read(p).map_err(|e| e.to_string()))),
        ..Default::default()
    };
    let mut app = SlideApp::new(Session::with_new(), services);
    app.run("app.insertPictureDialog", json!({})).unwrap();
    assert_eq!(pictures(&app), 1);
    assert!(app.asked.is_none());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn the_host_draws_below_the_ribbon_while_a_presentation_is_edited() {
    use egui_kittest::Harness;
    use egui_kittest::kittest::Queryable;

    let services = Services {
        below_ribbon: Some(Box::new(|app: &mut SlideApp, ui: &mut egui::Ui| {
            egui::Panel::top("host_row").show(ui, |ui| {
                if ui.button("Host row").clicked() {
                    app.set_status("the host row was clicked");
                }
            });
        })),
        ..Default::default()
    };
    let app = SlideApp::new(Session::with_new(), services);
    let mut h = Harness::builder().with_size(egui::vec2(1440.0, 900.0)).with_step_dt(1.0 / 60.0).build_ui_state(
        |ui, app: &mut SlideApp| {
            app.logic(ui.ctx());
            app.ui(ui);
        },
        app,
    );
    h.run_steps(4);
    h.get_by_label("Host row").click();
    h.run_steps(3);
    assert_eq!(h.state_mut().fresh_status().as_deref(), Some("the host row was clicked"));
    assert!(h.state().services.below_ribbon.is_some(), "the hook is put back after drawing");
    // Not on the File tab's view.
    h.state_mut().open_backstage(crate::backstage::Page::Home);
    h.run_steps(3);
    assert!(h.query_by_label("Host row").is_none());
}
