use std::path::PathBuf;

#[cfg(feature = "standalone")]
use crate::audio_codec::decode_audio_to_f32_interleaved_sync;
use crate::edits::{
    AudioEditAction, AudioEditKind, AudioEdits, apply_audio_edit_actions, default_fade_samples,
    summarize_audio_edit_actions,
};
use crate::history::DocumentSnapshot;
use crate::message::AudioBuffer;

#[derive(Debug, Clone)]
pub struct AudioDocument {
    pub(crate) source_path: PathBuf,
    pub(crate) save_path: Option<PathBuf>,
    pub(crate) samples: Vec<f32>,
    pub(crate) preview_samples: Vec<f32>,
    pub(crate) channels: usize,
    pub(crate) sample_rate: u32,
    pub(crate) channel_samples: Vec<Vec<f32>>,
    pub(crate) peak: f32,
    pub(crate) clip_region: Option<crate::edits::AudioRegion>,
    pub(crate) edits: AudioEdits,
    pub(crate) edit_actions: Vec<AudioEditAction>,
    pub(crate) markers: Vec<(usize, String)>,
}

impl AudioDocument {
    pub(crate) fn from_interleaved(
        audio: AudioBuffer,
        region: Option<crate::edits::AudioRegion>,
    ) -> Result<Self, String> {
        let channels = audio.channels.max(1);
        let sample_rate = audio.sample_rate.max(1);
        let source_path = PathBuf::from(if audio.name.is_empty() {
            String::from("embedded audio")
        } else {
            audio.name
        });
        let samples = clip_samples(audio.samples.as_ref(), channels, region);
        let edits = AudioEdits::default();
        let edit_actions = Vec::new();
        let preview = render_preview_samples(&samples, channels, &edit_actions);
        let channel_samples = deinterleave(&preview, channels);
        let peak = peak(&preview);

        Ok(Self {
            source_path,
            save_path: None,
            samples,
            preview_samples: preview,
            channels,
            sample_rate,
            channel_samples,
            peak,
            clip_region: region,
            edits,
            edit_actions,
            markers: Vec::new(),
        })
    }

    #[cfg(feature = "standalone")]
    pub(crate) fn open_with_progress<F>(
        path: PathBuf,
        region: Option<crate::edits::AudioRegion>,
        mut progress_callback: F,
    ) -> Result<Self, String>
    where
        F: FnMut(f32, &str),
    {
        let name = path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("audio file")
            .to_string();

        progress_callback(0.0, &format!("Opening {name}..."));
        progress_callback(0.05, &format!("Decoding {name}..."));
        let (samples, channels, sample_rate) = decode_audio_to_f32_interleaved_sync(&path)
            .map_err(|err| format!("Failed to open '{}': {err}", path.display()))?;
        progress_callback(0.72, &format!("Preparing clip from {name}..."));
        let samples = clip_samples(&samples, channels, region);
        let edits = AudioEdits::default();
        let edit_actions = Vec::new();
        progress_callback(0.78, &format!("Applying preview edits to {name}..."));
        let preview = render_preview_samples(&samples, channels, &edit_actions);
        progress_callback(0.85, &format!("Preparing waveform for {name}..."));
        let channel_samples = deinterleave(&preview, channels);
        progress_callback(0.95, &format!("Measuring peak level for {name}..."));
        let peak = peak(&preview);
        let save_path = region.is_none().then(|| path.clone());
        progress_callback(1.0, &format!("Opened {name}."));

        Ok(Self {
            source_path: path,
            save_path,
            samples,
            preview_samples: preview,
            channels,
            sample_rate,
            channel_samples,
            peak,
            clip_region: region,
            edits,
            edit_actions,
            markers: Vec::new(),
        })
    }

    pub(crate) fn frames(&self) -> usize {
        self.samples.len() / self.channels.max(1)
    }

    pub(crate) fn rebuild_preview(&mut self) {
        self.edits = summarize_audio_edit_actions(self.frames(), &self.edit_actions);
        let preview = render_preview_samples(&self.samples, self.channels, &self.edit_actions);
        self.preview_samples = preview.clone();
        self.channel_samples = deinterleave(&preview, self.channels);
        self.peak = peak(&preview);
    }

    pub(crate) fn region_action(&self, kind: AudioEditKind) -> AudioEditAction {
        match kind {
            AudioEditKind::FadeIn => {
                let length = default_fade_samples(self.frames());
                AudioEditAction::FadeIn {
                    start_sample: 0,
                    length_samples: length,
                }
            }
            AudioEditKind::FadeOut => {
                let length = default_fade_samples(self.frames());
                AudioEditAction::FadeOut {
                    start_sample: self.frames().saturating_sub(length),
                    length_samples: length,
                }
            }
            AudioEditKind::GainDb { delta_db } => AudioEditAction::GainDb {
                start_sample: 0,
                length_samples: self.frames(),
                delta_db,
            },
        }
    }

    pub(crate) fn edit_summary(&self) -> AudioEdits {
        summarize_audio_edit_actions(self.frames(), &self.edit_actions)
    }

    #[cfg(feature = "standalone")]
    pub(crate) fn rendered_save_samples(&self) -> Vec<f32> {
        render_preview_samples(&self.samples, self.channels, &self.edit_actions)
    }

    pub(crate) fn next_zero_crossing_frame(&self, start_frame: usize) -> Option<usize> {
        let channels = self.channels.max(1);
        let frames = self.preview_samples.len() / channels;
        if start_frame.saturating_add(1) >= frames {
            return None;
        }

        let mut iter = self
            .preview_samples
            .chunks_exact(channels)
            .enumerate()
            .skip(start_frame);
        let (_, previous_chunk) = iter.next()?;
        let mut previous = previous_chunk.iter().sum::<f32>() / channels as f32;

        for (frame, chunk) in iter {
            let current = chunk.iter().sum::<f32>() / channels as f32;
            if (previous > 0.0 && current <= 0.0) || (previous < 0.0 && current >= 0.0) {
                return Some(frame);
            }
            previous = current;
        }
        None
    }
}

pub(crate) fn delete_sample_range(audio: &mut AudioDocument, start: usize, length: usize) {
    let end = start.saturating_add(length).min(audio.frames());
    if start >= end {
        return;
    }
    let channels = audio.channels.max(1);
    let sample_start = start * channels;
    let sample_end = end * channels;
    audio.samples.drain(sample_start..sample_end);

    if let Some(region) = audio.clip_region.as_mut() {
        let region_start = region.offset;
        let region_end = region.offset + region.length;
        if end <= region_start {
            region.offset = region.offset.saturating_sub(end - start);
        } else if start < region_end {
            let delete_start = start.max(region_start);
            let delete_end = end.min(region_end);
            let deleted_in_region = delete_end.saturating_sub(delete_start);
            region.length = region.length.saturating_sub(deleted_in_region);
            if start < region_start {
                region.offset = region_start.saturating_sub(end - start);
            }
            if region.length == 0 {
                audio.clip_region = None;
            }
        }
    }

    let deleted_frames = end - start;
    audio
        .markers
        .retain(|(sample, _)| *sample < start || *sample >= end);
    for (sample, _) in &mut audio.markers {
        if *sample >= end {
            *sample = sample.saturating_sub(deleted_frames);
        }
    }
}

pub(crate) fn restore_document(audio: &mut AudioDocument, snapshot: DocumentSnapshot) {
    audio.samples = snapshot.samples;
    audio.edits = snapshot.edits;
    audio.edit_actions = snapshot.edit_actions;
    audio.markers = snapshot.markers;
}

pub(crate) fn render_preview_samples(
    samples: &[f32],
    channels: usize,
    actions: &[AudioEditAction],
) -> Vec<f32> {
    apply_audio_edit_actions(samples, channels, actions)
}

pub(crate) fn clip_samples(
    samples: &[f32],
    channels: usize,
    region: Option<crate::edits::AudioRegion>,
) -> Vec<f32> {
    let channels = channels.max(1);
    let Some(region) = region else {
        return samples.to_vec();
    };
    let frames = samples.len() / channels;
    let start = region.offset.min(frames);
    let end = start.saturating_add(region.length).min(frames);
    samples[start * channels..end * channels].to_vec()
}

pub(crate) fn deinterleave(samples: &[f32], channels: usize) -> Vec<Vec<f32>> {
    let channels = channels.max(1);
    let frames = samples.len() / channels;
    let mut output = vec![Vec::with_capacity(frames); channels];
    for frame in samples.chunks_exact(channels) {
        for (channel, sample) in frame.iter().copied().enumerate() {
            output[channel].push(sample);
        }
    }
    output
}

pub(crate) fn peak(samples: &[f32]) -> f32 {
    samples
        .iter()
        .fold(0.0f32, |peak, sample| peak.max(sample.abs()))
}

#[cfg(test)]
pub(crate) fn test_document(preview: Vec<f32>, channels: usize) -> AudioDocument {
    let channel_samples = deinterleave(&preview, channels);
    AudioDocument {
        source_path: PathBuf::new(),
        save_path: None,
        samples: preview.clone(),
        preview_samples: preview,
        channels,
        sample_rate: 48_000,
        channel_samples,
        peak: 1.0,
        clip_region: None,
        edits: AudioEdits::default(),
        edit_actions: Vec::new(),
        markers: Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deinterleave_splits_channels() {
        assert_eq!(
            deinterleave(&[1.0, 2.0, 3.0, 4.0], 2),
            vec![vec![1.0, 3.0], vec![2.0, 4.0]]
        );
    }

    #[test]
    fn clip_samples_extracts_frame_range() {
        let samples = [1.0, 10.0, 2.0, 20.0, 3.0, 30.0, 4.0, 40.0];
        assert_eq!(
            clip_samples(
                &samples,
                2,
                Some(crate::edits::AudioRegion {
                    offset: 1,
                    length: 2
                })
            ),
            vec![2.0, 20.0, 3.0, 30.0]
        );
    }

    #[test]
    fn next_zero_crossing_finds_positive_to_negative_crossing() {
        let audio = test_document(vec![1.0f32, -1.0], 1);
        assert_eq!(audio.next_zero_crossing_frame(0), Some(1));
    }

    #[test]
    fn next_zero_crossing_finds_negative_to_positive_crossing() {
        let audio = test_document(vec![-1.0f32, -0.5, 0.5, 1.0], 1);
        assert_eq!(audio.next_zero_crossing_frame(0), Some(2));
    }

    #[test]
    fn next_zero_crossing_detects_exact_zero_sample() {
        let audio = test_document(vec![1.0f32, 0.0, -1.0], 1);
        assert_eq!(audio.next_zero_crossing_frame(0), Some(1));
    }

    #[test]
    fn next_zero_crossing_returns_none_when_no_crossing() {
        let audio = test_document(vec![0.1f32, 0.2, 0.3], 1);
        assert_eq!(audio.next_zero_crossing_frame(0), None);
    }

    #[test]
    fn next_zero_crossing_respects_start_frame() {
        let audio = test_document(vec![1.0f32, -1.0, 1.0, -1.0], 1);
        assert_eq!(audio.next_zero_crossing_frame(1), Some(2));
    }

    #[test]
    fn next_zero_crossing_returns_none_past_end() {
        let audio = test_document(vec![1.0f32, -1.0], 1);
        assert_eq!(audio.next_zero_crossing_frame(1), None);
    }

    #[test]
    fn next_zero_crossing_averages_multiple_channels() {
        // Left: [1.0, 1.0], Right: [1.0, -1.0]; mixed: [1.0, 0.0]
        let audio = test_document(vec![1.0f32, 1.0, 1.0, -1.0], 2);
        assert_eq!(audio.next_zero_crossing_frame(0), Some(1));
    }
}
