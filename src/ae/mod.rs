use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, Mutex};

#[derive(Clone, Default)]
pub struct AeState {
    pub attached: bool,
    pub busy: bool,
    pub last_error: Option<String>,
}

#[derive(Clone)]
pub struct AeProcess {
    pub pid: u32,
    pub label: String,
}

#[derive(Clone, Debug)]
pub struct AeLayerSnapshot {
    pub key: String,
    pub name: String,
    pub source: PathBuf,
    pub start_frame: i64,
    pub in_frame: i64,
    pub out_frame: i64,
    pub gain_db: f32,
    pub muted: bool,
    pub fade_in_frames: i64,
    pub fade_out_frames: i64,
}

#[derive(Clone, Debug)]
pub struct AeProjectSnapshot {
    pub fps: f64,
    pub duration_frames: i64,
    pub layers: Vec<AeLayerSnapshot>,
}

#[derive(Default)]
pub(crate) struct Shared {
    target_pid: AtomicU32,
    state: Mutex<AeState>,
    pending_project_read: Mutex<Option<u32>>,
    pending_project_apply: Mutex<Option<(u32, AeProjectSnapshot)>>,
    project_result: Mutex<Option<(u32, Result<AeProjectSnapshot, String>)>>,
    pending_import: Mutex<Option<(u32, PathBuf)>>,
}

pub struct AeSession {
    shared: Arc<Shared>,
    stop: Arc<AtomicBool>,
    handle: Option<std::thread::JoinHandle<()>>,
}

impl AeSession {
    pub fn start() -> Self {
        let shared = Arc::new(Shared::default());
        let stop = Arc::new(AtomicBool::new(false));
        let sh = shared.clone();
        let st = stop.clone();
        let handle = std::thread::Builder::new()
            .name("ae-worker".into())
            .spawn(move || poll_loop(sh, st))
            .ok();
        Self {
            shared,
            stop,
            handle,
        }
    }

    pub fn list_processes(&self) -> Vec<AeProcess> {
        list_ae_processes()
    }

    pub fn attach(&self, pid: u32) {
        self.shared.pending_project_read.lock().unwrap().take();
        self.shared.pending_project_apply.lock().unwrap().take();
        self.shared.project_result.lock().unwrap().take();
        self.shared.pending_import.lock().unwrap().take();
        self.shared.target_pid.store(pid, Ordering::Relaxed);
    }

    pub fn detach(&self) {
        self.attach(0);
    }

    pub fn target_pid(&self) -> u32 {
        self.shared.target_pid.load(Ordering::Relaxed)
    }

    pub fn state(&self) -> AeState {
        self.shared.state.lock().unwrap().clone()
    }

    pub fn request_project(&self) {
        let pid = self.target_pid();
        if pid != 0 {
            self.shared.project_result.lock().unwrap().take();
            *self.shared.pending_project_read.lock().unwrap() = Some(pid);
        }
    }

    pub fn take_project(&self) -> Option<Result<AeProjectSnapshot, String>> {
        let (pid, result) = self.shared.project_result.lock().unwrap().take()?;
        (pid == self.target_pid()).then_some(result)
    }

    pub fn apply_project(&self, snapshot: AeProjectSnapshot) {
        let pid = self.target_pid();
        if pid != 0 {
            *self.shared.pending_project_apply.lock().unwrap() = Some((pid, snapshot));
        }
    }

    pub fn import_wav(&self, path: PathBuf) {
        let pid = self.target_pid();
        if pid != 0 {
            *self.shared.pending_import.lock().unwrap() = Some((pid, path));
        }
    }

    #[cfg(test)]
    pub(crate) fn disconnected() -> Self {
        Self {
            shared: Arc::new(Shared::default()),
            stop: Arc::new(AtomicBool::new(false)),
            handle: None,
        }
    }
}

impl Drop for AeSession {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
}

#[cfg(windows)]
#[path = "win.rs"]
mod win;

#[cfg(windows)]
fn poll_loop(shared: Arc<Shared>, stop: Arc<AtomicBool>) {
    win::poll_loop(shared, stop);
}

#[cfg(windows)]
fn list_ae_processes() -> Vec<AeProcess> {
    win::list_processes()
}

/// The same script the live connection runs, as a standalone .jsx file.
#[cfg(windows)]
pub fn layer_script(snapshot: &AeProjectSnapshot) -> String {
    win::layer_script(snapshot)
}

#[cfg(not(windows))]
pub fn layer_script(_snapshot: &AeProjectSnapshot) -> String {
    String::new()
}

#[cfg(not(windows))]
fn poll_loop(shared: Arc<Shared>, stop: Arc<AtomicBool>) {
    while !stop.load(Ordering::Relaxed) {
        shared.state.lock().unwrap().attached = false;
        std::thread::sleep(std::time::Duration::from_millis(200));
    }
}

#[cfg(not(windows))]
fn list_ae_processes() -> Vec<AeProcess> {
    Vec::new()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn commands_are_discarded_when_connection_changes() {
        let session = AeSession::disconnected();
        session.attach(42);
        session.request_project();
        session.apply_project(AeProjectSnapshot {
            fps: 30.0,
            duration_frames: 900,
            layers: Vec::new(),
        });
        session.attach(43);
        assert!(session.shared.pending_project_read.lock().unwrap().is_none());
        assert!(session.shared.pending_project_apply.lock().unwrap().is_none());
        assert!(session.take_project().is_none());
    }

    #[test]
    fn project_result_is_scoped_to_the_current_connection() {
        let session = AeSession::disconnected();
        let snapshot = AeProjectSnapshot {
            fps: 30.0,
            duration_frames: 900,
            layers: Vec::new(),
        };
        session.attach(42);
        *session.shared.project_result.lock().unwrap() = Some((42, Ok(snapshot.clone())));
        session.attach(43);
        assert!(session.take_project().is_none());
        *session.shared.project_result.lock().unwrap() = Some((43, Ok(snapshot)));
        assert!(session.take_project().is_some());
    }
}
