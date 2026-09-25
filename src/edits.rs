use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy)]
pub(crate) struct AudioRegion {
    pub(crate) offset: usize,
    pub(crate) length: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum AudioEditAction {
    FadeIn {
        start_sample: usize,
        length_samples: usize,
    },
    FadeOut {
        start_sample: usize,
        length_samples: usize,
    },
    GainDb {
        start_sample: usize,
        length_samples: usize,
        delta_db: f32,
    },
    Reverse,
    Delete {
        start_sample: usize,
        length_samples: usize,
    },
    ReplaceWithSilence {
        start_sample: usize,
        length_samples: usize,
    },
}

#[derive(Debug, Clone, Copy)]
pub(crate) enum AudioEditKind {
    FadeIn,
    FadeOut,
    GainDb { delta_db: f32 },
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
pub struct AudioEdits {
    pub fade_in_samples: usize,
    pub fade_out_samples: usize,
    pub gain_db: f32,
    pub reversed: bool,
}

impl AudioEdits {
    #[cfg(feature = "standalone")]
    pub(crate) fn needs_rendered_preview_file(self) -> bool {
        self.fade_in_samples != 0 || self.fade_out_samples != 0 || self.gain_db != 0.0
    }
}

impl AudioEditAction {
    pub(crate) fn is_empty_for_frames(self, frames: usize) -> bool {
        match self {
            Self::FadeIn {
                start_sample,
                length_samples,
            }
            | Self::FadeOut {
                start_sample,
                length_samples,
            }
            | Self::GainDb {
                start_sample,
                length_samples,
                ..
            }
            | Self::Delete {
                start_sample,
                length_samples,
            }
            | Self::ReplaceWithSilence {
                start_sample,
                length_samples,
            } => start_sample >= frames || length_samples == 0,
            Self::Reverse => false,
        }
    }
}

pub fn summarize_audio_edit_actions(frames: usize, actions: &[AudioEditAction]) -> AudioEdits {
    let mut edits = AudioEdits::default();
    for action in actions {
        match *action {
            AudioEditAction::FadeIn {
                start_sample: 0,
                length_samples,
            } => {
                edits.fade_in_samples = edits.fade_in_samples.max(length_samples.min(frames));
            }
            AudioEditAction::FadeOut {
                start_sample,
                length_samples,
            } if start_sample.saturating_add(length_samples) >= frames => {
                edits.fade_out_samples = edits.fade_out_samples.max(length_samples.min(frames));
            }
            AudioEditAction::GainDb {
                start_sample,
                length_samples,
                delta_db,
            } if start_sample == 0 && length_samples >= frames => {
                edits.gain_db = (edits.gain_db + delta_db).clamp(-48.0, 24.0);
            }
            AudioEditAction::Reverse => edits.reversed = !edits.reversed,
            _ => {}
        }
    }
    edits
}

pub(crate) fn audio_edit_status(
    action: AudioEditAction,
    edits: AudioEdits,
    frames: usize,
) -> String {
    match action {
        AudioEditAction::FadeIn {
            start_sample,
            length_samples: _,
        } => {
            if start_sample == 0 {
                String::from("Fade in applied to preview.")
            } else {
                String::from("Fade in applied to selection.")
            }
        }
        AudioEditAction::FadeOut {
            start_sample,
            length_samples,
        } => {
            if start_sample.saturating_add(length_samples) >= frames {
                String::from("Fade out applied to preview.")
            } else {
                String::from("Fade out applied to selection.")
            }
        }
        AudioEditAction::GainDb {
            start_sample,
            length_samples,
            ..
        } => {
            if start_sample == 0 && length_samples > 0 {
                format!("Preview gain: {:+.1} dB.", edits.gain_db)
            } else {
                String::from("Volume adjusted on selection.")
            }
        }
        AudioEditAction::Reverse => {
            if edits.reversed {
                String::from("Clip reversed.")
            } else {
                String::from("Clip restored to forward playback.")
            }
        }
        AudioEditAction::Delete { .. } => String::from("Selection deleted."),
        AudioEditAction::ReplaceWithSilence { .. } => {
            String::from("Selection replaced with silence.")
        }
    }
}

pub(crate) fn default_fade_samples(frames: usize) -> usize {
    (frames / 20).clamp(240, 48_000).min(frames / 2)
}

fn reverse_samples(samples: &[f32], channels: usize) -> Vec<f32> {
    let channels = channels.max(1);
    let mut output = Vec::with_capacity(samples.len());
    for frame in samples.chunks_exact(channels).rev() {
        output.extend_from_slice(frame);
    }
    output
}

pub fn apply_audio_edit_actions(
    samples: &[f32],
    channels: usize,
    actions: &[AudioEditAction],
) -> Vec<f32> {
    let channels = channels.max(1);
    let mut output = samples.to_vec();
    for action in actions {
        apply_audio_edit_action_to_samples(&mut output, channels, *action);
    }
    output
}

pub fn apply_audio_edit_action_to_samples(
    samples: &mut Vec<f32>,
    channels: usize,
    action: AudioEditAction,
) {
    let channels = channels.max(1);
    match action {
        AudioEditAction::Reverse => {
            *samples = reverse_samples(samples, channels);
        }
        AudioEditAction::Delete {
            start_sample,
            length_samples,
        } => {
            let frames = samples.len() / channels;
            let start = start_sample.min(frames);
            let end = start.saturating_add(length_samples).min(frames);
            if start < end {
                samples.drain(start * channels..end * channels);
            }
        }
        AudioEditAction::ReplaceWithSilence {
            start_sample,
            length_samples,
        } => {
            apply_region(
                samples,
                channels,
                start_sample,
                length_samples,
                |_frame, sample| {
                    *sample = 0.0;
                },
            );
        }
        AudioEditAction::FadeIn {
            start_sample,
            length_samples,
        } => {
            apply_region(
                samples,
                channels,
                start_sample,
                length_samples,
                |pos, sample| {
                    let envelope = pos as f32 / length_samples.max(1) as f32;
                    *sample *= envelope;
                },
            );
        }
        AudioEditAction::FadeOut {
            start_sample,
            length_samples,
        } => {
            apply_region(
                samples,
                channels,
                start_sample,
                length_samples,
                |pos, sample| {
                    let envelope = (length_samples.saturating_sub(pos + 1) as f32
                        / length_samples.max(1) as f32)
                        .clamp(0.0, 1.0);
                    *sample *= envelope;
                },
            );
        }
        AudioEditAction::GainDb {
            start_sample,
            length_samples,
            delta_db,
        } => {
            let gain = 10.0f32.powf(delta_db / 20.0);
            apply_region(
                samples,
                channels,
                start_sample,
                length_samples,
                |_pos, sample| {
                    *sample = (*sample * gain).clamp(-1.0, 1.0);
                },
            );
        }
    }
}

fn apply_region(
    samples: &mut [f32],
    channels: usize,
    start_sample: usize,
    length_samples: usize,
    mut f: impl FnMut(usize, &mut f32),
) {
    let channels = channels.max(1);
    let frames = samples.len() / channels;
    let start = start_sample.min(frames);
    let end = start.saturating_add(length_samples).min(frames);
    if start >= end {
        return;
    }
    for frame in start..end {
        let pos = frame - start;
        for channel in 0..channels {
            f(pos, &mut samples[frame * channels + channel]);
        }
    }
}

#[cfg(test)]
#[derive(Debug, Clone, Copy)]
pub(crate) enum EditOperation {
    FadeIn,
    FadeOut,
    IncreaseVolume,
    DecreaseVolume,
}

#[cfg(test)]
pub(crate) fn apply_edit_to_samples(
    samples: &mut [f32],
    channels: usize,
    region: AudioRegion,
    operation: EditOperation,
) {
    let channels = channels.max(1);
    let frames = samples.len() / channels;
    let start = region.offset.min(frames);
    let end = start.saturating_add(region.length).min(frames);
    if start >= end {
        return;
    }

    match operation {
        EditOperation::FadeIn => {
            let fade_len = end - start;
            for frame in start..end {
                let envelope = (frame - start) as f32 / fade_len as f32;
                for channel in 0..channels {
                    let index = frame * channels + channel;
                    samples[index] *= envelope;
                }
            }
        }
        EditOperation::FadeOut => {
            let fade_len = end - start;
            for frame in start..end {
                let envelope = (end - 1 - frame) as f32 / fade_len as f32;
                for channel in 0..channels {
                    let index = frame * channels + channel;
                    samples[index] *= envelope;
                }
            }
        }
        EditOperation::IncreaseVolume => {
            let gain = 10.0f32.powf(1.0 / 20.0);
            for frame in start..end {
                for channel in 0..channels {
                    let index = frame * channels + channel;
                    samples[index] = (samples[index] * gain).clamp(-1.0, 1.0);
                }
            }
        }
        EditOperation::DecreaseVolume => {
            let gain = 10.0f32.powf(-1.0 / 20.0);
            for frame in start..end {
                for channel in 0..channels {
                    let index = frame * channels + channel;
                    samples[index] = (samples[index] * gain).clamp(-1.0, 1.0);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn apply_edit_to_samples_fades_in_region() {
        let mut samples = vec![1.0f32; 8];
        apply_edit_to_samples(
            &mut samples,
            1,
            AudioRegion {
                offset: 2,
                length: 4,
            },
            EditOperation::FadeIn,
        );
        assert_eq!(samples, vec![1.0, 1.0, 0.0, 0.25, 0.5, 0.75, 1.0, 1.0]);
    }

    #[test]
    fn apply_edit_to_samples_fades_out_region() {
        let mut samples = vec![1.0f32; 8];
        apply_edit_to_samples(
            &mut samples,
            1,
            AudioRegion {
                offset: 2,
                length: 4,
            },
            EditOperation::FadeOut,
        );
        assert_eq!(samples, vec![1.0, 1.0, 0.75, 0.5, 0.25, 0.0, 1.0, 1.0]);
    }

    #[test]
    fn apply_edit_to_samples_adjusts_volume_in_region() {
        let mut samples = vec![0.5f32; 8];
        apply_edit_to_samples(
            &mut samples,
            1,
            AudioRegion {
                offset: 2,
                length: 4,
            },
            EditOperation::IncreaseVolume,
        );
        let expected_gain = 10.0f32.powf(1.0 / 20.0);
        for (index, sample) in samples.iter().enumerate() {
            let expected = if (2..6).contains(&index) {
                0.5 * expected_gain
            } else {
                0.5
            };
            assert!((sample - expected).abs() < 1.0e-5, "index {index}");
        }
    }

    #[test]
    fn apply_edit_to_samples_decreases_volume_in_region() {
        let mut samples = vec![1.0f32; 8];
        apply_edit_to_samples(
            &mut samples,
            1,
            AudioRegion {
                offset: 2,
                length: 4,
            },
            EditOperation::DecreaseVolume,
        );
        let expected_gain = 10.0f32.powf(-1.0 / 20.0);
        for (index, sample) in samples.iter().enumerate() {
            let expected = if (2..6).contains(&index) {
                expected_gain
            } else {
                1.0
            };
            assert!((sample - expected).abs() < 1.0e-5, "index {index}");
        }
    }

    #[test]
    fn apply_edit_to_samples_clamps_region_to_bounds() {
        let mut samples = vec![1.0f32; 4];
        apply_edit_to_samples(
            &mut samples,
            1,
            AudioRegion {
                offset: 2,
                length: 100,
            },
            EditOperation::FadeIn,
        );
        assert_eq!(samples, vec![1.0, 1.0, 0.0, 0.5]);
    }
}
