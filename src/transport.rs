use std::sync::Arc;

#[cfg(feature = "standalone")]
use maolan_engine::message::Action as EngineAction;
use maolan_widgets::iced::{Element, Task};
use maolan_widgets::meters;

use crate::document::AudioDocument;
#[cfg(feature = "standalone")]
use crate::edits::apply_audio_edit_actions;
use crate::edits::{AudioEditAction, AudioEditKind};
use crate::message::{AudioBuffer, Message};
use crate::state::EditApp;

pub fn set_embedded_transport(app: &mut EditApp, playing: bool, playhead_samples: usize) {
    app.playing = playing;
    let frames = app.audio.as_ref().map(AudioDocument::frames).unwrap_or(0);
    app.playhead_samples = playhead_samples.min(frames);
}

pub struct HostPreview {
    pub samples: Arc<Vec<f32>>,
    pub channels: usize,
    pub start_sample: usize,
}

#[derive(Debug, Clone)]
pub struct RenderedAudio {
    pub samples: Arc<Vec<f32>>,
    pub channels: usize,
    pub sample_rate: u32,
}

pub fn host_preview(app: &EditApp) -> Option<HostPreview> {
    let audio = app.audio.as_ref()?;
    Some(HostPreview {
        samples: Arc::new(audio.preview_samples.clone()),
        channels: audio.channels,
        start_sample: app.playhead_samples,
    })
}

pub fn is_playing(app: &EditApp) -> bool {
    app.playing
}

pub fn rendered_audio(app: &EditApp) -> Option<RenderedAudio> {
    let audio = app.audio.as_ref()?;
    Some(RenderedAudio {
        samples: Arc::new(audio.preview_samples.clone()),
        channels: audio.channels,
        sample_rate: audio.sample_rate,
    })
}

pub fn current_audio_edits(app: &EditApp) -> Option<crate::edits::AudioEdits> {
    app.audio.as_ref().map(|audio| audio.edit_summary())
}

pub fn current_audio_edit_actions(app: &EditApp) -> Option<Vec<AudioEditAction>> {
    app.audio.as_ref().map(|audio| audio.edit_actions.clone())
}

pub fn audio_edit_action_for_message(app: &EditApp, message: &Message) -> Option<AudioEditAction> {
    let audio = app.audio.as_ref()?;
    match message {
        Message::EditAction(action) => Some(*action),
        Message::FadeIn => Some(audio.region_action(AudioEditKind::FadeIn)),
        Message::FadeOut => Some(audio.region_action(AudioEditKind::FadeOut)),
        Message::IncreaseVolume => {
            Some(audio.region_action(AudioEditKind::GainDb { delta_db: 1.0 }))
        }
        Message::DecreaseVolume => {
            Some(audio.region_action(AudioEditKind::GainDb { delta_db: -1.0 }))
        }
        Message::Reverse => Some(AudioEditAction::Reverse),
        Message::DeleteSelection => app.selection_samples.and_then(|(start, end)| {
            (start < end).then_some(AudioEditAction::Delete {
                start_sample: start,
                length_samples: end - start,
            })
        }),
        _ => None,
    }
}

pub fn open_audio(app: &mut EditApp, audio: AudioBuffer) -> Task<Message> {
    crate::app::update(app, Message::OpenAudio(audio))
}

pub(crate) fn reset_after_close(app: &mut EditApp) {
    #[cfg(feature = "standalone")]
    let engine_playback = if app.standalone_ready {
        app.engine_playback.take()
    } else {
        None
    };

    *app = EditApp {
        status: String::from("Open an audio file to view its waveform."),
        standalone_ready: app.standalone_ready,
        setup: app.setup.clone(),
        #[cfg(feature = "standalone")]
        engine_playback,
        ..EditApp::default()
    };
}

#[cfg(feature = "standalone")]
pub(crate) fn stop_engine_playback(app: &EditApp) -> Task<Message> {
    if let Some(playback) = app.engine_playback.as_ref() {
        let client = playback.client.clone();
        Task::perform(
            async move { crate::engine::send_engine(&client, EngineAction::Stop).await },
            Message::StandalonePlaybackStopped,
        )
    } else {
        Task::none()
    }
}

#[cfg(not(feature = "standalone"))]
pub(crate) fn stop_engine_playback(_app: &EditApp) -> Task<Message> {
    Task::none()
}

#[cfg(feature = "standalone")]
pub(crate) fn play_standalone(app: &mut EditApp) -> Task<Message> {
    if app.busy || app.preparing_playback {
        app.status = String::from("Preparing audio for playback.");
        return Task::none();
    }
    if app.playing {
        return Task::none();
    }
    let Some(audio) = app.audio.as_ref() else {
        app.status = String::from("No audio file is open.");
        return Task::none();
    };
    let Some(playback) = app.engine_playback.as_ref() else {
        if !app.standalone_ready {
            app.playing = true;
            app.status = String::from("Playing preview.");
            if app.playhead_samples >= audio.frames() {
                app.playhead_samples = 0;
            }
            return Task::none();
        }
        app.status = String::from("Open audio hardware before playback.");
        return Task::none();
    };
    app.playing = true;
    app.status = String::from("Playing.");
    if app.playhead_samples >= audio.frames() {
        app.playhead_samples = 0;
    }
    let start = app.playhead_samples;
    let client = playback.client.clone();
    Task::perform(
        async move { crate::engine::start_engine_playback(client, start).await },
        Message::StandalonePlaybackStarted,
    )
}

#[cfg(not(feature = "standalone"))]
pub(crate) fn play_standalone(app: &mut EditApp) -> Task<Message> {
    if app.busy || app.preparing_playback {
        app.status = String::from("Preparing audio for playback.");
        return Task::none();
    }
    if app.playing {
        return Task::none();
    }
    let Some(audio) = app.audio.as_ref() else {
        app.status = String::from("No audio file is open.");
        return Task::none();
    };
    app.playing = true;
    app.status = String::from("Playing preview.");
    if app.playhead_samples >= audio.frames() {
        app.playhead_samples = 0;
    }
    Task::none()
}

pub(crate) fn refresh_standalone_playhead(app: &mut EditApp) -> bool {
    if !app.playing {
        return false;
    }
    let Some(audio) = app.audio.as_ref() else {
        app.playing = false;
        return false;
    };
    let step = (audio.sample_rate / 25).max(1) as usize;
    app.playhead_samples = app.playhead_samples.saturating_add(step);
    if app.playhead_samples >= audio.frames() {
        app.playhead_samples = audio.frames();
        return true;
    }
    false
}

pub(crate) fn playhead_ratio(app: &EditApp) -> Option<f32> {
    let frames = app.audio.as_ref()?.frames().max(1);
    Some((app.playhead_samples as f32 / frames as f32).clamp(0.0, 1.0))
}

pub(crate) fn selection_ratio(app: &EditApp) -> Option<(f32, f32)> {
    let frames = app.audio.as_ref()?.frames().max(1) as f32;
    let (start, end) = app.selection_samples?;
    Some((start as f32 / frames, end as f32 / frames))
}

pub(crate) fn sample_at_ratio(app: &EditApp, ratio: f32) -> Option<usize> {
    let frames = app.audio.as_ref()?.frames();
    Some(((ratio.clamp(0.0, 1.0) * frames as f32).round() as usize).min(frames))
}

pub(crate) fn selection_duration_seconds(app: &EditApp) -> f32 {
    let Some(audio) = app.audio.as_ref() else {
        return 0.0;
    };
    let Some((start, end)) = app.selection_samples else {
        return 0.0;
    };
    end.saturating_sub(start) as f32 / audio.sample_rate.max(1) as f32
}

pub(crate) fn vu_meter(app: &EditApp) -> Element<'_, Message> {
    let levels = vu_levels_db(app);
    meters::meters(levels.len(), &levels, 0.0)
}

pub fn vu_levels_db(app: &EditApp) -> Vec<f32> {
    let Some(audio) = app.audio.as_ref() else {
        return vec![-90.0, -90.0];
    };
    let channels = audio.channels.max(1);
    let frames = audio.frames();
    let start = app.playhead_samples.min(frames);
    let end = start.saturating_add(2048).min(frames);
    let channel_count = channels.min(2);
    let mut levels = vec![0.0f32; channel_count.max(1)];
    if start >= end {
        return levels;
    }
    for frame in start..end {
        for (channel, level) in levels.iter_mut().enumerate().take(channel_count) {
            let sample = audio.preview_samples[frame * channels + channel].abs();
            *level = (*level).max(sample);
        }
    }
    levels
        .into_iter()
        .map(|level| {
            if level <= 1.0e-9 {
                -90.0
            } else {
                (20.0 * level.log10()).clamp(-90.0, 20.0)
            }
        })
        .collect()
}

#[cfg(feature = "standalone")]
pub(crate) fn prepare_document_track(app: &mut EditApp) -> Task<Message> {
    if !app.standalone_ready {
        return Task::none();
    }
    let (Some(audio), Some(playback)) = (app.audio.as_ref(), app.engine_playback.as_ref()) else {
        return Task::none();
    };
    let client = playback.client.clone();
    let path = audio.source_path.clone();
    let samples = apply_audio_edit_actions(&audio.samples, audio.channels, &audio.edit_actions);
    let channels = audio.channels;
    let sample_rate = audio.sample_rate;
    let clip_len = audio.frames();
    let render_preview = audio.edits.needs_rendered_preview_file();
    let clip_offset = if render_preview {
        0
    } else {
        audio.clip_region.map(|region| region.offset).unwrap_or(0)
    };
    app.engine_clip_path = Some(if render_preview {
        crate::engine::preview_path(&path)
    } else {
        path.clone()
    });
    app.preparing_playback = true;
    app.status = String::from("Preparing audio for playback...");
    let request = crate::engine::EngineDocumentRequest {
        path,
        samples,
        channels,
        sample_rate,
        clip_len,
        clip_offset,
        render_preview,
        reversed: audio.edits.reversed,
    };
    Task::perform(
        async move { crate::engine::prepare_engine_document(client, request).await },
        Message::EngineDocumentPrepared,
    )
}

#[cfg(not(feature = "standalone"))]
pub(crate) fn prepare_document_track(_app: &mut EditApp) -> Task<Message> {
    Task::none()
}
