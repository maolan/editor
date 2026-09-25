#[cfg(feature = "standalone")]
use std::path::Path;
#[cfg(feature = "standalone")]
use std::path::PathBuf;

#[cfg(feature = "standalone")]
use crate::audio_codec::{AudioDither, AudioEncodeFormat, WavBitDepth, encode_audio_to_file};
use maolan_widgets::iced::Task;
#[cfg(feature = "standalone")]
use rubato::audioadapter_buffers::direct::SequentialSliceOfVecs;
#[cfg(feature = "standalone")]
use rubato::{Fft, FixedSync, Resampler};

#[cfg(feature = "standalone")]
use crate::dialogs::{ExportBitDepth, ExportFormat};
use crate::document::AudioDocument;
use crate::edits::AudioRegion;
#[cfg(feature = "standalone")]
use crate::markers::{marker_range_samples, marker_ranges};
use crate::message::{AudioBuffer, Message};
use crate::state::EditApp;

pub(crate) fn load_audio_buffer(
    app: &mut EditApp,
    audio: AudioBuffer,
    region: Option<AudioRegion>,
) -> Task<Message> {
    app.busy = true;
    app.busy_progress = 0.0;
    app.status = format!("Opening {}...", audio.name);
    Task::perform(
        async move { AudioDocument::from_interleaved(audio, region) },
        Message::DocumentLoaded,
    )
}

#[cfg(feature = "standalone")]
pub(crate) fn load_document(
    app: &mut EditApp,
    path: PathBuf,
    region: Option<AudioRegion>,
    _timeline_region: Option<AudioRegion>,
) -> Task<Message> {
    app.busy = true;
    app.busy_progress = 0.0;
    app.status = match region {
        Some(_) => format!("Opening clip from {}...", path.display()),
        None => format!("Opening {}...", path.display()),
    };
    Task::run(
        {
            let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
            std::thread::spawn(move || {
                let mut last_bucket = None;
                let mut last_status = String::new();
                let progress_tx = tx.clone();
                let result = AudioDocument::open_with_progress(path, region, |progress, status| {
                    let progress = progress.clamp(0.0, 1.0);
                    let bucket = (progress * 100.0).round() as u8;
                    if last_bucket == Some(bucket) && last_status == status {
                        return;
                    }
                    last_bucket = Some(bucket);
                    last_status = status.to_string();
                    let _ = progress_tx.send(Message::DocumentLoadProgress {
                        progress,
                        status: status.to_string(),
                    });
                });
                let _ = tx.send(Message::DocumentLoaded(result));
            });

            maolan_widgets::iced::futures::stream::unfold(rx, |mut rx| async move {
                rx.recv().await.map(|msg| (msg, rx))
            })
        },
        |msg| msg,
    )
}

pub(crate) fn document_status(audio: &AudioDocument) -> String {
    match audio.clip_region {
        Some(region) => format!(
            "{} - clip {}..{} samples, {} ch, {} Hz, {} frames",
            audio.source_path.display(),
            region.offset,
            region.offset.saturating_add(region.length),
            audio.channels,
            audio.sample_rate,
            audio.frames()
        ),
        None => format!(
            "{} - {} ch, {} Hz, {} frames",
            audio.source_path.display(),
            audio.channels,
            audio.sample_rate,
            audio.frames()
        ),
    }
}

#[cfg(feature = "standalone")]
pub(crate) async fn save_document(
    path: PathBuf,
    samples: Vec<f32>,
    channels: usize,
    sample_rate: u32,
) -> Result<PathBuf, String> {
    let format = encode_format_for_path(&path)?;
    encode_audio_to_file(
        &path,
        &samples,
        channels,
        sample_rate,
        format,
        AudioDither::None,
    )
    .map_err(|err| format!("Failed to save '{}': {err}", path.display()))?;
    Ok(path)
}

#[cfg(feature = "standalone")]
pub(crate) async fn choose_export_directory() -> Option<PathBuf> {
    rfd::FileDialog::new().pick_folder()
}

#[cfg(feature = "standalone")]
pub(crate) async fn export_marker_ranges(
    directory: PathBuf,
    audio: AudioDocument,
    format: ExportFormat,
    bit_depth: ExportBitDepth,
    sample_rate: u32,
) -> Result<usize, String> {
    let channels = audio.channels.max(1);
    let frames = audio.frames();
    let ranges = marker_ranges(&audio.markers, frames);
    if ranges.is_empty() {
        return Err(String::from("No ranges to export."));
    }

    let source_stem = audio
        .source_path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("export");
    let encode_format = export_encode_format(format, bit_depth);
    let extension = format.extension();

    std::fs::create_dir_all(&directory)
        .map_err(|err| format!("Failed to create export directory: {err}"))?;

    let total = ranges.len();
    for (index, (start, end)) in ranges.iter().enumerate() {
        let range_samples = marker_range_samples(&audio, *start, *end);
        let resampled = if sample_rate == audio.sample_rate {
            range_samples
        } else {
            resample_interleaved(&range_samples, channels, audio.sample_rate, sample_rate)?
        };
        let filename = export_filename(source_stem, index + 1, extension);
        let path = directory.join(filename);
        encode_audio_to_file(
            &path,
            &resampled,
            channels,
            sample_rate,
            encode_format,
            AudioDither::None,
        )
        .map_err(|err| format!("Failed to export '{}': {err}", path.display()))?;
    }

    Ok(total)
}

#[cfg(feature = "standalone")]
fn export_encode_format(format: ExportFormat, bit_depth: ExportBitDepth) -> AudioEncodeFormat {
    match format {
        ExportFormat::Wav => AudioEncodeFormat::Wav(match bit_depth {
            ExportBitDepth::Bits16 => WavBitDepth::Int16,
            ExportBitDepth::Bits24 => WavBitDepth::Int24,
            ExportBitDepth::Bits32 => WavBitDepth::Int32,
        }),
        ExportFormat::Flac => AudioEncodeFormat::Flac(bit_depth.bits()),
        ExportFormat::OggFlac => AudioEncodeFormat::OggFlac(bit_depth.bits()),
        ExportFormat::Mp3 => AudioEncodeFormat::Mp3,
    }
}

#[cfg(feature = "standalone")]
fn export_filename(stem: &str, index: usize, extension: &str) -> String {
    format!("{stem}_{index:03}.{extension}")
}

#[cfg(feature = "standalone")]
fn resample_interleaved(
    samples: &[f32],
    channels: usize,
    from_rate: u32,
    to_rate: u32,
) -> Result<Vec<f32>, String> {
    if from_rate == to_rate {
        return Ok(samples.to_vec());
    }
    let channels = channels.max(1);
    let frames = samples.len() / channels;
    if frames == 0 {
        return Ok(Vec::new());
    }

    let mut input_per_channel: Vec<Vec<f32>> = vec![Vec::with_capacity(frames); channels];
    for frame in samples.chunks_exact(channels) {
        for (channel, sample) in frame.iter().copied().enumerate() {
            input_per_channel[channel].push(sample);
        }
    }

    let input = SequentialSliceOfVecs::new(&input_per_channel, channels, frames)
        .map_err(|err| format!("Failed to wrap input samples: {err}"))?;
    let mut resampler = Fft::<f32>::new(
        from_rate as usize,
        to_rate as usize,
        1024,
        channels,
        FixedSync::Both,
    )
    .map_err(|err| format!("Failed to create resampler: {err}"))?;

    let output = resampler
        .process_all(&input, frames, None)
        .map_err(|err| format!("Failed to resample: {err}"))?;

    let expected_output_frames =
        (frames as f64 * to_rate as f64 / from_rate as f64).round() as usize;
    let mut output_samples = output.take_data();
    output_samples.truncate(expected_output_frames * channels);

    Ok(output_samples)
}

#[cfg(feature = "standalone")]
pub(crate) async fn open_audio_dialog() -> Option<PathBuf> {
    rfd::FileDialog::new()
        .add_filter(
            "Audio",
            &["wav", "flac", "mp3", "ogg", "vorbis", "m4a", "aac", "alac"],
        )
        .pick_file()
}

#[cfg(feature = "standalone")]
pub(crate) async fn save_audio_dialog(current: Option<PathBuf>) -> Option<PathBuf> {
    let mut dialog =
        rfd::FileDialog::new().add_filter("Maolan audio export", &["wav", "flac", "mp3", "ogg"]);
    if let Some(path) = current.as_ref() {
        if let Some(parent) = path.parent() {
            dialog = dialog.set_directory(parent);
        }
        if let Some(name) = path.file_name() {
            dialog = dialog.set_file_name(name.to_string_lossy());
        }
    }
    dialog.save_file()
}

#[cfg(feature = "standalone")]
pub(crate) fn encode_format_for_path(path: &Path) -> Result<AudioEncodeFormat, String> {
    let ext = path
        .extension()
        .and_then(|ext| ext.to_str())
        .map(str::to_ascii_lowercase)
        .ok_or_else(|| {
            String::from("Save path needs an audio extension: wav, flac, mp3, or ogg.")
        })?;

    match ext.as_str() {
        "wav" => Ok(AudioEncodeFormat::Wav(WavBitDepth::Float32)),
        "flac" => Ok(AudioEncodeFormat::Flac(24)),
        "mp3" => Ok(AudioEncodeFormat::Mp3),
        "ogg" => Ok(AudioEncodeFormat::OggFlac(24)),
        _ => Err(format!(
            "Cannot save '{}': supported save formats are wav, flac, mp3, and ogg.",
            path.display()
        )),
    }
}

#[cfg(all(test, feature = "standalone"))]
mod tests {
    use super::*;

    #[test]
    fn encode_format_matches_extensions() {
        assert!(matches!(
            encode_format_for_path(Path::new("x.wav")).unwrap(),
            AudioEncodeFormat::Wav(WavBitDepth::Float32)
        ));
        assert!(matches!(
            encode_format_for_path(Path::new("x.flac")).unwrap(),
            AudioEncodeFormat::Flac(24)
        ));
        assert!(matches!(
            encode_format_for_path(Path::new("x.mp3")).unwrap(),
            AudioEncodeFormat::Mp3
        ));
        assert!(matches!(
            encode_format_for_path(Path::new("x.ogg")).unwrap(),
            AudioEncodeFormat::OggFlac(24)
        ));
    }

    #[test]
    fn export_filename_includes_index_and_extension() {
        assert_eq!(export_filename("track", 7, "wav"), "track_007.wav");
    }

    #[test]
    fn export_encode_format_maps_formats() {
        assert!(matches!(
            export_encode_format(ExportFormat::Wav, ExportBitDepth::Bits24),
            AudioEncodeFormat::Wav(WavBitDepth::Int24)
        ));
        assert!(matches!(
            export_encode_format(ExportFormat::Flac, ExportBitDepth::Bits16),
            AudioEncodeFormat::Flac(16)
        ));
        assert!(matches!(
            export_encode_format(ExportFormat::OggFlac, ExportBitDepth::Bits32),
            AudioEncodeFormat::OggFlac(32)
        ));
        assert!(matches!(
            export_encode_format(ExportFormat::Mp3, ExportBitDepth::Bits24),
            AudioEncodeFormat::Mp3
        ));
    }

    #[test]
    fn resample_interleaved_identity_when_rates_match() {
        let samples = vec![0.1f32, 0.2, 0.3, 0.4, 0.5, 0.6];
        let output = resample_interleaved(&samples, 2, 48_000, 48_000).unwrap();
        assert_eq!(output, samples);
    }

    #[test]
    fn resample_interleaved_changes_length_when_rates_differ() {
        let samples: Vec<f32> = (0..960).map(|i| (i as f32 / 960.0).sin()).collect();
        let output = resample_interleaved(&samples, 1, 48_000, 24_000).unwrap();
        assert!(!output.is_empty());
        assert!(output.len() < samples.len());
    }

    #[tokio::test]
    async fn export_marker_ranges_creates_files() {
        let dir = std::env::temp_dir().join(format!(
            "maolan-edit-export-test-{}",
            std::time::UNIX_EPOCH.elapsed().unwrap().as_secs()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        let mut audio = crate::document::test_document(vec![0.5f32; 48_000], 1);
        audio.source_path = PathBuf::from("test_track.wav");
        audio.markers = vec![(12_000, "A".to_string()), (36_000, "B".to_string())];
        let result = export_marker_ranges(
            dir.clone(),
            audio,
            ExportFormat::Wav,
            ExportBitDepth::Bits16,
            48_000,
        )
        .await;
        assert_eq!(result.unwrap(), 3);
        assert!(dir.join("test_track_001.wav").exists());
        assert!(dir.join("test_track_002.wav").exists());
        assert!(dir.join("test_track_003.wav").exists());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
