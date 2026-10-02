





use anyhow::{Context, Result};
use std::path::Path;

pub const TARGET_SR: u32 = 44_100;
pub const TARGET_CH: usize = 2;


pub struct Decoded {
    pub samples: Vec<f32>,
    pub frames: usize,
}

pub fn decode_file(path: &Path) -> Result<Decoded> {
    let ext = path
        .extension()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    match ext.as_str() {
        "wav" | "wave" => decode_wav(path),
        #[cfg(windows)]
        _ => mf::decode(path),
        #[cfg(not(windows))]
        other => anyhow::bail!("未対応の形式です: .{other}"),
    }
}

/// Length only, without decoding samples, so a dropped file lands on the
/// timeline at its true length before the waveform exists.
pub fn probe_seconds(path: &Path) -> Result<f64> {
    let ext = path
        .extension()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    match ext.as_str() {
        "wav" | "wave" => {
            let reader = hound::WavReader::open(path)
                .with_context(|| format!("WAV を開けません: {}", path.display()))?;
            let spec = reader.spec();
            Ok(reader.duration() as f64 / spec.sample_rate.max(1) as f64)
        }
        #[cfg(windows)]
        _ => mf::probe_seconds(path),
        #[cfg(not(windows))]
        other => anyhow::bail!("未対応の形式です: .{other}"),
    }
}

fn decode_wav(path: &Path) -> Result<Decoded> {
    let mut reader = hound::WavReader::open(path)
        .with_context(|| format!("WAV を開けません: {}", path.display()))?;
    let spec = reader.spec();
    let src_ch = spec.channels.max(1) as usize;
    let src_sr = spec.sample_rate.max(1);

    
    let raw: Vec<f32> = match spec.sample_format {
        hound::SampleFormat::Float => reader
            .samples::<f32>()
            .collect::<Result<_, _>>()
            .context("WAV(float) 読み取り失敗")?,
        hound::SampleFormat::Int => {
            let max = (1i64 << (spec.bits_per_sample - 1)) as f32;
            reader
                .samples::<i32>()
                .map(|s| s.map(|v| v as f32 / max))
                .collect::<Result<_, _>>()
                .context("WAV(int) 読み取り失敗")?
        }
    };

    let stereo = to_stereo(&raw, src_ch);
    let out = if src_sr == TARGET_SR {
        stereo
    } else {
        resample_linear_stereo(&stereo, src_sr, TARGET_SR)
    };

    let frames = out.len() / TARGET_CH;
    Ok(Decoded {
        samples: out,
        frames,
    })
}


fn to_stereo(inter: &[f32], ch: usize) -> Vec<f32> {
    if ch == 2 {
        return inter.to_vec();
    }
    let frames = inter.len() / ch.max(1);
    let mut out = Vec::with_capacity(frames * 2);
    for f in 0..frames {
        if ch == 1 {
            let v = inter[f];
            out.push(v);
            out.push(v);
        } else {
            
            out.push(inter[f * ch]);
            out.push(inter[f * ch + 1]);
        }
    }
    out
}


fn resample_linear_stereo(inter: &[f32], from_sr: u32, to_sr: u32) -> Vec<f32> {
    let in_frames = inter.len() / 2;
    if in_frames == 0 {
        return Vec::new();
    }
    let ratio = to_sr as f64 / from_sr as f64;
    let out_frames = ((in_frames as f64) * ratio).round() as usize;
    let mut out = Vec::with_capacity(out_frames * 2);
    for of in 0..out_frames {
        let src = of as f64 / ratio;
        let i0 = src.floor() as usize;
        let frac = (src - i0 as f64) as f32;
        let i1 = (i0 + 1).min(in_frames - 1);
        for c in 0..2 {
            let a = inter[i0 * 2 + c];
            let b = inter[i1 * 2 + c];
            out.push(a + (b - a) * frac);
        }
    }
    out
}


#[cfg(windows)]
mod mf {
    use super::{resample_linear_stereo, to_stereo, Decoded, TARGET_CH, TARGET_SR};
    use anyhow::{bail, Context, Result};
    use std::os::windows::ffi::OsStrExt;
    use std::path::Path;
    use std::sync::Once;
    use windows::core::PCWSTR;
    use windows::Win32::Media::MediaFoundation::*;
    use windows::Win32::System::Com::{CoInitializeEx, COINIT_MULTITHREADED};

    const FIRST_AUDIO_STREAM: u32 = 0xFFFF_FFFD;
    const ALL_STREAMS: u32 = 0xFFFF_FFFE;
    const ENDOFSTREAM: u32 = 0x0000_0002;

    static INIT: Once = Once::new();
    static mut INIT_OK: bool = false;

    fn ensure_started() -> bool {
        INIT.call_once(|| unsafe {
            let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
            
            INIT_OK = MFStartup(MF_VERSION, MFSTARTUP_LITE).is_ok();
        });
        unsafe { INIT_OK }
    }

    pub fn probe_seconds(path: &Path) -> Result<f64> {
        if !ensure_started() {
            bail!("Media Foundation を初期化できませんでした");
        }
        unsafe {
            let wide: Vec<u16> = path
                .as_os_str()
                .encode_wide()
                .chain(std::iter::once(0))
                .collect();
            let reader: IMFSourceReader =
                MFCreateSourceReaderFromURL(PCWSTR(wide.as_ptr()), None)
                    .with_context(|| format!("メディアを開けません: {}", path.display()))?;
            let value = reader
                .GetPresentationAttribute(MF_SOURCE_READER_MEDIASOURCE.0 as u32, &MF_PD_DURATION)
                .context("再生時間を取得できません")?;
            // MF_PD_DURATION is in 100-nanosecond units.
            let hundred_ns = u64::try_from(&value).unwrap_or(0);
            Ok(hundred_ns as f64 / 10_000_000.0)
        }
    }

    pub fn decode(path: &Path) -> Result<Decoded> {
        if !ensure_started() {
            bail!("Media Foundation を初期化できませんでした");
        }
        unsafe { decode_inner(path) }
    }

    unsafe fn decode_inner(path: &Path) -> Result<Decoded> {
        let wide: Vec<u16> = path
            .as_os_str()
            .encode_wide()
            .chain(std::iter::once(0))
            .collect();
        let reader: IMFSourceReader = MFCreateSourceReaderFromURL(PCWSTR(wide.as_ptr()), None)
            .with_context(|| format!("メディアを開けません: {}", path.display()))?;

        
        reader.SetStreamSelection(ALL_STREAMS, false)?;
        reader.SetStreamSelection(FIRST_AUDIO_STREAM, true)?;

        
        let want: IMFMediaType = MFCreateMediaType()?;
        want.SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Audio)?;
        want.SetGUID(&MF_MT_SUBTYPE, &MFAudioFormat_Float)?;
        want.SetUINT32(&MF_MT_AUDIO_SAMPLES_PER_SECOND, TARGET_SR)?;
        want.SetUINT32(&MF_MT_AUDIO_NUM_CHANNELS, TARGET_CH as u32)?;
        want.SetUINT32(&MF_MT_AUDIO_BITS_PER_SAMPLE, 32)?;
        reader
            .SetCurrentMediaType(FIRST_AUDIO_STREAM, None, &want)
            .context("この形式の音声を PCM へ変換できません")?;

        let actual = reader.GetCurrentMediaType(FIRST_AUDIO_STREAM)?;
        let ch = actual
            .GetUINT32(&MF_MT_AUDIO_NUM_CHANNELS)
            .unwrap_or(TARGET_CH as u32)
            .max(1) as usize;
        let sr = actual
            .GetUINT32(&MF_MT_AUDIO_SAMPLES_PER_SECOND)
            .unwrap_or(TARGET_SR)
            .max(1);

        let mut samples: Vec<f32> = Vec::new();
        loop {
            let mut flags = 0u32;
            let mut sample: Option<IMFSample> = None;
            reader.ReadSample(
                FIRST_AUDIO_STREAM,
                0,
                None,
                Some(&mut flags),
                None,
                Some(&mut sample),
            )?;
            if flags & ENDOFSTREAM != 0 {
                break;
            }
            let Some(sample) = sample else {
                continue;
            };
            let buf = sample.ConvertToContiguousBuffer()?;
            let mut ptr: *mut u8 = std::ptr::null_mut();
            let mut cur = 0u32;
            buf.Lock(&mut ptr, None, Some(&mut cur))?;
            if !ptr.is_null() && cur >= 4 {
                let bytes = std::slice::from_raw_parts(ptr, cur as usize);
                let (chunks, _) = bytes.as_chunks::<4>();
                for c in chunks {
                    samples.push(f32::from_le_bytes(*c));
                }
            }
            buf.Unlock()?;
        }

        if samples.is_empty() {
            bail!("音声データを取得できませんでした（音声トラックなし？）");
        }

        let stereo = to_stereo(&samples, ch);
        let out = if sr == TARGET_SR {
            stereo
        } else {
            resample_linear_stereo(&stereo, sr, TARGET_SR)
        };
        let frames = out.len() / TARGET_CH;
        Ok(Decoded {
            samples: out,
            frames,
        })
    }
}
