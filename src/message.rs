use std::path::PathBuf;
use std::sync::Arc;

use maolan_widgets::iced::window;

use crate::devices::{AudioDeviceOption, AudioEngineOption};
use crate::dialogs::{ExportBitDepth, ExportFormat, ExportSampleRate};
use crate::document::AudioDocument;
use crate::edits::AudioEditAction;

#[cfg(feature = "standalone")]
pub(crate) type StandaloneOpenResult = maolan_engine::client::Client;
#[cfg(not(feature = "standalone"))]
pub(crate) type StandaloneOpenResult = ();

#[derive(Debug, Clone)]
pub enum Message {
    None,
    StartupBackendSelected(AudioEngineOption),
    StartupOutputDeviceSelected(AudioDeviceOption),
    StartupInputDeviceSelected(AudioDeviceOption),
    StartupSampleRateSelected(i32),
    StartupBitsSelected(usize),
    StartupPeriodFramesSelected(usize),
    StartupNPeriodsSelected(usize),
    StartupExclusiveToggled(bool),
    StartupSyncModeToggled(bool),
    StartupOpen,
    StartupOpened(Result<StandaloneOpenResult, String>),
    Vst3PluginsLoaded,
    Vst3PluginsUnavailable,
    ClapPluginsLoaded,
    ClapPluginsUnavailable,
    #[cfg(unix)]
    Lv2PluginsLoaded,
    #[cfg(unix)]
    Lv2PluginsUnavailable,
    Open,
    Close,
    Save,
    SaveAs,
    Play,
    Stop,
    TogglePlayback,
    RewindToStart,
    GoToEnd,
    JumpToNextZeroCrossing,
    PlaybackTick,
    SelectionStart(f32),
    SelectionDrag(f32),
    SelectionFinish(f32),
    SelectionResize(f32),
    PlayheadMoved(f32),
    SelectMarkerRegion(f32),
    StandalonePlaybackStarted(Result<(), String>),
    StandalonePlaybackStopped(Result<(), String>),
    FadeIn,
    FadeOut,
    IncreaseVolume,
    DecreaseVolume,
    Reverse,
    EditAction(AudioEditAction),
    Undo,
    Redo,
    DeleteSelection,
    OpenPath(PathBuf),
    OpenAudio(AudioBuffer),
    OpenClip {
        path: PathBuf,
        offset: usize,
        length: usize,
        timeline_start: Option<usize>,
    },
    FileOpened(Option<PathBuf>),
    DocumentLoadProgress {
        progress: f32,
        status: String,
    },
    DocumentLoaded(Result<AudioDocument, String>),
    EngineDocumentPrepared(Result<(), String>),
    FileSaved(Option<PathBuf>),
    DocumentSaved(Result<PathBuf, String>),
    WindowCloseRequested(window::Id),
    CloseDialogResult(window::Id, rfd::MessageDialogResult),
    MarkerCreateDialog {
        sample: usize,
    },
    MarkerNameInput(String),
    MarkerNameConfirm,
    MarkerNameCancel,
    MarkerDelete {
        sample: usize,
    },
    DetectMarkersDialog,
    DetectMarkersThresholdInput(String),
    DetectMarkersSilenceSamplesInput(String),
    DetectMarkersConfirm,
    DetectMarkersCancel,
    ExportMarkersDialog,
    ExportMarkersDirectorySelected(Option<PathBuf>),
    ExportMarkersFormatSelected(ExportFormat),
    ExportMarkersBitDepthSelected(ExportBitDepth),
    ExportMarkersSampleRateSelected(ExportSampleRate),
    ExportMarkersConfirm,
    ExportMarkersCancel,
    ExportMarkersFinished(Result<usize, String>),
    PreferencesDialog,
    PreferencesOutputDeviceSelected(AudioDeviceOption),
    PreferencesInputDeviceSelected(AudioDeviceOption),
    PreferencesSave,
    PreferencesCancel,
}

pub fn message_edits_document(message: &Message) -> bool {
    matches!(
        message,
        Message::FadeIn
            | Message::FadeOut
            | Message::IncreaseVolume
            | Message::DecreaseVolume
            | Message::Reverse
            | Message::EditAction(_)
            | Message::Undo
            | Message::Redo
            | Message::DeleteSelection
            | Message::MarkerNameConfirm
            | Message::MarkerDelete { .. }
            | Message::DetectMarkersConfirm
    )
}

#[derive(Debug, Clone)]
pub struct AudioBuffer {
    pub name: String,
    pub samples: Arc<Vec<f32>>,
    pub channels: usize,
    pub sample_rate: u32,
}

impl AudioBuffer {
    pub fn new(
        name: impl Into<String>,
        samples: impl Into<Arc<Vec<f32>>>,
        channels: usize,
        sample_rate: u32,
    ) -> Self {
        Self {
            name: name.into(),
            samples: samples.into(),
            channels,
            sample_rate,
        }
    }
}
