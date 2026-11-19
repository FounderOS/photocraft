//! Host mode: a page that frames PhotoCraft (`?host=parent`) or runs it in its own document
//! (`data-host="page"` on the canvas) opens documents in it and stores what it saves instead of
//! the browser downloading. The messages and both transports are `craft-web-host`'s, the same
//! contract as the other Craft apps' bridges; this module is PhotoCraft's part of it.
//!
//! - `photocraft:ready` once the app is running and listening. PhotoCraft reads whole files, so
//!   it says `file: false` and a host sends `bytes`.
//! - `photocraft:open` `{ name, bytes }` opens as File › Open would
//!   ([`photocraft_ui_egui::PhotocraftApp::open_hosted_bytes`]: a layered document keeps the name
//!   as its file, so Save hands the same file back).
//! - `photocraft:save` `{ name, bytes, saveAs }` for every file the app would download: Save and
//!   exports with `saveAs: false` (an export under its own extension), Save As… and Save a Copy…
//!   with `true`. A document with no file of its own (a new one, or a flat image) saves as
//!   `<its name>.pcraft` (`Services::save_new_native`): the host keeps only those. From then on it
//!   is that file, which Save and autosave write back in place.
//! - `photocraft:dirty` whenever a document's unsaved changes appear or go away, and
//!   `photocraft:failed` when the app couldn't start.

use std::cell::RefCell;
use std::rc::Rc;

use craft_web_host::web::{FileData, Host, Message};

/// The app id in the host messages (`photocraft:ready`).
const APP: &str = "photocraft";

/// Files the host sent, waiting for the next frame: (name, bytes).
type Opened = Rc<RefCell<Vec<(String, Vec<u8>)>>>;

#[derive(Clone)]
pub struct Bridge {
    host: Rc<Host>,
    opened: Opened,
}

/// The bridge for this page, or `None` when PhotoCraft isn't hosted (no `?host=parent` or
/// `data-host="page"`, no framing parent, or a parent the page doesn't allow): then saves stay
/// browser downloads.
pub fn connect(canvas: &web_sys::Element) -> Option<Bridge> {
    Some(Bridge { host: Rc::new(Host::connect(APP, canvas)?), opened: Opened::default() })
}

impl Bridge {
    /// Hear the host's files. `ctx` wakes the app: a message is no input event, so egui would not
    /// run a frame (and open the file) before the next one.
    pub fn listen(&self, ctx: egui::Context) {
        let opened = self.opened.clone();
        self.host.listen(move |message| {
            if let Message::Open { name, data } = message {
                deliver_open(&opened, &ctx, name, &data);
            }
        });
    }

    /// The files the host sent since the last call, in the order they came.
    pub fn take_opened(&self) -> Vec<(String, Vec<u8>)> {
        std::mem::take(&mut *self.opened.borrow_mut())
    }

    pub fn ready(&self) {
        let _ = self.host.post_ready(env!("CARGO_PKG_VERSION"), false);
    }

    /// The app couldn't start (no WebGPU or WebGL2, for example): let the host say so.
    pub fn failed(&self, error: &str) {
        let _ = self.host.post_failed(error, None);
    }

    /// Hand the host a file the app would otherwise download. The host names it by the last
    /// component of `name`, so an export keeps its own extension.
    pub fn save(&self, name: &str, bytes: &[u8], save_as: bool) -> Result<(), String> {
        self.host.post_save(name, bytes, None, save_as)
    }

    /// Report unsaved changes; the crate sends only changes of state, so this can run every frame.
    pub fn dirty(&self, dirty: bool) {
        self.host.post_dirty(dirty);
    }
}

fn deliver_open(opened: &Opened, ctx: &egui::Context, name: String, data: &FileData) {
    let FileData::Bytes(array) = data else {
        log::warn!("host file {name} came as a File; PhotoCraft reads bytes only; not opened");
        return;
    };
    opened.borrow_mut().push((name, array.to_vec()));
    ctx.request_repaint();
}
