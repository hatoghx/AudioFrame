
use crate::audio::snapshot::{db_to_lin, SR};
use crate::audio::SourceLibrary;
use crate::model::Project;
use anyhow::{Context, Result};
use std::io::Write;
use std::path::Path;




pub fn render_mix(doc: &Project, lib: &SourceLibrary, main_db: f32) -> Vec<f32> {
    let fps = doc.fps.max(1) as f64;
    let sr = SR as f64;
    let end_frame = doc.content_end_frame().max(1);
    let total = ((end_frame as f64 / fps) * sr).ceil() as usize + SR as usize / 5;
    let mut buf = vec![0.0f32; total * 2];

    let any_solo = doc.tracks.iter().any(|t| t.soloed);
    for track in &doc.tracks {
        let audible = if any_solo {
            track.soloed && !track.muted
        } else {
            !track.muted
        };
        if !audible {
            continue;
        }
        let (lp, rp) = pan_gains(track.pan);
        for clip in &track.clips {
            if clip.muted {
                continue;
            }
            let Some(src) = lib.get(&clip.source) else {
                continue;
            };
            if src.frames == 0 {
                continue;
            }
            let in_s = clip.in_frame as f64 / fps;
            let out_s = clip.out_frame as f64 / fps;
            let start_s = clip.start_frame as f64 / fps;
            let src_len_s = src.frames as f64 / sr;
            let fi = clip.fade_in_frames as f64 / fps;
            let fo = clip.fade_out_frames as f64 / fps;

            let n0 = (in_s * sr).max(0.0) as usize;
            let n1 = ((out_s * sr).ceil() as usize).min(total);
            for n in n0..n1 {
                let t = n as f64 / sr;
                let st = t - start_s;
                if st < 0.0 || st >= src_len_s {
                    continue;
                }
                let sp = st * sr;
                let i0 = sp.floor() as usize;
                let fr = (sp - i0 as f64) as f32;
                let i1 = (i0 + 1).min(src.frames - 1);
                let l = lerp(src.samples[i0 * 2], src.samples[i1 * 2], fr);
                let r = lerp(src.samples[i0 * 2 + 1], src.samples[i1 * 2 + 1], fr);

                let tg = db_to_lin(track.volume_db);
                let g = db_to_lin(clip.gain_db) * tg * db_to_lin(main_db);

                let mut env = 1.0f32;
                if fi > 0.0 {
                    let d = (t - in_s) / fi;
                    if d < 1.0 {
                        env *= d.clamp(0.0, 1.0) as f32;
                    }
                }
                if fo > 0.0 {
                    let d = (out_s - t) / fo;
                    if d < 1.0 {
                        env *= d.clamp(0.0, 1.0) as f32;
                    }
                }
                buf[n * 2] += l * g * env * lp;
                buf[n * 2 + 1] += r * g * env * rp;
            }
        }
    }
    for x in &mut buf {
        *x = x.clamp(-1.0, 1.0);
    }
    buf
}


pub fn export_mix_wav(path: &Path, doc: &Project, lib: &SourceLibrary, main_db: f32) -> Result<()> {
    let mix = render_mix(doc, lib, main_db);
    let fps = doc.fps.max(1) as f64;

    
    let mut cues: Vec<(u32, String)> = Vec::new();
    for t in &doc.tracks {
        for c in &t.clips {
            let off = ((c.in_frame as f64 / fps) * SR as f64).max(0.0) as u32;
            let name = c
                .source
                .file_stem()
                .and_then(|s| s.to_str())
                .map(str::to_owned)
                .unwrap_or_else(|| crate::core::loc::t("arr.clipDefaultName"));
            cues.push((off, name));
        }
    }
    for m in &doc.markers {
        let off = ((m.frame as f64 / fps) * SR as f64).max(0.0) as u32;
        cues.push((off, m.name.clone()));
    }
    cues.sort_by_key(|(o, _)| *o);

    write_wav_cue(path, &mix, SR, &cues)
        .with_context(|| format!("WAV 書き出し失敗: {}", path.display()))
}


fn write_wav_cue(path: &Path, samples: &[f32], sr: u32, cues: &[(u32, String)]) -> Result<()> {
    let ch: u16 = 2;
    let bits: u16 = 16;
    let block_align = ch * bits / 8;
    let byte_rate = sr * block_align as u32;

    
    let mut data = Vec::with_capacity(samples.len() * 2);
    for &s in samples {
        let v = (s.clamp(-1.0, 1.0) * i16::MAX as f32).round() as i16;
        data.extend_from_slice(&v.to_le_bytes());
    }
    let data_pad = data.len() % 2;

    
    let mut cue = Vec::new();
    cue.extend_from_slice(&(cues.len() as u32).to_le_bytes());
    for (i, (off, _)) in cues.iter().enumerate() {
        cue.extend_from_slice(&(i as u32 + 1).to_le_bytes()); 
        cue.extend_from_slice(&off.to_le_bytes()); 
        cue.extend_from_slice(b"data"); 
        cue.extend_from_slice(&0u32.to_le_bytes()); 
        cue.extend_from_slice(&0u32.to_le_bytes()); 
        cue.extend_from_slice(&off.to_le_bytes()); 
    }

    
    let mut adtl = Vec::new();
    adtl.extend_from_slice(b"adtl");
    for (i, (_, label)) in cues.iter().enumerate() {
        let mut txt = label.as_bytes().to_vec();
        txt.push(0);
        if txt.len() % 2 == 1 {
            txt.push(0);
        }
        let labl_len = 4 + txt.len();
        adtl.extend_from_slice(b"labl");
        adtl.extend_from_slice(&(labl_len as u32).to_le_bytes());
        adtl.extend_from_slice(&(i as u32 + 1).to_le_bytes());
        adtl.extend_from_slice(&txt);
    }

    let has_cue = !cues.is_empty();
    let cue_total = if has_cue { 8 + cue.len() } else { 0 };
    let list_total = if has_cue { 8 + adtl.len() } else { 0 };
    let riff_size = 4 + (8 + 16) + (8 + data.len() + data_pad) + cue_total + list_total;

    let mut f = std::io::BufWriter::new(std::fs::File::create(path)?);
    f.write_all(b"RIFF")?;
    f.write_all(&(riff_size as u32).to_le_bytes())?;
    f.write_all(b"WAVE")?;

    f.write_all(b"fmt ")?;
    f.write_all(&16u32.to_le_bytes())?;
    f.write_all(&1u16.to_le_bytes())?; 
    f.write_all(&ch.to_le_bytes())?;
    f.write_all(&sr.to_le_bytes())?;
    f.write_all(&byte_rate.to_le_bytes())?;
    f.write_all(&block_align.to_le_bytes())?;
    f.write_all(&bits.to_le_bytes())?;

    f.write_all(b"data")?;
    f.write_all(&(data.len() as u32).to_le_bytes())?;
    f.write_all(&data)?;
    if data_pad == 1 {
        f.write_all(&[0u8])?;
    }

    if has_cue {
        f.write_all(b"cue ")?;
        f.write_all(&(cue.len() as u32).to_le_bytes())?;
        f.write_all(&cue)?;
        f.write_all(b"LIST")?;
        f.write_all(&(adtl.len() as u32).to_le_bytes())?;
        f.write_all(&adtl)?;
    }

    f.flush()?;
    Ok(())
}

#[inline]
fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

#[inline]
fn pan_gains(pan: f32) -> (f32, f32) {
    let p = (pan.clamp(-1.0, 1.0) + 1.0) * 0.5 * std::f32::consts::FRAC_PI_2;
    (p.cos(), p.sin())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Clip, Track};

    fn tmp(name: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!("audioframe_test_{name}"))
    }

    fn make_source_wav(path: &Path, secs: f32) {
        let spec = hound::WavSpec {
            channels: 2,
            sample_rate: SR,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        };
        let mut w = hound::WavWriter::create(path, spec).unwrap();
        let n = (secs * SR as f32) as usize;
        for i in 0..n {
            let v = ((i as f32 * 0.05).sin() * 8000.0) as i16;
            w.write_sample(v).unwrap();
            w.write_sample(v).unwrap();
        }
        w.finalize().unwrap();
    }

    #[test]
    fn mix_wav_roundtrips_through_hound() {
        let src = tmp("src.wav");
        make_source_wav(&src, 0.5);
        let mut lib = SourceLibrary::default();
        lib.get_or_load(&src).unwrap();

        let mut doc = Project::default();
        let mut t = Track::new("T1");
        t.clips.push(Clip::new(src.clone(), 0, 15)); 
        doc.tracks.push(t);
        doc.markers.push(crate::model::Marker {
            name: "m1".into(),
            frame: 5,
        });

        let out = tmp("mix.wav");
        export_mix_wav(&out, &doc, &lib, 0.0).unwrap();

        let r = hound::WavReader::open(&out).unwrap();
        assert_eq!(r.spec().sample_rate, SR);
        assert_eq!(r.spec().channels, 2);
        assert_eq!(r.spec().bits_per_sample, 16);
        assert!(r.len() > 0);

        let _ = std::fs::remove_file(&src);
        let _ = std::fs::remove_file(&out);
    }

}
