#[cfg(feature = "standalone")]
use maolan_engine::client::Client as EngineClient;
use maolan_widgets::iced::Task;

use crate::devices::StartupSetup;
use crate::dialogs::{DetectMarkersDialog, ExportMarkersDialog, MarkerDialog, PreferencesDialog};
use crate::document::AudioDocument;
#[cfg(feature = "standalone")]
use crate::engine::{EnginePlayback, PluginFormat, scan_plugins_startup};
use crate::history::EditHistory;
use crate::message::Message;

#[derive(Debug, Default)]
pub struct EditApp {
    pub(crate) standalone_ready: bool,
    pub(crate) setup: StartupSetup,
    pub(crate) audio: Option<AudioDocument>,
    pub(crate) history: EditHistory,
    pub(crate) status: String,
    pub(crate) busy: bool,
    pub(crate) preparing_playback: bool,
    pub(crate) busy_progress: f32,
    pub(crate) playing: bool,
    pub(crate) playhead_samples: usize,
    pub(crate) selection_anchor_samples: Option<usize>,
    pub(crate) selection_samples: Option<(usize, usize)>,
    pub(crate) engine_clip_path: Option<std::path::PathBuf>,
    #[cfg(feature = "standalone")]
    pub(crate) engine_playback: Option<EnginePlayback>,
    pub(crate) close_window_id: Option<maolan_widgets::iced::window::Id>,
    pub(crate) marker_dialog: Option<MarkerDialog>,
    pub(crate) detect_markers_dialog: Option<DetectMarkersDialog>,
    pub(crate) export_markers_dialog: Option<ExportMarkersDialog>,
    pub(crate) preferences_dialog: Option<PreferencesDialog>,
    pub(crate) vst3_plugins_loaded: bool,
    pub(crate) vst3_plugins_unavailable: bool,
    pub(crate) clap_plugins_loaded: bool,
    pub(crate) clap_plugins_unavailable: bool,
    #[cfg(unix)]
    pub(crate) lv2_plugins_loaded: bool,
    #[cfg(unix)]
    pub(crate) lv2_plugins_unavailable: bool,
}

#[cfg(feature = "standalone")]
impl EditApp {
    pub(crate) fn plugins_loaded(&self) -> bool {
        let core = (self.vst3_plugins_loaded || self.vst3_plugins_unavailable)
            && (self.clap_plugins_loaded || self.clap_plugins_unavailable);
        #[cfg(unix)]
        {
            core && (self.lv2_plugins_loaded || self.lv2_plugins_unavailable)
        }
        #[cfg(not(unix))]
        {
            core
        }
    }
}

#[cfg(not(feature = "standalone"))]
impl EditApp {
    pub(crate) fn plugins_loaded(&self) -> bool {
        true
    }
}

#[cfg(feature = "standalone")]
pub fn new() -> (EditApp, Task<Message>) {
    let client = EngineClient::default();
    let scan_tasks = vec![
        Task::perform(
            scan_plugins_startup(client.clone(), PluginFormat::Vst3),
            |loaded| {
                if loaded {
                    Message::Vst3PluginsLoaded
                } else {
                    Message::Vst3PluginsUnavailable
                }
            },
        ),
        Task::perform(
            scan_plugins_startup(client.clone(), PluginFormat::Clap),
            |loaded| {
                if loaded {
                    Message::ClapPluginsLoaded
                } else {
                    Message::ClapPluginsUnavailable
                }
            },
        ),
        #[cfg(unix)]
        Task::perform(
            scan_plugins_startup(client.clone(), PluginFormat::Lv2),
            |loaded| {
                if loaded {
                    Message::Lv2PluginsLoaded
                } else {
                    Message::Lv2PluginsUnavailable
                }
            },
        ),
    ];
    (
        EditApp {
            status: String::from("Choose audio hardware and open the engine."),
            ..EditApp::default()
        },
        Task::batch(scan_tasks),
    )
}

#[cfg(not(feature = "standalone"))]
pub fn new() -> (EditApp, Task<Message>) {
    (
        EditApp {
            status: String::from("Open an audio file to view its waveform."),
            ..EditApp::default()
        },
        Task::none(),
    )
}

pub fn title(app: &EditApp) -> String {
    let base = app
        .audio
        .as_ref()
        .and_then(|audio| audio.source_path.file_name())
        .map(|name| format!("Maolan Editor - {}", name.to_string_lossy()))
        .unwrap_or_else(|| String::from("Maolan Editor"));
    if app.history.is_dirty() {
        format!("{base} *")
    } else {
        base
    }
}
