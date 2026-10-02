use super::clip::Clip;
use super::id::Id;
use serde::{Deserialize, Serialize};


#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Track {
    #[serde(default)]
    pub id: Id,
    pub name: String,
    #[serde(default)]
    pub color_tag: Option<String>,
    #[serde(default)]
    pub muted: bool,
    #[serde(default)]
    pub soloed: bool,
    #[serde(default)]
    pub volume_db: f32,
    
    #[serde(default)]
    pub pan: f32,
    #[serde(default = "default_height")]
    pub height: f32,
    #[serde(default)]
    pub clips: Vec<Clip>,
}

fn default_height() -> f32 {
    72.0
}

impl Track {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            id: Id::new(),
            name: name.into(),
            color_tag: None,
            muted: false,
            soloed: false,
            volume_db: 0.0,
            pan: 0.0,
            height: default_height(),
            clips: Vec::new(),
        }
    }

    pub fn clip(&self, id: Id) -> Option<&Clip> {
        self.clips.iter().find(|c| c.id == id)
    }
}
