use super::id::Id;
use super::track::Track;
use serde::{Deserialize, Serialize};

pub const SCHEMA_VERSION: u32 = 1;
pub const DEFAULT_FPS: u32 = 30;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Marker {
    pub name: String,
    pub frame: i64,
}



#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Project {
    #[serde(default = "default_schema")]
    pub schema_version: u32,
    #[serde(default = "default_fps")]
    pub fps: u32,
    #[serde(default)]
    pub tracks: Vec<Track>,
    #[serde(default)]
    pub markers: Vec<Marker>,
}

fn default_schema() -> u32 {
    SCHEMA_VERSION
}
fn default_fps() -> u32 {
    DEFAULT_FPS
}

impl Default for Project {
    fn default() -> Self {
        Self {
            schema_version: SCHEMA_VERSION,
            fps: DEFAULT_FPS,
            tracks: Vec::new(),
            markers: Vec::new(),
        }
    }
}

impl Project {
    
    pub fn content_end_frame(&self) -> i64 {
        self.tracks
            .iter()
            .flat_map(|t| t.clips.iter())
            .map(|c| c.end_frame.max(c.out_frame))
            .reduce(f64::max)
            .map(|frame| frame.ceil() as i64)
            .unwrap_or(0)
    }

    
    pub fn reindex_ids(&mut self) {
        let mut max = Id(0);
        for t in &mut self.tracks {
            max = max.max(t.id);
            for c in &t.clips {
                max = max.max(c.id);
            }
        }
        Id::bump_past(max);
    }
}
