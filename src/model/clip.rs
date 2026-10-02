use super::id::Id;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;







#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Clip {
    #[serde(default)]
    pub id: Id,
    pub source: PathBuf,
    pub start_frame: f64,
    pub in_frame: f64,
    pub out_frame: f64,
    pub end_frame: f64,
    #[serde(default)]
    pub gain_db: f32,
    #[serde(default)]
    pub muted: bool,
    #[serde(default)]
    pub fade_in_frames: i64,
    #[serde(default)]
    pub fade_out_frames: i64,
}

impl Clip {
    pub fn new(source: PathBuf, start_frame: i64, length_frames: i64) -> Self {
        let len = length_frames.max(1) as f64;
        let start_frame = start_frame as f64;
        Self {
            id: Id::new(),
            source,
            start_frame,
            in_frame: start_frame,
            out_frame: start_frame + len,
            end_frame: start_frame + len,
            gain_db: 0.0,
            muted: false,
            fade_in_frames: 0,
            fade_out_frames: 0,
        }
    }
}
