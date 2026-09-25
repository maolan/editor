use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use maolan_engine::message::generate_clip_id;
use maolan_engine::{
    client::Client as EngineClient,
    kind::Kind,
    message::{Action as EngineAction, Message as EngineMessage, QueryReply},
};

use crate::devices::StartupSetup;
use crate::io::save_document;

#[derive(Debug)]
pub(crate) struct EnginePlayback {
    pub(crate) client: EngineClient,
}

pub(crate) struct EngineDocumentRequest {
    pub(crate) path: PathBuf,
    pub(crate) samples: Vec<f32>,
    pub(crate) channels: usize,
    pub(crate) sample_rate: u32,
    pub(crate) clip_len: usize,
    pub(crate) clip_offset: usize,
    pub(crate) render_preview: bool,
    pub(crate) reversed: bool,
}

#[derive(Debug, Clone, Copy)]
pub(crate) enum PluginFormat {
    Vst3,
    Clap,
    #[cfg(unix)]
    Lv2,
}

pub(crate) async fn scan_plugins_startup(client: EngineClient, format: PluginFormat) -> bool {
    let mut rx = client.subscribe().await;
    let action = match format {
        PluginFormat::Vst3 => EngineAction::ListVst3Plugins,
        PluginFormat::Clap => EngineAction::ListClapPlugins,
        #[cfg(unix)]
        PluginFormat::Lv2 => EngineAction::ListLv2Plugins,
    };
    let Ok(()) = send_engine(&client, action).await else {
        return false;
    };
    let accepts = |reply: &QueryReply| match format {
        PluginFormat::Vst3 => {
            matches!(
                reply,
                QueryReply::Vst3Plugins(_) | QueryReply::Vst3PluginsUnavailable { .. }
            )
        }
        PluginFormat::Clap => {
            matches!(
                reply,
                QueryReply::ClapPlugins(_) | QueryReply::ClapPluginsUnavailable { .. }
            )
        }
        #[cfg(unix)]
        PluginFormat::Lv2 => {
            matches!(
                reply,
                QueryReply::Lv2Plugins(_) | QueryReply::Lv2PluginsUnavailable { .. }
            )
        }
    };
    wait_for_query_reply(&mut rx, accepts).await.is_ok()
}

pub(crate) async fn open_standalone_engine(setup: StartupSetup) -> Result<EngineClient, String> {
    let client = EngineClient::default();
    let mut rx = client.subscribe().await;
    send_engine(&client, EngineAction::Stop).await?;
    scan_plugins(&client, &mut rx).await?;
    send_engine(
        &client,
        EngineAction::OpenAudioDevice {
            device: selected_output_device(&setup),
            input_device: selected_input_device(&setup),
            sample_rate_hz: setup.sample_rate_hz,
            bits: selected_bits(&setup),
            exclusive: setup.exclusive,
            period_frames: selected_period_frames(&setup),
            nperiods: setup.nperiods,
            sync_mode: setup.sync_mode,
            actual_period_frames: 0,
            input_channels: 0,
            output_channels: 0,
            bytes_per_frame: 0,
            ring_buffer_multiplier: 8,
            auto_open_midi_devices: false,
        },
    )
    .await?;
    wait_for_engine_response(&mut rx, |action| {
        matches!(action, EngineAction::OpenAudioDevice { .. })
    })
    .await?;
    Ok(client)
}

async fn scan_plugins(
    client: &EngineClient,
    rx: &mut tokio::sync::mpsc::Receiver<EngineMessage>,
) -> Result<(), String> {
    #[cfg(unix)]
    {
        send_engine(client, EngineAction::ListLv2Plugins).await?;
    }
    send_engine(client, EngineAction::ListVst3Plugins).await?;
    send_engine(client, EngineAction::ListClapPlugins).await?;

    #[cfg(unix)]
    wait_for_query_reply(rx, |reply| {
        matches!(
            reply,
            QueryReply::Lv2Plugins(_) | QueryReply::Lv2PluginsUnavailable { .. }
        )
    })
    .await?;
    wait_for_query_reply(rx, |reply| {
        matches!(
            reply,
            QueryReply::Vst3Plugins(_) | QueryReply::Vst3PluginsUnavailable { .. }
        )
    })
    .await?;
    wait_for_query_reply(rx, |reply| {
        matches!(
            reply,
            QueryReply::ClapPlugins(_) | QueryReply::ClapPluginsUnavailable { .. }
        )
    })
    .await?;
    Ok(())
}

pub(crate) async fn wait_for_query_reply(
    rx: &mut tokio::sync::mpsc::Receiver<EngineMessage>,
    mut accepts: impl FnMut(&QueryReply) -> bool,
) -> Result<(), String> {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return Err(String::from("Timed out waiting for audio engine."));
        }
        let Some(message) = tokio::time::timeout(remaining, rx.recv())
            .await
            .map_err(|_| String::from("Timed out waiting for audio engine."))?
        else {
            return Err(String::from("Audio engine response channel closed."));
        };
        if let EngineMessage::QueryReply(reply) = message
            && accepts(&reply)
        {
            return Ok(());
        }
    }
}

pub(crate) async fn prepare_engine_document(
    client: EngineClient,
    request: EngineDocumentRequest,
) -> Result<(), String> {
    let clip_path = if request.render_preview {
        let temp_path = preview_path(&request.path);
        save_document(
            temp_path.clone(),
            request.samples,
            request.channels,
            request.sample_rate,
        )
        .await?;
        temp_path
    } else {
        request.path
    };
    let track = "editor-preview".to_string();
    send_engine(&client, EngineAction::Stop).await?;
    let _ = send_engine(&client, EngineAction::RemoveTrack(track.clone())).await;
    let mut rx = client.subscribe().await;
    send_engine(
        &client,
        EngineAction::AddTrack {
            name: track.clone(),
            audio_ins: request.channels,
            midi_ins: 0,
            audio_outs: request.channels,
            midi_outs: 0,
            folder: false,
            mixosc_addr: None,
        },
    )
    .await?;
    wait_for_engine_response(
        &mut rx,
        |action| matches!(action, EngineAction::AddTrack { name, .. } if name == "editor-preview"),
    )
    .await?;
    send_engine(
        &client,
        EngineAction::AddClip {
            clip_id: generate_clip_id(),
            name: clip_path.to_string_lossy().to_string(),
            track_name: track.clone(),
            start: 0,
            length: request.clip_len,
            offset: request.clip_offset,
            input_channel: 0,
            muted: false,
            reversed: request.reversed,
            gain_db: 0.0,
            peaks_file: None,
            kind: Kind::Audio,
            fade_enabled: true,
            fade_in_samples: 240,
            fade_out_samples: 240,
            source_name: None,
            source_offset: None,
            source_length: None,
            preview_name: None,
            pitch_correction_points: Vec::new(),
            pitch_correction_frame_likeness: None,
            pitch_correction_inertia_ms: None,
            pitch_correction_formant_compensation: None,
            pitch_correction_detector: maolan_engine::message::PitchCorrectionDetector::Classic,
            pitch_correction_mode: maolan_engine::message::PitchCorrectionMode::Shift,
            plugin_graph_json: None,
        },
    )
    .await?;
    wait_for_engine_response(
        &mut rx,
        |action| matches!(action, EngineAction::AddClip { track_name, .. } if track_name == "editor-preview"),
    )
    .await?;
    for channel in 0..request.channels.clamp(1, 2) {
        send_engine(
            &client,
            EngineAction::Connect {
                from_track: track.clone(),
                from_port: channel,
                to_track: "hw:out".to_string(),
                to_port: channel,
                kind: Kind::Audio,
            },
        )
        .await?;
        wait_for_engine_response(&mut rx, |action| {
            matches!(action, EngineAction::Connect {
                from_track,
                from_port,
                to_track,
                to_port,
                kind,
            } if from_track == "editor-preview"
                && *from_port == channel
                && to_track == "hw:out"
                && *to_port == channel
                && *kind == Kind::Audio)
        })
        .await?;
    }
    send_engine(&client, EngineAction::SetClipPlaybackEnabled(true)).await?;
    wait_for_engine_response(&mut rx, |action| {
        matches!(action, EngineAction::SetClipPlaybackEnabled(true))
    })
    .await?;
    Ok(())
}

pub(crate) async fn start_engine_playback(
    client: EngineClient,
    start: usize,
) -> Result<(), String> {
    let mut rx = client.subscribe().await;
    send_engine(&client, EngineAction::SetClipPlaybackEnabled(true)).await?;
    wait_for_engine_response(&mut rx, |action| {
        matches!(action, EngineAction::SetClipPlaybackEnabled(true))
    })
    .await?;
    send_engine(&client, EngineAction::TransportPosition(start)).await?;
    send_engine(&client, EngineAction::Play).await?;
    wait_for_engine_response(&mut rx, |action| matches!(action, EngineAction::Play)).await?;
    Ok(())
}

fn selected_output_device(setup: &StartupSetup) -> String {
    if setup.audio_engine.is_jack() {
        String::from("jack")
    } else {
        setup
            .output_device
            .as_ref()
            .map(|device| device.id.clone())
            .unwrap_or_else(|| crate::devices::default_audio_device(setup.audio_engine).to_string())
    }
}

fn selected_input_device(setup: &StartupSetup) -> Option<String> {
    if setup.audio_engine.is_jack() {
        None
    } else {
        setup.input_device.as_ref().map(|device| device.id.clone())
    }
}

pub(crate) fn selected_bits(setup: &StartupSetup) -> i32 {
    if setup.audio_engine.is_jack() {
        32
    } else {
        setup.bits as i32
    }
}

fn selected_period_frames(setup: &StartupSetup) -> usize {
    let options = crate::devices::period_frame_options(setup);
    if options.contains(&setup.period_frames) {
        setup.period_frames
    } else {
        options
            .iter()
            .copied()
            .find(|value| *value >= setup.period_frames)
            .or_else(|| options.last().copied())
            .unwrap_or(setup.period_frames)
    }
}

pub(crate) async fn send_engine(client: &EngineClient, action: EngineAction) -> Result<(), String> {
    client.send(EngineMessage::Request(action)).await
}

pub(crate) async fn wait_for_engine_response(
    rx: &mut tokio::sync::mpsc::Receiver<EngineMessage>,
    mut accepts: impl FnMut(&EngineAction) -> bool,
) -> Result<(), String> {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return Err(String::from("Timed out waiting for audio engine."));
        }
        let Some(message) = tokio::time::timeout(remaining, rx.recv())
            .await
            .map_err(|_| String::from("Timed out waiting for audio engine."))?
        else {
            return Err(String::from("Audio engine response channel closed."));
        };
        if let EngineMessage::Response(result) = message {
            match result {
                Ok(action) if accepts(&action) => return Ok(()),
                Ok(_) => {}
                Err(err) => return Err(err),
            }
        }
    }
}

pub(crate) fn preview_path(source: &Path) -> PathBuf {
    let mut path = std::env::temp_dir();
    let stem = source
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or("maolan-editor-preview");
    path.push(format!("maolan-editor-preview-{stem}.wav"));
    path
}
