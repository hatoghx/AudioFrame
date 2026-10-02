







use super::snapshot::MixSnapshot;
use arc_swap::{ArcSwap, ArcSwapOption};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::Arc;

const MAX_TRACKS: usize = 256;
const PEAK_SLOTS: usize = MAX_TRACKS + 1; 
const PEAK_DECAY_PER_BLOCK: f32 = 0.82;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LoopRange {
    start: f64,
    end: f64,
}

impl LoopRange {
    pub fn new(start: f64, end: f64) -> Option<Self> {
        (start.is_finite() && end.is_finite() && start >= 0.0 && end > start)
            .then_some(Self { start, end })
    }

    fn wrap(self, position: f64) -> f64 {
        if position < self.start {
            self.start
        } else if position >= self.end {
            self.start + (position - self.start).rem_euclid(self.end - self.start)
        } else {
            position
        }
    }
}

struct Shared {
    snap: ArcSwap<MixSnapshot>,
    loop_range: ArcSwapOption<LoopRange>,
    playing: AtomicBool,
    
    pos_bits: AtomicU64,
    
    peaks: Vec<AtomicU32>,
    main_left_peak: AtomicU32,
    main_right_peak: AtomicU32,
    device_sr: f64,
    device_ch: usize,
}

impl Shared {
    fn pos(&self) -> f64 {
        f64::from_bits(self.pos_bits.load(Ordering::Relaxed))
    }
    fn set_pos(&self, s: f64) {
        self.pos_bits.store(s.to_bits(), Ordering::Relaxed);
    }
}

pub struct PreviewEngine {
    shared: Arc<Shared>,
    _stream: cpal::Stream,
}

impl PreviewEngine {
    pub fn new() -> anyhow::Result<Self> {
        let host = cpal::default_host();
        let device = host
            .default_output_device()
            .ok_or_else(|| anyhow::anyhow!("既定の出力デバイスが見つかりません"))?;
        let cfg = device.default_output_config()?;
        let sample_format = cfg.sample_format();
        let stream_cfg: cpal::StreamConfig = cfg.into();
        let device_sr = stream_cfg.sample_rate.0 as f64;
        let device_ch = stream_cfg.channels as usize;

        let shared = Arc::new(Shared {
            snap: ArcSwap::from_pointee(MixSnapshot::empty()),
            loop_range: ArcSwapOption::empty(),
            playing: AtomicBool::new(false),
            pos_bits: AtomicU64::new(0.0f64.to_bits()),
            peaks: (0..PEAK_SLOTS).map(|_| AtomicU32::new(0)).collect(),
            main_left_peak: AtomicU32::new(0),
            main_right_peak: AtomicU32::new(0),
            device_sr,
            device_ch,
        });

        let s = shared.clone();
        let err_fn = |e| log::error!("オーディオストリームエラー: {e}");

        let stream = match sample_format {
            cpal::SampleFormat::F32 => device.build_output_stream(
                &stream_cfg,
                move |data: &mut [f32], _| render(&s, data),
                err_fn,
                None,
            )?,
            other => anyhow::bail!("未対応のサンプル形式: {other:?}"),
        };
        stream.play()?;

        Ok(Self {
            shared,
            _stream: stream,
        })
    }

    pub fn publish(&self, snap: Arc<MixSnapshot>) {
        self.shared.snap.store(snap);
    }

    pub fn set_loop_range(&self, range: Option<LoopRange>) {
        if self.shared.loop_range.load().as_deref().copied() != range {
            self.shared.loop_range.store(range.map(Arc::new));
        }
    }

    pub fn set_playing(&self, playing: bool) {
        self.shared.playing.store(playing, Ordering::Relaxed);
    }
    pub fn is_playing(&self) -> bool {
        self.shared.playing.load(Ordering::Relaxed)
    }
    pub fn toggle(&self) {
        let p = self.is_playing();
        self.set_playing(!p);
    }

    pub fn seek_seconds(&self, s: f64) {
        self.shared.set_pos(s.max(0.0));
    }
    pub fn position_seconds(&self) -> f64 {
        self.shared.pos()
    }

    
    
    
    pub fn sync(&self, mmd_seconds: f64, advancing: bool) {
        let target = mmd_seconds.max(0.0);
        self.set_playing(advancing);
        
        
        let snap = if advancing { 0.100 } else { 0.0 };
        if !advancing || (self.shared.pos() - target).abs() > snap {
            self.shared.set_pos(target);
        }
    }

    
    pub fn track_peak(&self, i: usize) -> f32 {
        if i >= MAX_TRACKS {
            return 0.0;
        }
        f32::from_bits(self.shared.peaks[i].load(Ordering::Relaxed))
    }
    #[allow(dead_code)]
    pub fn main_peak(&self) -> f32 {
        f32::from_bits(self.shared.peaks[MAX_TRACKS].load(Ordering::Relaxed))
    }
    pub fn main_peak_stereo(&self) -> (f32, f32) {
        (
            f32::from_bits(self.shared.main_left_peak.load(Ordering::Relaxed)),
            f32::from_bits(self.shared.main_right_peak.load(Ordering::Relaxed)),
        )
    }
}


fn render(s: &Shared, data: &mut [f32]) {
    data.fill(0.0);
    let ch = s.device_ch.max(1);
    let out_frames = data.len() / ch;
    if out_frames == 0 {
        return;
    }

    let playing = s.playing.load(Ordering::Relaxed);

    
    
    if !playing {
        for p in &s.peaks {
            let prev = f32::from_bits(p.load(Ordering::Relaxed));
            p.store((prev * PEAK_DECAY_PER_BLOCK).to_bits(), Ordering::Relaxed);
        }
        for p in [&s.main_left_peak, &s.main_right_peak] {
            let prev = f32::from_bits(p.load(Ordering::Relaxed));
            p.store((prev * PEAK_DECAY_PER_BLOCK).to_bits(), Ordering::Relaxed);
        }
        return;
    }

    let snap = s.snap.load();
    let loop_range = s.loop_range.load();
    let position = |time| loop_range.as_ref().map_or(time, |range| range.wrap(time));
    let pos0 = position(s.pos());
    let inv_dev_sr = 1.0 / s.device_sr;
    let src_sr = super::snapshot::SR as f64;
    let fps = snap.fps;

    
    let mut block_peak = [0.0f32; MAX_TRACKS];
    let mut main_block_peak = 0.0f32;
    let mut main_left_block_peak = 0.0f32;
    let mut main_right_block_peak = 0.0f32;

    for v in &snap.voices {
        if v.track_gain <= 0.0 || v.gain <= 0.0 || v.samples.is_empty() {
            continue;
        }
        let in_sec = v.in_frame as f64 / fps;
        let out_sec = v.out_frame as f64 / fps;
        let start_sec = v.start_frame as f64 / fps;
        let src_len_sec = v.src_frames as f64 / src_sr;
        if src_len_sec <= 0.0 || out_sec <= in_sec {
            continue;
        }

        let (lpan, rpan) = pan_gains(v.pan);
        let fade_in_sec = v.fade_in_frames as f64 / fps;
        let fade_out_sec = v.fade_out_frames as f64 / fps;

        for k in 0..out_frames {
            let t = position(pos0 + k as f64 * inv_dev_sr);
            if t < in_sec || t >= out_sec {
                continue;
            }
            
            let src_t = t - start_sec;
            if src_t < 0.0 || src_t >= src_len_sec {
                continue;
            }
            
            let sp = src_t * src_sr;
            let i0 = sp.floor() as usize;
            let frac = (sp - i0 as f64) as f32;
            let i1 = (i0 + 1).min(v.src_frames - 1);
            let l = lerp(v.samples[i0 * 2], v.samples[i1 * 2], frac);
            let r = lerp(v.samples[i0 * 2 + 1], v.samples[i1 * 2 + 1], frac);

            let g = v.gain
                * v.track_gain
                * super::snapshot::db_to_lin(v.track_volume_db)
                * snap.main_gain;

            
            let mut env = 1.0f32;
            if fade_in_sec > 0.0 {
                let d = (t - in_sec) / fade_in_sec;
                if d < 1.0 {
                    env *= d.clamp(0.0, 1.0) as f32;
                }
            }
            if fade_out_sec > 0.0 {
                let d = (out_sec - t) / fade_out_sec;
                if d < 1.0 {
                    env *= d.clamp(0.0, 1.0) as f32;
                }
            }

            let sl = l * g * env * lpan;
            let sr = r * g * env * rpan;

            let base = k * ch;
            if ch >= 2 {
                data[base] += sl;
                data[base + 1] += sr;
            } else {
                data[base] += 0.5 * (sl + sr);
            }

            let a = sl.abs().max(sr.abs());
            if v.track_index < MAX_TRACKS {
                block_peak[v.track_index] = block_peak[v.track_index].max(a);
            }
        }
    }

    
    for (index, x) in data.iter_mut().enumerate() {
        *x = x.clamp(-1.0, 1.0);
        main_block_peak = main_block_peak.max(x.abs());
        if index % ch == 0 {
            main_left_block_peak = main_left_block_peak.max(x.abs());
        } else if ch >= 2 && index % ch == 1 {
            main_right_block_peak = main_right_block_peak.max(x.abs());
        }
    }
    if ch < 2 {
        main_right_block_peak = main_left_block_peak;
    }

    
    for (i, bp) in block_peak.iter().enumerate() {
        let prev = f32::from_bits(s.peaks[i].load(Ordering::Relaxed));
        let next = bp.max(prev * PEAK_DECAY_PER_BLOCK);
        s.peaks[i].store(next.to_bits(), Ordering::Relaxed);
    }
    let prev_main = f32::from_bits(s.peaks[MAX_TRACKS].load(Ordering::Relaxed));
    let next_main = main_block_peak.max(prev_main * PEAK_DECAY_PER_BLOCK);
    s.peaks[MAX_TRACKS].store(next_main.to_bits(), Ordering::Relaxed);
    let prev_main_left = f32::from_bits(s.main_left_peak.load(Ordering::Relaxed));
    let next_main_left = main_left_block_peak.max(prev_main_left * PEAK_DECAY_PER_BLOCK);
    s.main_left_peak.store(next_main_left.to_bits(), Ordering::Relaxed);
    let prev_main_right = f32::from_bits(s.main_right_peak.load(Ordering::Relaxed));
    let next_main_right = main_right_block_peak.max(prev_main_right * PEAK_DECAY_PER_BLOCK);
    s.main_right_peak.store(next_main_right.to_bits(), Ordering::Relaxed);

    
    s.set_pos(position(pos0 + out_frames as f64 * inv_dev_sr));
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
    use crate::audio::snapshot::{VoiceSnap, SR};

    fn shared(range: Option<LoopRange>) -> Shared {
        Shared {
            snap: ArcSwap::from_pointee(MixSnapshot {
                voices: vec![VoiceSnap {
                    samples: vec![0.5; SR as usize * 2].into(),
                    src_frames: SR as usize,
                    start_frame: 0.0,
                    in_frame: 1.0,
                    out_frame: 3.0,
                    fade_in_frames: 0,
                    fade_out_frames: 0,
                    gain: 1.0,
                    track_gain: 1.0,
                    track_volume_db: 0.0,
                    pan: -1.0,
                    track_index: 0,
                }],
                main_gain: 1.0,
                fps: 4.0,
            }),
            loop_range: ArcSwapOption::from(range.map(Arc::new)),
            playing: AtomicBool::new(true),
            pos_bits: AtomicU64::new(0.5_f64.to_bits()),
            peaks: (0..PEAK_SLOTS).map(|_| AtomicU32::new(0)).collect(),
            main_left_peak: AtomicU32::new(0),
            main_right_peak: AtomicU32::new(0),
            device_sr: 4.0,
            device_ch: 2,
        }
    }

    #[test]
    fn loop_range_rejects_empty_reversed_or_nonfinite_bounds() {
        for (start, end) in [
            (0.0, 0.0),
            (2.0, 1.0),
            (-1.0, 1.0),
            (0.0, f64::INFINITY),
            (f64::NAN, 1.0),
        ] {
            assert_eq!(LoopRange::new(start, end), None);
        }
    }

    #[test]
    fn audio_block_wraps_repeatedly_without_silent_gaps() {
        let s = shared(LoopRange::new(0.25, 0.75));
        let mut data = [0.0; 16];
        render(&s, &mut data);
        for stereo in data.chunks_exact(2) {
            assert!((stereo[0] - 0.5).abs() < 0.00001);
            assert_eq!(stereo[1], 0.0);
        }
        assert_eq!(
            f32::from_bits(s.main_left_peak.load(Ordering::Relaxed)),
            0.5
        );
        assert_eq!(
            f32::from_bits(s.main_right_peak.load(Ordering::Relaxed)),
            0.0
        );
        assert_eq!(s.pos(), 0.5);
    }

    #[test]
    fn disabling_loop_preserves_normal_forward_playback() {
        let s = shared(LoopRange::new(0.25, 0.75));
        s.loop_range.store(None);
        let mut data = [0.0; 8];
        render(&s, &mut data);
        assert_eq!(data[0], 0.5);
        assert!(data[2..].iter().all(|sample| *sample == 0.0));
        assert_eq!(s.pos(), 1.5);
    }

    #[test]
    fn stopped_playback_remains_silent_and_does_not_wrap() {
        let s = shared(LoopRange::new(0.25, 0.75));
        s.playing.store(false, Ordering::Relaxed);
        s.set_pos(2.0);
        let mut data = [1.0; 8];
        render(&s, &mut data);
        assert_eq!(data, [0.0; 8]);
        assert_eq!(s.pos(), 2.0);
    }

    #[test]
    fn playback_outside_loop_returns_to_its_range() {
        let s = shared(LoopRange::new(0.25, 0.75));
        for position in [0.0, 0.75, 4.25] {
            s.set_pos(position);
            let mut data = [0.0; 2];
            render(&s, &mut data);
            assert_eq!(data[0], 0.5);
            assert_eq!(s.pos(), 0.5);
        }
    }
}
