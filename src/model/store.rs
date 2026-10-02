use super::document::Project;
use anyhow::{Context, Result};
use std::path::Path;


pub fn save(path: &Path, project: &Project) -> Result<()> {
    let json = serde_json::to_string_pretty(project).context("プロジェクトの直列化に失敗")?;
    std::fs::write(path, json).with_context(|| format!("書き込み失敗: {}", path.display()))?;
    Ok(())
}

pub fn load(path: &Path) -> Result<Project> {
    let text = std::fs::read_to_string(path)
        .with_context(|| format!("読み込み失敗: {}", path.display()))?;
    let mut project: Project =
        serde_json::from_str(&text).context("プロジェクト JSON の解釈に失敗")?;
    project.reindex_ids();
    Ok(project)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Clip, Marker, Track};

    #[test]
    fn save_load_roundtrip() {
        let mut doc = Project::default();
        let mut t = Track::new("Bass");
        t.volume_db = -4.0;
        t.clips.push(Clip::new("C:/x/a.wav".into(), 30, 120));
        t.clips.push(Clip::new("C:/x/b.wav".into(), 200, 60));
        doc.tracks.push(t);
        doc.markers.push(Marker {
            name: "Preset1".into(),
            frame: 90,
        });

        let tmp = std::env::temp_dir().join("audioframe_store_test.json");
        save(&tmp, &doc).unwrap();
        let back = load(&tmp).unwrap();
        let _ = std::fs::remove_file(&tmp);

        assert_eq!(back.tracks.len(), 1);
        assert_eq!(back.tracks[0].name, "Bass");
        assert_eq!(back.tracks[0].clips.len(), 2);
        assert_eq!(back.tracks[0].clips[1].start_frame, 200.0);
        assert_eq!(back.markers[0].name, "Preset1");
    }
}
