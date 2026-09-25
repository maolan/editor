use crate::document::AudioDocument;

pub(crate) fn detect_markers(
    preview_samples: &[f32],
    channels: usize,
    threshold_db: f32,
    silence_samples: usize,
) -> Vec<(usize, String)> {
    let channels = channels.max(1);
    let frames = preview_samples.len() / channels;
    if frames == 0 {
        return Vec::new();
    }
    let threshold = 10.0f32.powf(threshold_db / 20.0);

    // Build contiguous segments of silent or non-silent frames.
    let mut segments: Vec<(usize, usize, bool)> = Vec::new();
    let mut segment_start = 0usize;
    let frame_amplitude = |frame: usize| {
        preview_samples[frame * channels..(frame + 1) * channels]
            .iter()
            .map(|sample| sample.abs())
            .sum::<f32>()
            / channels as f32
    };
    let mut is_silent = frame_amplitude(0) < threshold;

    for frame in 1..frames {
        let frame_silent = frame_amplitude(frame) < threshold;
        if frame_silent != is_silent {
            segments.push((segment_start, frame, is_silent));
            segment_start = frame;
            is_silent = frame_silent;
        }
    }
    segments.push((segment_start, frames, is_silent));

    // Treat short silent runs as part of the surrounding sound.
    let mut classified: Vec<(usize, usize, bool)> = Vec::new();
    for (start, end, silent) in segments {
        if silent && end.saturating_sub(start) >= silence_samples {
            classified.push((start, end, true));
        } else if let Some(last) = classified.last_mut() {
            if !last.2 {
                last.1 = end;
                continue;
            }
            classified.push((start, end, false));
        } else {
            classified.push((start, end, false));
        }
    }

    // Place markers at the boundaries of every sound region.
    let mut markers: Vec<(usize, String)> = Vec::new();
    let mut region_index = 1usize;
    for (index, (start, end, is_silence)) in classified.iter().enumerate() {
        if *is_silence {
            continue;
        }
        let preceded_by_silence = index == 0 || classified[index - 1].2;
        let followed_by_silence = index == classified.len() - 1 || classified[index + 1].2;
        if preceded_by_silence {
            markers.push((*start, format!("Region {region_index}")));
            region_index += 1;
        }
        if followed_by_silence {
            markers.push((*end, format!("Region {region_index}")));
            region_index += 1;
        }
    }

    markers
}

#[cfg(feature = "standalone")]
pub(crate) fn marker_ranges(markers: &[(usize, String)], frames: usize) -> Vec<(usize, usize)> {
    let mut sorted: Vec<usize> = markers.iter().map(|(sample, _)| *sample).collect();
    sorted.sort_unstable();
    sorted.dedup();
    let mut ranges = Vec::new();
    let mut start = 0usize;
    for marker in sorted {
        if marker > start && marker <= frames {
            ranges.push((start, marker));
            start = marker;
        }
    }
    if start < frames {
        ranges.push((start, frames));
    }
    ranges
}

#[cfg(feature = "standalone")]
pub(crate) fn marker_range_samples(audio: &AudioDocument, start: usize, end: usize) -> Vec<f32> {
    let channels = audio.channels.max(1);
    let frames = audio.frames();
    let start = start.min(frames);
    let end = end.min(frames);
    if start >= end {
        return Vec::new();
    }
    audio.preview_samples[start * channels..end * channels].to_vec()
}

pub(crate) fn nearest_marker_sample(audio: &AudioDocument, ratio: f32) -> Option<usize> {
    if audio.markers.is_empty() {
        return None;
    }
    let frames = audio.frames().max(1);
    let target = (ratio.clamp(0.0, 1.0) * frames as f32).round() as usize;
    audio
        .markers
        .iter()
        .min_by_key(|(sample, _)| sample.abs_diff(target))
        .map(|(sample, _)| *sample)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detect_markers_places_boundaries_around_sound_regions() {
        // 0..10 silence, 10..20 sound, 20..30 silence, 30..40 sound, 40..50 silence.
        let mut samples = vec![0.0f32; 10];
        samples.extend(vec![0.8f32; 10]);
        samples.extend(vec![0.0f32; 10]);
        samples.extend(vec![0.8f32; 10]);
        samples.extend(vec![0.0f32; 10]);
        let markers = detect_markers(&samples, 1, -60.0, 5);
        assert_eq!(
            markers,
            vec![
                (10, "Region 1".to_string()),
                (20, "Region 2".to_string()),
                (30, "Region 3".to_string()),
                (40, "Region 4".to_string()),
            ]
        );
    }

    #[test]
    fn detect_markers_ignores_short_silence() {
        // 0..10 silence, 10..15 sound, 15..17 short silence, 17..25 sound, 25..35 silence.
        let mut samples = vec![0.0f32; 10];
        samples.extend(vec![0.8f32; 5]);
        samples.extend(vec![0.0f32; 2]);
        samples.extend(vec![0.8f32; 8]);
        samples.extend(vec![0.0f32; 10]);
        let markers = detect_markers(&samples, 1, -60.0, 5);
        assert_eq!(
            markers,
            vec![(10, "Region 1".to_string()), (25, "Region 2".to_string()),]
        );
    }

    #[test]
    fn detect_markers_all_silence_returns_empty() {
        let samples = vec![0.0f32; 100];
        let markers = detect_markers(&samples, 1, -60.0, 5);
        assert!(markers.is_empty());
    }

    #[test]
    fn detect_markers_all_sound_returns_single_region() {
        let samples = vec![0.8f32; 100];
        let markers = detect_markers(&samples, 1, -60.0, 5);
        assert_eq!(
            markers,
            vec![(0, "Region 1".to_string()), (100, "Region 2".to_string()),]
        );
    }

    #[cfg(feature = "standalone")]
    #[test]
    fn marker_ranges_split_at_sorted_markers() {
        let markers = vec![(50, "A".to_string()), (20, "B".to_string())];
        assert_eq!(
            marker_ranges(&markers, 100),
            vec![(0, 20), (20, 50), (50, 100)]
        );
    }

    #[cfg(feature = "standalone")]
    #[test]
    fn marker_ranges_ignores_out_of_bounds_markers() {
        let markers = vec![(150, "A".to_string())];
        assert_eq!(marker_ranges(&markers, 100), vec![(0, 100)]);
    }

    #[cfg(feature = "standalone")]
    #[test]
    fn marker_range_samples_extracts_interleaved_range() {
        let audio = crate::document::test_document(vec![1.0f32, 2.0, 3.0, 4.0, 5.0, 6.0], 2);
        assert_eq!(marker_range_samples(&audio, 0, 2), vec![1.0, 2.0, 3.0, 4.0]);
    }
}
