//! Saves where a page hosts the web app: Save gives the host its file back, Save As… and Save a
//! Copy… come as new files (`Services::write_as`), exports as the files they are.

use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::Arc;

use craft::host::{Picked, ScriptedFiles};
use serde_json::json;

use crate::{PhotocraftApp, Services, menus};

/// Which writer got each file: `write` or `write_as`, and the name.
type Calls = Rc<RefCell<Vec<(&'static str, String)>>>;

fn record(calls: &Calls, via: &'static str) -> crate::services::WriteFn {
    let calls = calls.clone();
    Box::new(move |path: &str, _: &[u8]| {
        calls.borrow_mut().push((via, path.to_string()));
        Ok(())
    })
}

/// An app as the web shell sets it up when hosted, holding its file dialogs until answered.
fn hosted_app() -> (Box<PhotocraftApp>, Arc<ScriptedFiles>, Calls) {
    let files = Arc::new(ScriptedFiles::held());
    let calls = Calls::default();
    let services = Services {
        import: Some(Box::new(|name: &str, bytes: &[u8], opts: &photocraft_io::ImportOptions| {
            photocraft_io::import_with_options(name, bytes, opts).map(|r| (r.document, r.warnings)).map_err(|e| e.to_string())
        })),
        export: Some(Box::new(|_: &photocraft_doc::Document, _: &str, _: &crate::ExportSettings| Ok((b"out".to_vec(), Vec::new())))),
        write: Some(record(&calls, "write")),
        write_as: Some(record(&calls, "write_as")),
        files: Some(files.clone()),
        ..Default::default()
    };
    (PhotocraftApp::new(photocraft_engine::Session::new(), services), files, calls)
}

/// A small .pcraft file, as a host would send it.
fn pcraft_bytes() -> Vec<u8> {
    let (mut app, _, _) = hosted_app();
    app.run("file.new", json!({"width": 4, "height": 4})).unwrap();
    photocraft_io::export(&app.session.active().unwrap().doc, "one.pcraft", &Default::default()).unwrap().bytes
}

fn answer(app: &mut PhotocraftApp, files: &ScriptedFiles, path: &str) {
    files.answer(Picked::Paths(vec![PathBuf::from(path)])).expect("a dialog is open");
    app.poll_file_dialog();
}

/// A layered file from the host keeps its name as its file: Save writes it straight back, with no
/// dialog, where the same bytes opened as a drop ask for a new file (offered as a PSD).
#[test]
fn a_hosted_pcraft_saves_back_under_its_own_name() {
    let ctx = egui::Context::default();
    let (mut app, files, calls) = hosted_app();
    app.open_hosted_bytes("one.pcraft", &pcraft_bytes()).unwrap();
    assert_eq!(app.session.active().unwrap().path.as_deref(), Some("one.pcraft"));
    menus::invoke(&mut app, &ctx, "file.save", json!({})).unwrap();
    assert!(files.requests().is_empty(), "no dialog: {:?}", files.requests());
    assert_eq!(*calls.borrow(), [("write", "one.pcraft".to_string())]);

    let (mut app, files, _) = hosted_app();
    app.open_bytes("one.pcraft", &pcraft_bytes()).unwrap();
    assert_eq!(app.session.active().unwrap().path, None);
    menus::invoke(&mut app, &ctx, "file.save", json!({})).unwrap();
    assert_eq!(files.requests().last().and_then(|r| r.file_name.clone()).as_deref(), Some("one.psd"));
}

/// A flat file has no file of its own to write back (Save would flatten the document into it),
/// hosted or not.
#[test]
fn a_hosted_flat_image_opens_without_a_file() {
    let (mut app, _, _) = hosted_app();
    app.run("file.new", json!({"width": 4, "height": 4})).unwrap();
    let png = photocraft_io::export(&app.session.active().unwrap().doc, "flat.png", &Default::default()).unwrap().bytes;
    let (mut app, _, _) = hosted_app();
    app.open_hosted_bytes("flat.png", &png).unwrap();
    assert_eq!(app.session.active().unwrap().path, None);
}

/// Save As… and Save a Copy… (also after a format's options) go to `write_as`; Save, and a Save that
/// has to ask for a name, never do (nor do exports, which keep `write`). Without
/// `write_as`, Save As… writes as Save does.
#[test]
fn save_as_is_true_only_for_save_as() {
    let ctx = egui::Context::default();
    let (mut app, files, calls) = hosted_app();
    app.open_hosted_bytes("one.pcraft", &pcraft_bytes()).unwrap();

    menus::invoke(&mut app, &ctx, "file.save", json!({})).unwrap();
    menus::invoke(&mut app, &ctx, "file.saveAs", json!({})).unwrap();
    answer(&mut app, &files, "one.pcraft");
    menus::invoke(&mut app, &ctx, "file.saveACopy", json!({})).unwrap();
    answer(&mut app, &files, "one copy.pcraft");
    // A JPEG asks JPEG Options first; the answered save is still a Save As.
    menus::invoke(&mut app, &ctx, "file.saveAs", json!({})).unwrap();
    answer(&mut app, &files, "one.jpg");
    crate::format_prompts::confirm(&mut app, &ctx);
    assert_eq!(
        *calls.borrow(),
        [
            ("write", "one.pcraft".to_string()),
            ("write_as", "one.pcraft".to_string()),
            ("write_as", "one copy.pcraft".to_string()),
            ("write_as", "one.jpg".to_string()),
        ]
    );

    // A document with no file of its own: Save asks for a name, and is still a Save.
    let (mut app, files, calls) = hosted_app();
    app.run("file.new", json!({"width": 4, "height": 4})).unwrap();
    menus::invoke(&mut app, &ctx, "file.save", json!({})).unwrap();
    answer(&mut app, &files, "Untitled-1.psd");
    assert_eq!(calls.borrow().last().map(|(via, _)| *via), Some("write"));

    app.services.write_as = None;
    menus::invoke(&mut app, &ctx, "file.saveAs", json!({})).unwrap();
    answer(&mut app, &files, "other.psd");
    assert_eq!(calls.borrow().last().cloned(), Some(("write", "other.psd".to_string())));
}

/// A page that keeps only native files (`Services::save_new_native`): a new document's first Save
/// writes `<its name>.pcraft` as a Save, without a dialog, and the document then has that file as
/// its own, so later Saves (and the web shell's autosave) write it back in place. Save As… still
/// asks, offering the same name.
#[test]
fn a_new_document_saves_as_its_native_file_where_the_host_keeps_only_those() {
    let ctx = egui::Context::default();
    let (mut app, files, calls) = hosted_app();
    app.services.save_new_native = true;
    app.run("file.new", json!({"width": 4, "height": 4})).unwrap();
    app.run("file.new", json!({"width": 4, "height": 4, "name": "Banner"})).unwrap();
    menus::invoke(&mut app, &ctx, "file.save", json!({})).unwrap();
    assert!(files.requests().is_empty(), "no dialog: {:?}", files.requests());
    assert_eq!(*calls.borrow(), [("write", "Banner.pcraft".to_string())]);
    let st = app.session.active().unwrap();
    assert_eq!(st.path.as_deref(), Some("Banner.pcraft"));
    assert!(!st.is_dirty());
    assert!(photocraft_engine::file_cmds::holds_at(&st.doc, "Banner.pcraft"), "autosave writes it back in place");

    app.run("image.imageRotation.90cw", json!({})).unwrap();
    menus::invoke(&mut app, &ctx, "file.save", json!({})).unwrap();
    assert_eq!(calls.borrow().last().cloned(), Some(("write", "Banner.pcraft".to_string())));

    assert!(app.session.set_active(0));
    menus::invoke(&mut app, &ctx, "file.saveAs", json!({})).unwrap();
    assert_eq!(files.requests().last().and_then(|r| r.file_name.clone()).as_deref(), Some("Untitled.pcraft"));
}
