//! Ctrl+O opens a native file dialog for choosing a .vrm model. The dialog blocks, so it runs on its
//! own short-lived thread; the chosen path comes back over a channel and is emitted as [`OpenVrm`].
//! The render loop only does one key check and one `try_recv` per frame.

use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver, TryRecvError};
use std::sync::Mutex;

use bevy::prelude::*;

/// Emitted when the user picks a .vrm file in the open dialog. The receiver still runs
/// `validate_vrm1` before loading it.
#[derive(Message, Clone)]
pub struct OpenVrm(pub PathBuf);

/// Adds Ctrl+O, the open dialog, and the [`OpenVrm`] message.
pub struct OpenDialogPlugin;

impl Plugin for OpenDialogPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<OpenDialog>()
            .add_message::<OpenVrm>()
            .add_systems(Update, (start_on_chord, poll_dialog).chain());
    }
}

/// True when Ctrl is held and O was pressed this frame.
pub fn is_open_chord(ctrl: bool, key_o_just_pressed: bool) -> bool {
    ctrl && key_o_just_pressed
}

/// Tracks whether a dialog thread is running. At most one dialog is open at a time.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct DialogState {
    open: bool,
}

impl DialogState {
    /// Marks a dialog as open. Returns false when one is already open, so the caller must not start another.
    pub fn try_start(&mut self) -> bool {
        if self.open {
            return false;
        }
        self.open = true;
        true
    }

    /// Marks the dialog as closed, whether it picked a file or was cancelled.
    pub fn finish(&mut self) {
        self.open = false;
    }

    pub fn is_open(&self) -> bool {
        self.open
    }
}

/// Dialog state plus the receiving end of the running dialog thread's channel.
#[derive(Resource, Default)]
struct OpenDialog {
    state: DialogState,
    /// Wrapped in a Mutex because `Receiver` is not `Sync` and a Bevy resource must be.
    rx: Option<Mutex<Receiver<PathBuf>>>,
}

impl OpenDialog {
    fn close(&mut self) {
        self.state.finish();
        self.rx = None;
    }
}

/// What one poll of the dialog channel found.
#[derive(Debug, PartialEq)]
enum Pick {
    /// The dialog is still open.
    Waiting,
    /// The user chose this file.
    Chosen(PathBuf),
    /// The dialog closed without a choice: the thread dropped its sender with nothing sent.
    Cancelled,
}

/// Reads one result from the dialog channel without blocking.
fn check(rx: &Receiver<PathBuf>) -> Pick {
    match rx.try_recv() {
        Ok(path) => Pick::Chosen(path),
        Err(TryRecvError::Empty) => Pick::Waiting,
        Err(TryRecvError::Disconnected) => Pick::Cancelled,
    }
}

/// Starts the dialog on Ctrl+O unless one is already open.
fn start_on_chord(keys: Res<ButtonInput<KeyCode>>, mut dialog: ResMut<OpenDialog>) {
    let ctrl = keys.pressed(KeyCode::ControlLeft) || keys.pressed(KeyCode::ControlRight);
    if !is_open_chord(ctrl, keys.just_pressed(KeyCode::KeyO)) {
        return;
    }
    if !dialog.state.try_start() {
        return;
    }
    let (tx, rx) = mpsc::channel();
    dialog.rx = Some(Mutex::new(rx));
    let spawned = std::thread::Builder::new().name("open-vrm-dialog".into()).spawn(move || {
        if let Some(path) = rfd::FileDialog::new().add_filter("VRM model", &["vrm"]).pick_file() {
            let _ = tx.send(path);
        }
    });
    if spawned.is_err() {
        dialog.close();
    }
}

/// Emits [`OpenVrm`] when the dialog thread has sent a file, and frees the dialog slot when it closes.
fn poll_dialog(mut dialog: ResMut<OpenDialog>, mut opened: MessageWriter<OpenVrm>) {
    let pick = match dialog.rx.as_ref() {
        None => return,
        Some(rx) => check(&rx.lock().unwrap_or_else(|e| e.into_inner())),
    };
    match pick {
        Pick::Waiting => {}
        Pick::Chosen(path) => {
            dialog.close();
            opened.write(OpenVrm(path));
        }
        Pick::Cancelled => dialog.close(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chord_needs_ctrl_and_o_together() {
        assert!(is_open_chord(true, true));
        assert!(!is_open_chord(false, true), "O alone does nothing");
        assert!(!is_open_chord(true, false), "Ctrl alone does nothing");
        assert!(!is_open_chord(false, false));
    }

    #[test]
    fn only_one_dialog_at_a_time() {
        let mut s = DialogState::default();
        assert!(!s.is_open());
        assert!(s.try_start());
        assert!(s.is_open());
        assert!(!s.try_start(), "second Ctrl+O while open is ignored");
        s.finish();
        assert!(!s.is_open());
        assert!(s.try_start(), "a new dialog may open after the first closes");
    }

    #[test]
    fn poll_reports_waiting_then_the_chosen_path() {
        let (tx, rx) = mpsc::channel();
        assert_eq!(check(&rx), Pick::Waiting);
        tx.send(PathBuf::from("a.vrm")).unwrap();
        drop(tx);
        assert_eq!(check(&rx), Pick::Chosen(PathBuf::from("a.vrm")), "buffered pick comes before disconnect");
        assert_eq!(check(&rx), Pick::Cancelled);
    }

    #[test]
    fn cancelled_dialog_drops_sender_without_a_pick() {
        let (tx, rx) = mpsc::channel::<PathBuf>();
        drop(tx);
        assert_eq!(check(&rx), Pick::Cancelled);
    }
}
