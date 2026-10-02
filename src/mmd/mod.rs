










use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicI64, AtomicU32, Ordering};
use std::sync::{Arc, Mutex};


#[derive(Clone, Default)]
pub struct MmdState {
    pub attached: bool,
    pub frame: f64,
    pub playing: bool,
    pub looping: Option<bool>,
    pub wav_path: Option<PathBuf>,
}


#[derive(Clone)]
pub struct MmdProcess {
    pub pid: u32,
    pub label: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PlaybackRange {
    pub start: i64,
    pub end: i64,
}

impl PlaybackRange {
    pub fn is_valid(self) -> bool {
        (0..=i32::MAX as i64).contains(&self.start)
            && (self.end == -1 || (self.start..=i32::MAX as i64).contains(&self.end))
    }
}

#[derive(Default)]
struct Shared {
    
    target_pid: AtomicU32,
    state: Mutex<MmdState>,
    pending_frame: AtomicI64, 
    pending_toggle: AtomicBool,
    
    pending_stop: AtomicBool,
    
    pending_normalize: AtomicBool,
    
    pending_frame_stop: Mutex<Option<bool>>,
    
    pending_wav_output: Mutex<Option<bool>>,
    pending_import: Mutex<Option<PathBuf>>,
    pending_range_read: Mutex<Option<u32>>,
    pending_range_write: Mutex<Option<(u32, PlaybackRange)>>,
    range_result: Mutex<Option<(u32, PlaybackRange)>>,
    pending_loop: Mutex<Option<(u32, bool)>>,
}

pub struct MmdSession {
    shared: Arc<Shared>,
    stop: Arc<AtomicBool>,
    handle: Option<std::thread::JoinHandle<()>>,
}

impl MmdSession {
    pub fn start() -> Self {
        let shared = Arc::new(Shared::default());
        shared.pending_frame.store(-1, Ordering::Relaxed);
        
        let stop = Arc::new(AtomicBool::new(false));
        let sh = shared.clone();
        let st = stop.clone();
        let handle = std::thread::Builder::new()
            .name("mmd-poll".into())
            .spawn(move || poll_loop(sh, st))
            .ok();
        Self {
            shared,
            stop,
            handle,
        }
    }

    
    pub fn list_processes(&self) -> Vec<MmdProcess> {
        list_mmd_processes()
    }

    
    pub fn attach(&self, pid: u32) {
        self.shared.pending_range_read.lock().unwrap().take();
        self.shared.pending_range_write.lock().unwrap().take();
        self.shared.range_result.lock().unwrap().take();
        self.shared.pending_loop.lock().unwrap().take();
        self.shared.target_pid.store(pid, Ordering::Relaxed);
    }
    pub fn detach(&self) {
        self.attach(0);
    }
    pub fn target_pid(&self) -> u32 {
        self.shared.target_pid.load(Ordering::Relaxed)
    }

    pub fn state(&self) -> MmdState {
        self.shared.state.lock().unwrap().clone()
    }

    

    
    pub fn set_current_frame(&self, frame: i64) {
        self.shared
            .pending_frame
            .store(frame.max(0), Ordering::Relaxed);
    }
    
    pub fn toggle_playback(&self) {
        self.shared.pending_toggle.store(true, Ordering::Relaxed);
    }
    
    pub fn stop_playback(&self) {
        self.shared.pending_stop.store(true, Ordering::Relaxed);
    }
    
    pub fn normalize_playback(&self) {
        self.shared.pending_normalize.store(true, Ordering::Relaxed);
    }
    
    pub fn set_frame_stop(&self, on: bool) {
        *self.shared.pending_frame_stop.lock().unwrap() = Some(on);
    }
    
    pub fn set_wav_output(&self, on: bool) {
        *self.shared.pending_wav_output.lock().unwrap() = Some(on);
    }

    pub fn import_wav(&self, path: PathBuf) {
        *self.shared.pending_import.lock().unwrap() = Some(path);
    }

    pub fn request_playback_range(&self) {
        self.shared.range_result.lock().unwrap().take();
        let pid = self.target_pid();
        if pid != 0 {
            *self.shared.pending_range_read.lock().unwrap() = Some(pid);
        }
    }

    pub fn take_playback_range(&self) -> Option<PlaybackRange> {
        let (pid, range) = self.shared.range_result.lock().unwrap().take()?;
        (pid == self.target_pid()).then_some(range)
    }

    pub fn set_playback_range(&self, range: PlaybackRange) {
        let pid = self.target_pid();
        if pid != 0 && range.is_valid() {
            *self.shared.pending_range_write.lock().unwrap() = Some((pid, range));
        }
    }

    pub fn set_looping(&self, on: bool) {
        let pid = self.target_pid();
        if pid != 0 {
            *self.shared.pending_loop.lock().unwrap() = Some((pid, on));
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

impl Drop for MmdSession {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(h) = self.handle.take() {
            let _ = h.join();
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
fn list_mmd_processes() -> Vec<MmdProcess> {
    win::list_processes()
}

#[cfg(not(windows))]
fn poll_loop(shared: Arc<Shared>, stop: Arc<AtomicBool>) {
    while !stop.load(Ordering::Relaxed) {
        shared.state.lock().unwrap().attached = false;
        std::thread::sleep(std::time::Duration::from_millis(200));
    }
}

#[cfg(not(windows))]
fn list_mmd_processes() -> Vec<MmdProcess> {
    Vec::new()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loop_commands_are_explicit_and_do_not_cross_connections() {
        let session = MmdSession::disconnected();
        session.set_looping(true);
        assert!(session.shared.pending_loop.lock().unwrap().is_none());
        session.attach(42);
        session.set_looping(true);
        assert_eq!(*session.shared.pending_loop.lock().unwrap(), Some((42, true)));
        session.set_looping(false);
        assert_eq!(*session.shared.pending_loop.lock().unwrap(), Some((42, false)));
        session.attach(43);
        assert!(session.shared.pending_loop.lock().unwrap().is_none());
        session.set_looping(true);
        assert_eq!(*session.shared.pending_loop.lock().unwrap(), Some((43, true)));
        session.detach();
        assert!(session.shared.pending_loop.lock().unwrap().is_none());
    }

    #[test]
    fn playback_range_accepts_unbounded_end_and_rejects_invalid_bounds() {
        for range in [
            PlaybackRange { start: 0, end: 0 },
            PlaybackRange { start: 30, end: 900 },
            PlaybackRange { start: 30, end: -1 },
            PlaybackRange { start: i32::MAX as i64, end: i32::MAX as i64 },
        ] {
            assert!(range.is_valid());
        }
        for range in [
            PlaybackRange { start: -1, end: 30 },
            PlaybackRange { start: 30, end: 29 },
            PlaybackRange { start: 0, end: -2 },
            PlaybackRange { start: 0, end: i64::MAX },
            PlaybackRange { start: i64::MAX, end: -1 },
        ] {
            assert!(!range.is_valid());
        }
    }

    #[test]
    fn range_commands_and_late_replies_do_not_cross_connections() {
        let session = MmdSession {
            shared: Arc::new(Shared::default()),
            stop: Arc::new(AtomicBool::new(false)),
            handle: None,
        };
        let range = PlaybackRange { start: 30, end: 900 };
        session.attach(42);
        session.request_playback_range();
        session.set_playback_range(range);
        *session.shared.range_result.lock().unwrap() = Some((42, range));

        session.attach(43);
        assert!(session.shared.pending_range_read.lock().unwrap().is_none());
        assert!(session.shared.pending_range_write.lock().unwrap().is_none());
        assert_eq!(session.take_playback_range(), None);

        *session.shared.range_result.lock().unwrap() = Some((42, range));
        assert_eq!(session.take_playback_range(), None);
        *session.shared.range_result.lock().unwrap() = Some((43, range));
        assert_eq!(session.take_playback_range(), Some(range));
        assert_eq!(session.take_playback_range(), None);

        session.detach();
        session.request_playback_range();
        session.set_playback_range(range);
        assert!(session.shared.pending_range_read.lock().unwrap().is_none());
        assert!(session.shared.pending_range_write.lock().unwrap().is_none());
    }
}
