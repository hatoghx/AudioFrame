


use super::decode::TARGET_SR;
use super::sources::SourceLibrary;
use crate::model::Project;
use std::sync::Arc;

pub const SR: u32 = TARGET_SR;


pub struct VoiceSnap {
    pub samples: Arc<[f32]>,
    pub src_frames: usize,
    pub start_frame: f64,
    pub in_frame: f64,
    pub out_frame: f64,
    pub fade_in_frames: i64,
    pub fade_out_frames: i64,
    
    pub gain: f32,
    pub track_gain: f32,
    pub track_volume_db: f32,
    
    pub pan: f32,
    
    pub track_index: usize,
}

pub struct MixSnapshot {
    pub voices: Vec<VoiceSnap>,
    pub main_gain: f32,
    pub fps: f64,
}

impl MixSnapshot {
    pub fn empty() -> Self {
        Self {
            voices: Vec::new(),
            main_gain: 1.0,
            fps: 30.0,
        }
    }

    
    pub fn build(doc: &Project, lib: &SourceLibrary, main_gain_db: f32) -> Self {
        let any_solo = doc.tracks.iter().any(|t| t.soloed);
        let mut voices = Vec::new();
        for (ti, track) in doc.tracks.iter().enumerate() {
            let audible = if any_solo {
                track.soloed && !track.muted
            } else {
                !track.muted
            };
            let track_gain = if audible { 1.0 } else { 0.0 };
            for clip in &track.clips {
                if clip.muted {
                    continue;
                }
                let Some(src) = lib.get(&clip.source) else {
                    continue;
                };
                voices.push(VoiceSnap {
                    samples: src.samples.clone(),
                    src_frames: src.frames,
                    start_frame: clip.start_frame,
                    in_frame: clip.in_frame,
                    out_frame: clip.out_frame,
                    fade_in_frames: clip.fade_in_frames.max(0),
                    fade_out_frames: clip.fade_out_frames.max(0),
                    gain: db_to_lin(clip.gain_db),
                    track_gain,
                    track_volume_db: track.volume_db,
                    pan: track.pan.clamp(-1.0, 1.0),
                    track_index: ti,
                });
            }
        }
        Self {
            voices,
            main_gain: db_to_lin(main_gain_db),
            fps: doc.fps.max(1) as f64,
        }
    }
}

#[inline]
pub fn db_to_lin(db: f32) -> f32 {
    if db <= -60.0 {
        0.0
    } else {
        10f32.powf(db / 20.0)
    }
}
