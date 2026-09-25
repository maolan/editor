use maolan_widgets::iced::{Task, window};

use crate::devices::{
    discover_input_audio_devices, discover_output_audio_devices, pick_bits, pick_period_frames,
    pick_sample_rate,
};
#[cfg(feature = "standalone")]
use crate::dialogs::ExportMarkersDialog;
use crate::dialogs::{
    DetectMarkersDialog, MarkerDialog, PreferencesDialog, close_confirmation_dialog,
};
use crate::document::{delete_sample_range, restore_document};
use crate::edits::audio_edit_status;
#[cfg(feature = "standalone")]
use crate::engine::open_standalone_engine;
use crate::history::{DocumentSnapshot, EditHistory};
#[cfg(feature = "standalone")]
use crate::io::{
    choose_export_directory, encode_format_for_path, export_marker_ranges, load_document,
    open_audio_dialog, save_audio_dialog, save_document,
};
use crate::io::{document_status, load_audio_buffer};
use crate::markers::detect_markers;
use crate::preferences::EditorPreferences;
use crate::transport::{
    play_standalone, prepare_document_track, refresh_standalone_playhead, reset_after_close,
    sample_at_ratio, selection_duration_seconds, stop_engine_playback,
};

pub use crate::devices::{AudioDeviceOption, AudioEngineOption};
pub use crate::dialogs::{ExportBitDepth, ExportFormat, ExportSampleRate};
pub use crate::document::AudioDocument;
pub use crate::edits::{
    AudioEditAction, AudioEdits, apply_audio_edit_action_to_samples, apply_audio_edit_actions,
    summarize_audio_edit_actions,
};
pub use crate::message::{AudioBuffer, Message, message_edits_document};
pub use crate::shortcuts::subscription;
pub use crate::state::{EditApp, new, title};
pub use crate::transport::{
    HostPreview, RenderedAudio, audio_edit_action_for_message, current_audio_edit_actions,
    current_audio_edits, host_preview, is_playing, open_audio, rendered_audio,
    set_embedded_transport, vu_levels_db,
};
pub use crate::view::{
    embedded_view, embedded_view_with_play_disabled, embedded_view_without_vu_meter, menu,
    standalone_menu, toolbar, toolbar_with_playhead, view,
};

pub fn update(app: &mut EditApp, message: Message) -> Task<Message> {
    match message {
        Message::None => Task::none(),
        Message::StartupBackendSelected(engine) => {
            app.setup.audio_engine = engine;
            app.setup.output_devices = discover_output_audio_devices(engine);
            app.setup.input_devices = discover_input_audio_devices(engine);
            app.setup.output_device = app.setup.output_devices.first().cloned();
            app.setup.input_device = app.setup.input_devices.first().cloned();
            app.setup.sample_rate_hz = pick_sample_rate(&app.setup);
            app.setup.bits = pick_bits(&app.setup);
            app.setup.period_frames = pick_period_frames(&app.setup);
            Task::none()
        }
        Message::StartupOutputDeviceSelected(device) => {
            app.setup.output_device = Some(device);
            app.setup.sample_rate_hz = pick_sample_rate(&app.setup);
            app.setup.bits = pick_bits(&app.setup);
            app.setup.period_frames = pick_period_frames(&app.setup);
            Task::none()
        }
        Message::StartupInputDeviceSelected(device) => {
            app.setup.input_device = Some(device);
            Task::none()
        }
        Message::StartupSampleRateSelected(rate) => {
            app.setup.sample_rate_hz = rate;
            Task::none()
        }
        Message::StartupBitsSelected(bits) => {
            app.setup.bits = bits;
            app.setup.period_frames = pick_period_frames(&app.setup);
            Task::none()
        }
        Message::StartupPeriodFramesSelected(period_frames) => {
            app.setup.period_frames = period_frames;
            Task::none()
        }
        Message::StartupNPeriodsSelected(nperiods) => {
            app.setup.nperiods = nperiods;
            Task::none()
        }
        Message::StartupExclusiveToggled(exclusive) => {
            app.setup.exclusive = exclusive;
            Task::none()
        }
        Message::StartupSyncModeToggled(sync_mode) => {
            app.setup.sync_mode = sync_mode;
            Task::none()
        }
        Message::StartupOpen => {
            app.busy = true;
            app.busy_progress = 0.0;
            #[cfg(feature = "standalone")]
            {
                app.status = String::from("Scanning plugins and opening audio device...");
                let setup = app.setup.clone();
                Task::perform(open_standalone_engine(setup), Message::StartupOpened)
            }
            #[cfg(not(feature = "standalone"))]
            {
                app.status = String::from("Open an audio file to view its waveform.");
                Task::perform(async { Ok(()) }, Message::StartupOpened)
            }
        }
        Message::StartupOpened(Ok(client)) => {
            app.busy = false;
            app.busy_progress = 1.0;
            app.standalone_ready = true;
            #[cfg(feature = "standalone")]
            {
                app.engine_playback = Some(crate::engine::EnginePlayback { client });
            }
            #[cfg(not(feature = "standalone"))]
            {
                let _unused = &client;
            }
            app.status = String::from("Open an audio file to view its waveform.");
            Task::none()
        }
        Message::StartupOpened(Err(err)) => {
            app.busy = false;
            app.busy_progress = 0.0;
            app.status = err;
            Task::none()
        }
        Message::Vst3PluginsLoaded => {
            app.vst3_plugins_loaded = true;
            Task::none()
        }
        Message::Vst3PluginsUnavailable => {
            app.vst3_plugins_unavailable = true;
            Task::none()
        }
        Message::ClapPluginsLoaded => {
            app.clap_plugins_loaded = true;
            Task::none()
        }
        Message::ClapPluginsUnavailable => {
            app.clap_plugins_unavailable = true;
            Task::none()
        }
        #[cfg(unix)]
        Message::Lv2PluginsLoaded => {
            app.lv2_plugins_loaded = true;
            Task::none()
        }
        #[cfg(unix)]
        Message::Lv2PluginsUnavailable => {
            app.lv2_plugins_unavailable = true;
            Task::none()
        }
        Message::Open => {
            #[cfg(feature = "standalone")]
            {
                app.busy = true;
                app.busy_progress = 0.0;
                app.status = String::from("Opening audio file...");
                Task::perform(open_audio_dialog(), Message::FileOpened)
            }
            #[cfg(not(feature = "standalone"))]
            {
                app.status = String::from("File loading is handled by the embedding host.");
                Task::none()
            }
        }
        Message::Close => {
            reset_after_close(app);
            Task::none()
        }
        Message::OpenPath(path) => {
            #[cfg(feature = "standalone")]
            {
                load_document(app, path, None, None)
            }
            #[cfg(not(feature = "standalone"))]
            {
                let _ = path;
                app.status =
                    String::from("File loading is only available in standalone editor builds.");
                Task::none()
            }
        }
        Message::OpenAudio(audio) => load_audio_buffer(app, audio, None),
        Message::OpenClip {
            path,
            offset,
            length,
            timeline_start,
        } => {
            #[cfg(feature = "standalone")]
            {
                load_document(
                    app,
                    path,
                    Some(crate::edits::AudioRegion { offset, length }),
                    timeline_start.map(|offset| crate::edits::AudioRegion { offset, length }),
                )
            }
            #[cfg(not(feature = "standalone"))]
            {
                let _ = (path, offset, length, timeline_start);
                app.status =
                    String::from("File loading is only available in standalone editor builds.");
                Task::none()
            }
        }
        Message::Save => {
            #[cfg(feature = "standalone")]
            {
                if let Some(audio) = app.audio.as_ref() {
                    if let Some(path) = audio.save_path.clone() {
                        if encode_format_for_path(&path).is_ok() {
                            app.busy = true;
                            app.busy_progress = 0.0;
                            app.status = format!("Saving {}...", path.display());
                            let samples = audio.rendered_save_samples();
                            let channels = audio.channels;
                            let sample_rate = audio.sample_rate;
                            Task::perform(
                                save_document(path, samples, channels, sample_rate),
                                Message::DocumentSaved,
                            )
                        } else {
                            app.busy = true;
                            app.busy_progress = 0.0;
                            app.status = String::from("Choose a Maolan export format to save.");
                            Task::perform(save_audio_dialog(Some(path)), Message::FileSaved)
                        }
                    } else {
                        app.busy = true;
                        app.busy_progress = 0.0;
                        app.status = String::from("Choose where to save this clip.");
                        Task::perform(
                            save_audio_dialog(Some(audio.source_path.clone())),
                            Message::FileSaved,
                        )
                    }
                } else {
                    app.status = String::from("No audio file is open.");
                    Task::none()
                }
            }
            #[cfg(not(feature = "standalone"))]
            {
                app.status = String::from("Saving is handled by the embedding host.");
                Task::none()
            }
        }
        Message::SaveAs => {
            #[cfg(feature = "standalone")]
            {
                if let Some(audio) = app.audio.as_ref() {
                    app.busy = true;
                    app.busy_progress = 0.0;
                    app.status = String::from("Choosing save destination...");
                    Task::perform(
                        save_audio_dialog(Some(audio.source_path.clone())),
                        Message::FileSaved,
                    )
                } else {
                    app.status = String::from("No audio file is open.");
                    Task::none()
                }
            }
            #[cfg(not(feature = "standalone"))]
            {
                app.status = String::from("Saving is handled by the embedding host.");
                Task::none()
            }
        }
        Message::Play => play_standalone(app),
        Message::TogglePlayback => {
            if app.playing {
                update(app, Message::Stop)
            } else {
                update(app, Message::Play)
            }
        }
        Message::Stop => {
            app.playing = false;
            app.status = String::from("Stopped.");
            stop_engine_playback(app)
        }
        Message::RewindToStart => {
            app.playhead_samples = 0;
            Task::none()
        }
        Message::GoToEnd => {
            app.playhead_samples = app.audio.as_ref().map(AudioDocument::frames).unwrap_or(0);
            Task::none()
        }
        Message::JumpToNextZeroCrossing => {
            let Some(audio) = app.audio.as_ref() else {
                app.status = String::from("No audio file is open.");
                return Task::none();
            };
            let start_frame = app.playhead_samples.min(audio.frames());
            match audio.next_zero_crossing_frame(start_frame) {
                Some(frame) => {
                    app.playhead_samples = frame;
                    app.status = format!("Jumped to zero crossing at frame {frame}.");
                }
                None => {
                    app.status = String::from("No zero crossing found after playhead.");
                }
            }
            Task::none()
        }
        Message::PlaybackTick => {
            if refresh_standalone_playhead(app) {
                update(app, Message::Stop)
            } else {
                Task::none()
            }
        }
        Message::SelectionStart(ratio) => {
            if let Some(sample) = sample_at_ratio(app, ratio) {
                app.selection_anchor_samples = Some(sample);
                app.selection_samples = Some((sample, sample));
            }
            Task::none()
        }
        Message::SelectionDrag(ratio) => {
            if let (Some(anchor), Some(sample)) =
                (app.selection_anchor_samples, sample_at_ratio(app, ratio))
            {
                app.selection_samples = Some((anchor.min(sample), anchor.max(sample)));
            }
            Task::none()
        }
        Message::SelectionFinish(ratio) => {
            if let (Some(anchor), Some(sample)) = (
                app.selection_anchor_samples.take(),
                sample_at_ratio(app, ratio),
            ) {
                let start = anchor.min(sample);
                let end = anchor.max(sample);
                app.selection_samples = (end > start).then_some((start, end));
                if let Some((start, end)) = app.selection_samples {
                    app.status = format!(
                        "Selected {}..{} samples ({:.3} s).",
                        start,
                        end,
                        selection_duration_seconds(app)
                    );
                }
            }
            Task::none()
        }
        Message::SelectionResize(ratio) => {
            let Some((start, end)) = app.selection_samples else {
                return Task::none();
            };
            let Some(click_sample) = sample_at_ratio(app, ratio) else {
                return Task::none();
            };
            let (new_start, new_end) = if click_sample <= start {
                (click_sample, end)
            } else if click_sample >= end {
                (start, click_sample)
            } else if click_sample - start < end - click_sample {
                (click_sample, end)
            } else {
                (start, click_sample)
            };
            app.selection_anchor_samples = None;
            app.selection_samples = Some((new_start, new_end));
            app.status = format!(
                "Selected {}..{} samples ({:.3} s).",
                new_start,
                new_end,
                selection_duration_seconds(app)
            );
            Task::none()
        }
        Message::PlayheadMoved(ratio) => {
            if let Some(sample) = sample_at_ratio(app, ratio) {
                app.playhead_samples = sample;
            }
            Task::none()
        }
        Message::SelectMarkerRegion(ratio) => {
            let Some(audio) = app.audio.as_ref() else {
                return Task::none();
            };
            if audio.markers.is_empty() {
                app.status = String::from("No markers to select between.");
                return Task::none();
            }
            let frames = audio.frames();
            let click_sample = (ratio.clamp(0.0, 1.0) * frames as f32).round() as usize;
            let mut sorted = audio.markers.clone();
            sorted.sort_unstable_by_key(|(sample, _)| *sample);

            let (start, end) =
                if let Some((next, _)) = sorted.iter().find(|(s, _)| *s > click_sample) {
                    let prev = sorted
                        .iter()
                        .filter(|(s, _)| *s < click_sample)
                        .map(|(s, _)| *s)
                        .next_back()
                        .unwrap_or(0);
                    (prev, *next)
                } else {
                    let last = sorted.last().map(|(s, _)| *s).unwrap_or(0);
                    (last, frames)
                };

            app.selection_anchor_samples = None;
            app.selection_samples = Some((start, end));
            app.status = format!(
                "Selected region {}..{} samples ({:.3} s).",
                start,
                end,
                selection_duration_seconds(app)
            );
            Task::none()
        }
        Message::StandalonePlaybackStarted(Ok(())) => Task::none(),
        Message::StandalonePlaybackStarted(Err(err)) => {
            app.playing = false;
            app.status = err;
            Task::none()
        }
        Message::StandalonePlaybackStopped(Ok(())) => {
            app.playing = false;
            app.status = String::from("Stopped.");
            Task::none()
        }
        Message::StandalonePlaybackStopped(Err(err)) => {
            app.status = err;
            Task::none()
        }
        Message::FadeIn
        | Message::FadeOut
        | Message::IncreaseVolume
        | Message::DecreaseVolume
        | Message::Reverse => dispatch_audio_edit(app, message.clone()),
        Message::EditAction(action) => apply_standalone_audio_edit_action(app, action),
        Message::Undo => {
            let Some(audio) = app.audio.as_mut() else {
                app.status = String::from("No audio file is open.");
                return Task::none();
            };
            match app.history.undo() {
                Some(snapshot) => {
                    restore_document(audio, snapshot);
                    audio.rebuild_preview();
                    app.status = String::from("Undone.");
                    prepare_document_track(app)
                }
                None => {
                    app.status = String::from("Nothing to undo.");
                    Task::none()
                }
            }
        }
        Message::Redo => {
            let Some(audio) = app.audio.as_mut() else {
                app.status = String::from("No audio file is open.");
                return Task::none();
            };
            match app.history.redo() {
                Some(snapshot) => {
                    restore_document(audio, snapshot);
                    audio.rebuild_preview();
                    app.status = String::from("Redone.");
                    prepare_document_track(app)
                }
                None => {
                    app.status = String::from("Nothing to redo.");
                    Task::none()
                }
            }
        }
        Message::DeleteSelection => delete_selection(app),
        Message::FileOpened(Some(path)) => {
            #[cfg(feature = "standalone")]
            {
                load_document(app, path, None, None)
            }
            #[cfg(not(feature = "standalone"))]
            {
                let _ = path;
                app.busy = false;
                app.busy_progress = 0.0;
                app.status =
                    String::from("File loading is only available in standalone editor builds.");
                Task::none()
            }
        }
        Message::FileOpened(None) => {
            app.busy = false;
            app.busy_progress = 0.0;
            app.status = String::from("Open cancelled.");
            Task::none()
        }
        Message::DocumentLoadProgress { progress, status } => {
            app.busy = true;
            app.busy_progress = progress.clamp(0.0, 1.0);
            app.status = status;
            Task::none()
        }
        Message::DocumentLoaded(Ok(audio)) => {
            app.busy = false;
            app.busy_progress = 1.0;
            app.status = document_status(&audio);
            app.playing = false;
            app.playhead_samples = 0;
            app.engine_clip_path = None;
            app.selection_anchor_samples = None;
            app.selection_samples = None;
            app.history = EditHistory::new(DocumentSnapshot {
                samples: audio.samples.clone(),
                edits: audio.edits,
                edit_actions: audio.edit_actions.clone(),
                markers: audio.markers.clone(),
            });
            app.audio = Some(audio);
            prepare_document_track(app)
        }
        Message::DocumentLoaded(Err(err)) => {
            app.busy = false;
            app.busy_progress = 0.0;
            app.status = err;
            Task::none()
        }
        Message::EngineDocumentPrepared(Ok(())) => {
            app.preparing_playback = false;
            if let Some(audio) = app.audio.as_ref() {
                app.status = document_status(audio);
            }
            Task::none()
        }
        Message::EngineDocumentPrepared(Err(err)) => {
            app.preparing_playback = false;
            app.status = err;
            Task::none()
        }
        Message::FileSaved(Some(path)) => {
            #[cfg(feature = "standalone")]
            {
                let Some(audio) = app.audio.as_ref() else {
                    app.busy = false;
                    app.busy_progress = 0.0;
                    app.status = String::from("No audio file is open.");
                    return Task::none();
                };
                app.busy = true;
                app.busy_progress = 0.0;
                app.status = format!("Saving {}...", path.display());
                let samples = audio.rendered_save_samples();
                let channels = audio.channels;
                let sample_rate = audio.sample_rate;
                Task::perform(
                    save_document(path, samples, channels, sample_rate),
                    Message::DocumentSaved,
                )
            }
            #[cfg(not(feature = "standalone"))]
            {
                let _ = path;
                app.busy = false;
                app.busy_progress = 0.0;
                app.status = String::from("Saving is handled by the embedding host.");
                Task::none()
            }
        }
        Message::FileSaved(None) => {
            app.busy = false;
            app.busy_progress = 0.0;
            app.close_window_id = None;
            app.status = String::from("Save cancelled.");
            Task::none()
        }
        Message::DocumentSaved(Ok(path)) => {
            app.busy = false;
            app.busy_progress = 1.0;
            if let Some(audio) = app.audio.as_mut() {
                audio.save_path = Some(path.clone());
            }
            app.history.mark_saved();
            app.status = format!("Saved {}.", path.display());
            if let Some(window_id) = app.close_window_id.take() {
                window::close(window_id)
            } else {
                Task::none()
            }
        }
        Message::DocumentSaved(Err(err)) => {
            app.busy = false;
            app.busy_progress = 0.0;
            app.close_window_id = None;
            app.status = err;
            Task::none()
        }
        Message::WindowCloseRequested(window_id) => {
            if app.history.is_dirty() {
                app.status = String::from("Unsaved changes. Save, discard, or cancel?");
                Task::perform(close_confirmation_dialog(), move |result| {
                    Message::CloseDialogResult(window_id, result)
                })
            } else {
                window::close(window_id)
            }
        }
        Message::CloseDialogResult(window_id, result) => match result {
            rfd::MessageDialogResult::Yes => {
                app.close_window_id = Some(window_id);
                update(app, Message::Save)
            }
            rfd::MessageDialogResult::No => window::close(window_id),
            _ => {
                app.close_window_id = None;
                app.status = String::from("Close cancelled.");
                Task::none()
            }
        },
        Message::MarkerCreateDialog { sample } => {
            app.marker_dialog = Some(MarkerDialog {
                sample,
                name: String::new(),
            });
            maolan_widgets::iced::widget::operation::focus(crate::dialogs::marker_name_input_id())
        }
        Message::MarkerNameInput(name) => {
            if let Some(dialog) = app.marker_dialog.as_mut() {
                dialog.name = name;
            }
            Task::none()
        }
        Message::MarkerNameConfirm => {
            let Some(dialog) = app.marker_dialog.take() else {
                return Task::none();
            };
            let name = dialog.name.trim().to_string();
            if name.is_empty() {
                return Task::none();
            }
            let Some(audio) = app.audio.as_mut() else {
                return Task::none();
            };
            audio.markers.push((dialog.sample, name));
            audio.markers.sort_unstable_by_key(|(sample, _)| *sample);
            audio.markers.dedup_by(|a, b| a.0 == b.0 && a.1 == b.1);
            app.status = format!("Marker added at sample {}.", dialog.sample);
            Task::none()
        }
        Message::MarkerNameCancel => {
            app.marker_dialog = None;
            app.detect_markers_dialog = None;
            app.export_markers_dialog = None;
            app.preferences_dialog = None;
            Task::none()
        }
        Message::MarkerDelete { sample } => {
            let Some(audio) = app.audio.as_mut() else {
                return Task::none();
            };
            let before = audio.markers.len();
            audio
                .markers
                .retain(|(marker_sample, _)| *marker_sample != sample);
            if audio.markers.len() < before {
                app.status = format!("Marker at sample {sample} deleted.");
            } else {
                app.status = String::from("No marker at that position.");
            }
            Task::none()
        }
        Message::DetectMarkersDialog => {
            app.detect_markers_dialog = Some(DetectMarkersDialog::default());
            Task::none()
        }
        Message::DetectMarkersThresholdInput(value) => {
            if let Some(dialog) = app.detect_markers_dialog.as_mut() {
                dialog.threshold_db = value;
            }
            Task::none()
        }
        Message::DetectMarkersSilenceSamplesInput(value) => {
            if let Some(dialog) = app.detect_markers_dialog.as_mut() {
                dialog.silence_samples = value;
            }
            Task::none()
        }
        Message::DetectMarkersConfirm => {
            let Some(dialog) = app.detect_markers_dialog.take() else {
                return Task::none();
            };
            let Some(audio) = app.audio.as_mut() else {
                app.status = String::from("No audio file is open.");
                return Task::none();
            };
            let Ok(threshold_db) = dialog.threshold_db.trim().parse::<f32>() else {
                app.status = String::from("Invalid threshold value.");
                return Task::none();
            };
            let Ok(silence_samples) = dialog.silence_samples.trim().parse::<usize>() else {
                app.status = String::from("Invalid silence sample count.");
                return Task::none();
            };
            if silence_samples == 0 {
                app.status = String::from("Silence sample count must be greater than zero.");
                return Task::none();
            }
            let detected = detect_markers(
                &audio.preview_samples,
                audio.channels,
                threshold_db,
                silence_samples,
            );
            let added = detected.len();
            audio.markers.extend(detected);
            audio.markers.sort_unstable_by_key(|(sample, _)| *sample);
            audio.markers.dedup_by(|a, b| a.0 == b.0 && a.1 == b.1);
            app.status = format!("Detected {added} marker positions.");
            Task::none()
        }
        Message::DetectMarkersCancel => {
            app.detect_markers_dialog = None;
            Task::none()
        }
        Message::ExportMarkersDialog => {
            #[cfg(feature = "standalone")]
            {
                let task = if app.export_markers_dialog.is_some() {
                    Task::perform(
                        choose_export_directory(),
                        Message::ExportMarkersDirectorySelected,
                    )
                } else {
                    Task::none()
                };
                app.export_markers_dialog = Some(ExportMarkersDialog::default());
                task
            }
            #[cfg(not(feature = "standalone"))]
            {
                app.status =
                    String::from("Marker export is only available in standalone editor builds.");
                Task::none()
            }
        }
        Message::ExportMarkersDirectorySelected(directory) => {
            if let Some(dialog) = app.export_markers_dialog.as_mut() {
                dialog.directory = directory;
            }
            Task::none()
        }
        Message::ExportMarkersFormatSelected(format) => {
            if let Some(dialog) = app.export_markers_dialog.as_mut() {
                dialog.format = format;
            }
            Task::none()
        }
        Message::ExportMarkersBitDepthSelected(bit_depth) => {
            if let Some(dialog) = app.export_markers_dialog.as_mut() {
                dialog.bit_depth = bit_depth;
            }
            Task::none()
        }
        Message::ExportMarkersSampleRateSelected(sample_rate) => {
            if let Some(dialog) = app.export_markers_dialog.as_mut() {
                dialog.sample_rate = sample_rate;
            }
            Task::none()
        }
        Message::ExportMarkersConfirm => {
            #[cfg(feature = "standalone")]
            {
                let Some(dialog) = app.export_markers_dialog.take() else {
                    return Task::none();
                };
                let Some(audio) = app.audio.as_ref() else {
                    app.status = String::from("No audio file is open.");
                    return Task::none();
                };
                let Some(directory) = dialog.directory else {
                    app.status = String::from("Choose an export directory.");
                    return Task::none();
                };
                if audio.markers.is_empty() {
                    app.status = String::from("No markers to export between.");
                    return Task::none();
                }
                app.busy = true;
                app.busy_progress = 0.0;
                app.status = String::from("Exporting marker ranges...");
                let audio_clone = audio.clone();
                Task::perform(
                    export_marker_ranges(
                        directory,
                        audio_clone,
                        dialog.format,
                        dialog.bit_depth,
                        dialog.sample_rate.value(),
                    ),
                    Message::ExportMarkersFinished,
                )
            }
            #[cfg(not(feature = "standalone"))]
            {
                app.export_markers_dialog = None;
                app.status =
                    String::from("Marker export is only available in standalone editor builds.");
                Task::none()
            }
        }
        Message::ExportMarkersCancel => {
            app.export_markers_dialog = None;
            Task::none()
        }
        Message::ExportMarkersFinished(result) => {
            app.busy = false;
            app.busy_progress = 1.0;
            match result {
                Ok(count) => app.status = format!("Exported {count} marker range(s)."),
                Err(err) => app.status = err,
            }
            Task::none()
        }
        Message::PreferencesDialog => {
            app.preferences_dialog = Some(PreferencesDialog::from_setup(&app.setup));
            Task::none()
        }
        Message::PreferencesOutputDeviceSelected(device) => {
            if let Some(dialog) = app.preferences_dialog.as_mut() {
                dialog.output_device = Some(device);
            }
            Task::none()
        }
        Message::PreferencesInputDeviceSelected(device) => {
            if let Some(dialog) = app.preferences_dialog.as_mut() {
                dialog.input_device = Some(device);
            }
            Task::none()
        }
        Message::PreferencesSave => {
            let Some(dialog) = app.preferences_dialog.take() else {
                return Task::none();
            };
            app.setup.output_device = dialog.output_device.clone();
            app.setup.input_device = dialog.input_device.clone();
            let preferences = EditorPreferences {
                default_output_device_id: dialog.output_device.map(|device| device.id),
                default_input_device_id: dialog.input_device.map(|device| device.id),
            };
            match preferences.save() {
                Ok(()) => app.status = String::from("Preferences saved."),
                Err(err) => app.status = format!("Failed to save preferences: {err}"),
            }
            Task::none()
        }
        Message::PreferencesCancel => {
            app.preferences_dialog = None;
            Task::none()
        }
    }
}

fn dispatch_audio_edit(app: &mut EditApp, message: Message) -> Task<Message> {
    match audio_edit_action_for_message(app, &message) {
        Some(action) => apply_standalone_audio_edit_action(app, action),
        None => {
            app.status = String::from("No audio file is open.");
            Task::none()
        }
    }
}

fn apply_standalone_audio_edit_action(app: &mut EditApp, action: AudioEditAction) -> Task<Message> {
    let Some(audio) = app.audio.as_mut() else {
        app.status = String::from("No audio file is open.");
        return Task::none();
    };

    if action.is_empty_for_frames(audio.frames()) {
        app.status = String::from("Selection is empty.");
        return Task::none();
    }

    let previous_snapshot = DocumentSnapshot {
        samples: audio.samples.clone(),
        edits: audio.edits,
        edit_actions: audio.edit_actions.clone(),
        markers: audio.markers.clone(),
    };

    match action {
        AudioEditAction::Delete {
            start_sample,
            length_samples,
        } => {
            delete_sample_range(audio, start_sample, length_samples);
            app.selection_anchor_samples = None;
            app.selection_samples = None;
            app.status = format!(
                "Deleted {}..{} samples.",
                start_sample,
                start_sample.saturating_add(length_samples)
            );
        }
        _ => {
            audio.edit_actions.push(action);
            audio.edits = audio.edit_summary();
            app.status = audio_edit_status(action, audio.edits, audio.frames());
        }
    }

    app.history.record(
        previous_snapshot,
        DocumentSnapshot {
            samples: audio.samples.clone(),
            edits: audio.edits,
            edit_actions: audio.edit_actions.clone(),
            markers: audio.markers.clone(),
        },
    );
    audio.rebuild_preview();
    prepare_document_track(app)
}

fn delete_selection(app: &mut EditApp) -> Task<Message> {
    match audio_edit_action_for_message(app, &Message::DeleteSelection) {
        Some(action) => apply_standalone_audio_edit_action(app, action),
        None => Task::none(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::document::test_document;

    fn app_with_markers(markers: Vec<(usize, String)>, frames: usize) -> EditApp {
        let mut audio = test_document(vec![0.0f32; frames], 1);
        audio.markers = markers;
        EditApp {
            standalone_ready: true,
            audio: Some(audio),
            ..EditApp::default()
        }
    }

    #[test]
    fn playhead_moved_sets_playhead_from_ratio() {
        let mut app = app_with_markers(Vec::new(), 100);
        let _ = update(&mut app, Message::PlayheadMoved(0.25));
        assert_eq!(app.playhead_samples, 25);
    }

    #[test]
    fn embedded_play_without_engine_starts_preview_playback() {
        let mut app = EditApp {
            standalone_ready: false,
            audio: Some(test_document(vec![0.0f32; 100], 1)),
            ..EditApp::default()
        };

        let _ = update(&mut app, Message::Play);

        assert!(app.playing);
        assert_eq!(app.status, "Playing preview.");
    }

    #[test]
    fn select_marker_region_selects_between_markers() {
        let mut app = app_with_markers(vec![(20, "A".to_string()), (60, "B".to_string())], 100);
        let _ = update(&mut app, Message::SelectMarkerRegion(0.4));
        assert_eq!(app.selection_samples, Some((20, 60)));
    }

    #[test]
    fn select_marker_region_selects_start_to_first_marker() {
        let mut app = app_with_markers(vec![(50, "A".to_string())], 100);
        let _ = update(&mut app, Message::SelectMarkerRegion(0.25));
        assert_eq!(app.selection_samples, Some((0, 50)));
    }

    #[test]
    fn select_marker_region_selects_last_marker_to_end() {
        let mut app = app_with_markers(vec![(50, "A".to_string())], 100);
        let _ = update(&mut app, Message::SelectMarkerRegion(0.75));
        assert_eq!(app.selection_samples, Some((50, 100)));
    }

    #[test]
    fn detect_markers_confirm_adds_detected_markers() {
        let mut samples = vec![0.0f32; 10];
        samples.extend(vec![0.8f32; 10]);
        samples.extend(vec![0.0f32; 10]);
        let audio = test_document(samples, 1);
        let mut app = EditApp {
            standalone_ready: true,
            audio: Some(audio),
            detect_markers_dialog: Some(DetectMarkersDialog {
                threshold_db: String::from("-60.0"),
                silence_samples: String::from("5"),
            }),
            ..EditApp::default()
        };
        let _ = update(&mut app, Message::DetectMarkersConfirm);
        assert!(app.detect_markers_dialog.is_none());
        assert_eq!(
            app.audio.as_ref().unwrap().markers,
            vec![(10, "Region 1".to_string()), (20, "Region 2".to_string()),]
        );
    }

    #[test]
    fn detect_markers_confirm_rejects_invalid_input() {
        let audio = test_document(vec![0.8f32; 10], 1);
        let mut app = EditApp {
            standalone_ready: true,
            audio: Some(audio),
            detect_markers_dialog: Some(DetectMarkersDialog {
                threshold_db: String::from("not a number"),
                silence_samples: String::from("5"),
            }),
            ..EditApp::default()
        };
        let _ = update(&mut app, Message::DetectMarkersConfirm);
        assert!(app.audio.as_ref().unwrap().markers.is_empty());
        assert!(app.status.contains("Invalid"));
    }

    #[test]
    fn detect_markers_cancel_closes_dialog() {
        let mut app = EditApp {
            detect_markers_dialog: Some(DetectMarkersDialog::default()),
            ..EditApp::default()
        };
        let _ = update(&mut app, Message::DetectMarkersCancel);
        assert!(app.detect_markers_dialog.is_none());
    }

    #[test]
    fn selection_resize_moves_start_when_clicked_before_range() {
        let mut app = app_with_markers(Vec::new(), 100);
        app.selection_samples = Some((40, 80));
        let _ = update(&mut app, Message::SelectionResize(0.1));
        assert_eq!(app.selection_samples, Some((10, 80)));
    }

    #[test]
    fn selection_resize_moves_end_when_clicked_after_range() {
        let mut app = app_with_markers(Vec::new(), 100);
        app.selection_samples = Some((40, 80));
        let _ = update(&mut app, Message::SelectionResize(0.95));
        assert_eq!(app.selection_samples, Some((40, 95)));
    }

    #[test]
    fn selection_resize_moves_nearest_edge_when_clicked_inside_range() {
        let mut app = app_with_markers(Vec::new(), 100);
        app.selection_samples = Some((20, 80));
        let _ = update(&mut app, Message::SelectionResize(0.3));
        assert_eq!(app.selection_samples, Some((30, 80)));

        let _ = update(&mut app, Message::SelectionResize(0.7));
        assert_eq!(app.selection_samples, Some((30, 70)));
    }

    #[test]
    fn selection_resize_ignored_when_no_selection() {
        let mut app = app_with_markers(Vec::new(), 100);
        let _ = update(&mut app, Message::SelectionResize(0.5));
        assert_eq!(app.selection_samples, None);
    }

    #[test]
    fn delete_selection_removes_range_and_shifts_markers() {
        let mut app = app_with_markers(vec![(1, "A".to_string()), (8, "B".to_string())], 10);
        let audio = app.audio.as_mut().unwrap();
        audio.samples = (0..10).map(|i| i as f32).collect();
        audio.rebuild_preview();
        app.selection_samples = Some((3, 6));
        let _ = update(&mut app, Message::DeleteSelection);
        let audio = app.audio.as_ref().unwrap();
        assert_eq!(audio.frames(), 7);
        assert_eq!(audio.samples, vec![0.0, 1.0, 2.0, 6.0, 7.0, 8.0, 9.0]);
        assert_eq!(
            audio.markers,
            vec![(1, "A".to_string()), (5, "B".to_string())]
        );
        assert!(app.selection_samples.is_none());
        assert!(app.history.is_dirty());
    }

    #[test]
    fn delete_selection_ignored_when_nothing_selected() {
        let mut app = app_with_markers(vec![(1, "A".to_string())], 5);
        let audio = app.audio.as_mut().unwrap();
        audio.samples = vec![0.0, 1.0, 2.0, 3.0, 4.0];
        audio.rebuild_preview();
        let original = audio.samples.clone();
        let _ = update(&mut app, Message::DeleteSelection);
        let audio = app.audio.as_ref().unwrap();
        assert_eq!(audio.samples, original);
        assert_eq!(audio.markers, vec![(1, "A".to_string())]);
        assert!(!app.history.is_dirty());
    }

    #[test]
    fn delete_selection_undo_restores_document() {
        let mut app = app_with_markers(vec![(1, "A".to_string()), (8, "B".to_string())], 10);
        let audio = app.audio.as_mut().unwrap();
        audio.samples = (0..10).map(|i| i as f32).collect();
        audio.rebuild_preview();
        let original_samples = audio.samples.clone();
        let original_markers = audio.markers.clone();
        app.selection_samples = Some((3, 6));
        let _ = update(&mut app, Message::DeleteSelection);
        assert!(app.history.is_dirty());
        let _ = update(&mut app, Message::Undo);
        let audio = app.audio.as_ref().unwrap();
        assert_eq!(audio.samples, original_samples);
        assert_eq!(audio.markers, original_markers);
        assert!(!app.history.is_dirty());
    }

    #[test]
    fn preferences_dialog_opens_from_setup() {
        let mut app = app_with_markers(Vec::new(), 100);
        app.setup.output_devices = vec![AudioDeviceOption::with_supported_caps(
            "out1",
            "Out One",
            vec![32],
            vec![48_000],
        )];
        app.setup.input_devices = vec![AudioDeviceOption::with_supported_caps(
            "in1",
            "In One",
            vec![32],
            vec![48_000],
        )];
        app.setup.output_device = Some(app.setup.output_devices[0].clone());
        app.setup.input_device = Some(app.setup.input_devices[0].clone());
        let _ = update(&mut app, Message::PreferencesDialog);
        let dialog = app.preferences_dialog.as_ref().unwrap();
        assert_eq!(dialog.output_devices.len(), 1);
        assert_eq!(dialog.input_devices.len(), 1);
        assert_eq!(dialog.output_device.as_ref().unwrap().id, "out1");
        assert_eq!(dialog.input_device.as_ref().unwrap().id, "in1");
    }

    #[test]
    fn preferences_save_updates_setup() {
        let mut app = app_with_markers(Vec::new(), 100);
        let out_devices = vec![
            AudioDeviceOption::with_supported_caps("out1", "Out One", vec![32], vec![48_000]),
            AudioDeviceOption::with_supported_caps("out2", "Out Two", vec![32], vec![48_000]),
        ];
        let in_devices = vec![
            AudioDeviceOption::with_supported_caps("in1", "In One", vec![32], vec![48_000]),
            AudioDeviceOption::with_supported_caps("in2", "In Two", vec![32], vec![48_000]),
        ];
        app.setup.output_devices = out_devices.clone();
        app.setup.input_devices = in_devices.clone();
        app.preferences_dialog = Some(PreferencesDialog::from_setup(&app.setup));
        let _ = update(
            &mut app,
            Message::PreferencesOutputDeviceSelected(out_devices[1].clone()),
        );
        let _ = update(
            &mut app,
            Message::PreferencesInputDeviceSelected(in_devices[1].clone()),
        );
        let _ = update(&mut app, Message::PreferencesSave);
        assert!(app.preferences_dialog.is_none());
        assert_eq!(app.setup.output_device.as_ref().unwrap().id, "out2");
        assert_eq!(app.setup.input_device.as_ref().unwrap().id, "in2");
    }

    #[test]
    fn preferences_cancel_closes_dialog() {
        let mut app = EditApp {
            preferences_dialog: Some(PreferencesDialog::from_setup(
                &crate::devices::StartupSetup::with_preferences(
                    &EditorPreferences::default(),
                    Vec::new(),
                    Vec::new(),
                ),
            )),
            ..EditApp::default()
        };
        let _ = update(&mut app, Message::PreferencesCancel);
        assert!(app.preferences_dialog.is_none());
    }
}
