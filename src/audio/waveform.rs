#[derive(Debug)]
struct PeakLevel {
    block_size: usize,
    values: Vec<f32>,
}

#[derive(Debug)]
pub struct PeakData {
    frames: usize,
    levels: Vec<PeakLevel>,
}

impl PeakData {
    const BASE_BLOCK: usize = 4;
    const LEVEL_RATIO: usize = 4;

    pub fn from_stereo(samples: &[f32]) -> Self {
        let frames = samples.len() / 2;
        if frames == 0 {
            return Self {
                frames,
                levels: Vec::new(),
            };
        }

        let first_count = frames.div_ceil(Self::BASE_BLOCK);
        let mut first_values = Vec::with_capacity(first_count * 2);
        for block in 0..first_count {
            let start = block * Self::BASE_BLOCK;
            let end = (start + Self::BASE_BLOCK).min(frames);
            let (lo, hi) = mono_min_max(samples, start, end);
            first_values.push(lo);
            first_values.push(hi);
        }

        let mut levels = vec![PeakLevel {
            block_size: Self::BASE_BLOCK,
            values: first_values,
        }];
        while levels
            .last()
            .is_some_and(|level| level.block_size < frames)
        {
            let previous = levels.last().expect("peak level exists");
            let block_size = previous.block_size.saturating_mul(Self::LEVEL_RATIO);
            if block_size == previous.block_size {
                break;
            }
            let previous_count = previous.values.len() / 2;
            let count = frames.div_ceil(block_size);
            let mut values = Vec::with_capacity(count * 2);
            for block in 0..count {
                let first = block * Self::LEVEL_RATIO;
                let last = (first + Self::LEVEL_RATIO).min(previous_count);
                let mut lo = 0.0_f32;
                let mut hi = 0.0_f32;
                for previous_block in first..last {
                    lo = lo.min(previous.values[previous_block * 2]);
                    hi = hi.max(previous.values[previous_block * 2 + 1]);
                }
                values.push(lo);
                values.push(hi);
            }
            levels.push(PeakLevel {
                block_size,
                values,
            });
        }

        Self { frames, levels }
    }

    pub fn for_each_column<F>(
        &self,
        samples: &[f32],
        start_frame: usize,
        end_frame: usize,
        columns: usize,
        samples_per_pixel: f64,
        mut visit: F,
    ) where
        F: FnMut(usize, f32, f32),
    {
        if columns == 0 || self.frames == 0 || end_frame <= start_frame {
            return;
        }
        let sample_frames = samples.len() / 2;
        let start = start_frame.min(self.frames).min(sample_frames);
        let end = end_frame.min(self.frames).min(sample_frames);
        if end <= start {
            return;
        }

        let span = end - start;
        for column in 0..columns {
            let mut column_start = start + span.saturating_mul(column) / columns;
            if column_start >= end {
                column_start = end - 1;
            }
            let mut column_end = start + span.saturating_mul(column + 1) / columns;
            if column_end <= column_start {
                column_end = (column_start + 1).min(end);
            }
            let (lo, hi) = self.range_min_max(samples, column_start, column_end, samples_per_pixel);
            visit(column, lo, hi);
        }
    }

    fn range_min_max(
        &self,
        samples: &[f32],
        start_frame: usize,
        end_frame: usize,
        samples_per_pixel: f64,
    ) -> (f32, f32) {
        let sample_frames = samples.len() / 2;
        let start = start_frame.min(self.frames).min(sample_frames);
        let end = end_frame.min(self.frames).min(sample_frames);
        if end <= start {
            return (0.0, 0.0);
        }
        let Some(level) = self.level_for(samples_per_pixel) else {
            return mono_min_max(samples, start, end);
        };
        let block_size = level.block_size;
        let first_full = start.div_ceil(block_size) * block_size;
        let full_end = (end / block_size) * block_size;
        let mut lo = 0.0_f32;
        let mut hi = 0.0_f32;
        let leading_end = first_full.min(end);
        if start < leading_end {
            let (part_lo, part_hi) = mono_min_max(samples, start, leading_end);
            lo = lo.min(part_lo);
            hi = hi.max(part_hi);
        }

        let mut block_start = first_full;
        while block_start < full_end {
            let block = block_start / block_size;
            lo = lo.min(level.values[block * 2]);
            hi = hi.max(level.values[block * 2 + 1]);
            block_start += block_size;
        }

        if full_end < end {
            let (part_lo, part_hi) = mono_min_max(samples, full_end.max(start), end);
            lo = lo.min(part_lo);
            hi = hi.max(part_hi);
        }
        (lo, hi)
    }

    fn level_for(&self, samples_per_pixel: f64) -> Option<&PeakLevel> {
        if samples_per_pixel < Self::BASE_BLOCK as f64 {
            return None;
        }
        self.levels
            .iter()
            .take_while(|level| level.block_size as f64 <= samples_per_pixel)
            .last()
            .or_else(|| self.levels.first())
    }
}

fn mono_min_max(samples: &[f32], start_frame: usize, end_frame: usize) -> (f32, f32) {
    let mut lo = 0.0_f32;
    let mut hi = 0.0_f32;
    for frame in start_frame..end_frame {
        let mono = 0.5 * (samples[frame * 2] + samples[frame * 2 + 1]);
        lo = lo.min(mono);
        hi = hi.max(mono);
    }
    (lo, hi)
}

#[cfg(test)]
mod tests {
    use super::PeakData;

    fn stereo(values: &[f32]) -> Vec<f32> {
        values.iter().flat_map(|value| [*value, *value]).collect()
    }

    #[test]
    fn builds_multiple_peak_resolutions() {
        let data = PeakData::from_stereo(&stereo(&[0.0; 70]));
        let sizes: Vec<usize> = data.levels.iter().map(|level| level.block_size).collect();
        assert_eq!(sizes, vec![4, 16, 64, 256]);
    }

    #[test]
    fn visible_columns_use_only_the_requested_range() {
        let samples = stereo(&[
            0.0, 0.0, 0.0, 0.0, 0.8, 0.8, 0.0, 0.0, -0.6, -0.6, 0.0, 0.0,
        ]);
        let data = PeakData::from_stereo(&samples);
        let mut columns = Vec::new();
        data.for_each_column(&samples, 4, 8, 2, 1.0, |index, lo, hi| {
            columns.push((index, lo, hi))
        });
        assert_eq!(columns.len(), 2);
        assert_eq!(columns[0], (0, 0.0, 0.8));
        assert_eq!(columns[1], (1, 0.0, 0.0));
    }

    #[test]
    fn high_zoom_reads_pcm_without_peak_rounding() {
        let samples = stereo(&[-0.75, 0.25, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0]);
        let data = PeakData::from_stereo(&samples);
        let mut result = None;
        data.for_each_column(&samples, 0, 2, 1, 0.5, |_, lo, hi| result = Some((lo, hi)));
        assert_eq!(result, Some((-0.75, 0.25)));
    }
}
