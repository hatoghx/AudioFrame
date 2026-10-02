


use super::decode::{decode_file, TARGET_SR};
use super::waveform::PeakData;
use anyhow::Result;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, Sender};
use std::sync::Arc;

pub struct AudioSource {
    pub sample_rate: u32,
    pub frames: usize,
    
    pub samples: Arc<[f32]>,
    pub peaks: Arc<PeakData>,
}

impl AudioSource {
    pub fn seconds(&self) -> f64 {
        self.frames as f64 / self.sample_rate as f64
    }
}

#[derive(Default)]
pub struct SourceLibrary {
    map: HashMap<PathBuf, Arc<AudioSource>>,
    loading: HashSet<PathBuf>,
    inbox: Option<(Sender<Loaded>, Receiver<Loaded>)>,
}

type Loaded = (PathBuf, Result<Arc<AudioSource>, String>);

impl SourceLibrary {
    pub fn get(&self, path: &Path) -> Option<Arc<AudioSource>> {
        self.map.get(path).cloned()
    }

    /// Decode on a worker thread so dropping a file never blocks the UI.
    pub fn request_load(&mut self, path: &Path) {
        if self.map.contains_key(path) || self.loading.contains(path) {
            return;
        }
        let (tx, _) = self
            .inbox
            .get_or_insert_with(|| std::sync::mpsc::channel::<Loaded>());
        let tx = tx.clone();
        let owned = path.to_path_buf();
        self.loading.insert(owned.clone());
        std::thread::spawn(move || {
            let result = load_source(&owned).map_err(|e| format!("{e:#}"));
            let _ = tx.send((owned, result));
        });
    }

    pub fn has_pending(&self) -> bool {
        !self.loading.is_empty()
    }

    /// Install everything that finished decoding; true when the library changed.
    pub fn poll(&mut self) -> bool {
        let Some((_, rx)) = self.inbox.as_ref() else {
            return false;
        };
        let mut changed = false;
        while let Ok((path, result)) = rx.try_recv() {
            self.loading.remove(&path);
            match result {
                Ok(src) => {
                    self.map.insert(path, src);
                    changed = true;
                }
                Err(e) => log::error!("音源を読み込めません {}: {e}", path.display()),
            }
        }
        changed
    }

    
    pub fn get_or_load(&mut self, path: &Path) -> Result<Arc<AudioSource>> {
        if let Some(s) = self.map.get(path) {
            return Ok(s.clone());
        }
        let src = load_source(path)?;
        self.loading.remove(path);
        self.map.insert(path.to_path_buf(), src.clone());
        Ok(src)
    }

    pub fn contains(&self, path: &Path) -> bool {
        self.map.contains_key(path)
    }
}

fn load_source(path: &Path) -> Result<Arc<AudioSource>> {
    let d = decode_file(path)?;
    let peaks = PeakData::from_stereo(&d.samples);
    Ok(Arc::new(AudioSource {
        sample_rate: TARGET_SR,
        frames: d.frames,
        samples: Arc::from(d.samples.into_boxed_slice()),
        peaks: Arc::new(peaks),
    }))
}
