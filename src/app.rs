use std::{
    fmt,
    path::{Path, PathBuf},
    sync::Arc,
    time::{Duration, Instant},
};

use maolan_engine::audio_codec::{
    AudioDither, AudioEncodeFormat, WavBitDepth, decode_audio_to_f32_interleaved_sync,
    encode_audio_to_file,
};
use maolan_engine::{
    client::Client as EngineClient,
    history::{History as EngineHistory, UndoEntry},
    kind::Kind,
    message::{Action as EngineAction, Message as EngineMessage, generate_clip_id},
};
use maolan_widgets::iced::{
    Background, Border, Color, Element, Length, Subscription, Task, Theme, keyboard, time,
    widget::{
        Id, Space, button, column, container, pick_list, progress_bar, row, text, text_input,
        tooltip,
    },
    window,
};
use maolan_widgets::iced_aw::menu::DrawPath;
use maolan_widgets::iced_fonts::lucide::{
    arrow_down, arrow_right, arrow_up, fast_forward, flag, play, redo, rewind, square,
    trending_down, trending_up, undo,
};
use maolan_widgets::waveform::SampleWaveform;
use maolan_widgets::{
    audio_setup::{AudioSetupAction, AudioSetupState, audio_setup},
    menu::{menu_bar, menu_dropdown, menu_item, menu_items},
    meters,
};
use rubato::audioadapter_buffers::direct::SequentialSliceOfVecs;
use rubato::{Fft, FixedSync, Resampler};

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
    StartupOpened(Result<EngineClient, String>),
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
    Undo,
    Redo,
    DeleteSelection,
    OpenPath(PathBuf),
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
            | Message::Undo
            | Message::Redo
            | Message::DeleteSelection
            | Message::MarkerNameConfirm
            | Message::MarkerDelete { .. }
            | Message::DetectMarkersConfirm
    )
}

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

#[derive(Debug, Default)]
pub struct EditApp {
    standalone_ready: bool,
    setup: StartupSetup,
    audio: Option<AudioDocument>,
    history: EditHistory,
    status: String,
    busy: bool,
    preparing_playback: bool,
    busy_progress: f32,
    playing: bool,
    playhead_samples: usize,
    selection_anchor_samples: Option<usize>,
    selection_samples: Option<(usize, usize)>,
    engine_clip_path: Option<PathBuf>,
    engine_playback: Option<EnginePlayback>,
    close_window_id: Option<window::Id>,
    marker_dialog: Option<MarkerDialog>,
    detect_markers_dialog: Option<DetectMarkersDialog>,
    export_markers_dialog: Option<ExportMarkersDialog>,
    preferences_dialog: Option<PreferencesDialog>,
    vst3_plugins_loaded: bool,
    vst3_plugins_unavailable: bool,
    clap_plugins_loaded: bool,
    clap_plugins_unavailable: bool,
    #[cfg(unix)]
    lv2_plugins_loaded: bool,
    #[cfg(unix)]
    lv2_plugins_unavailable: bool,
}

#[derive(Debug, Clone)]
pub struct AudioDocument {
    source_path: PathBuf,
    save_path: Option<PathBuf>,
    samples: Vec<f32>,
    preview_samples: Vec<f32>,
    channels: usize,
    sample_rate: u32,
    channel_samples: Vec<Vec<f32>>,
    peak: f32,
    clip_region: Option<AudioRegion>,
    edits: AudioEdits,
    markers: Vec<(usize, String)>,
}

#[derive(Debug, Clone)]
struct MarkerDialog {
    sample: usize,
    name: String,
}

#[derive(Debug, Clone)]
struct DetectMarkersDialog {
    threshold_db: String,
    silence_samples: String,
}

impl Default for DetectMarkersDialog {
    fn default() -> Self {
        Self {
            threshold_db: String::from("-60.0"),
            silence_samples: String::from("1000"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ExportFormat {
    #[default]
    Wav,
    Flac,
    OggFlac,
    Mp3,
}

impl fmt::Display for ExportFormat {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Wav => write!(f, "WAV"),
            Self::Flac => write!(f, "FLAC"),
            Self::OggFlac => write!(f, "OGG FLAC"),
            Self::Mp3 => write!(f, "MP3"),
        }
    }
}

impl ExportFormat {
    const ALL: &'static [Self] = &[Self::Wav, Self::Flac, Self::OggFlac, Self::Mp3];

    fn extension(self) -> &'static str {
        match self {
            Self::Wav => "wav",
            Self::Flac => "flac",
            Self::OggFlac => "ogg",
            Self::Mp3 => "mp3",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ExportBitDepth {
    #[default]
    Bits16,
    Bits24,
    Bits32,
}

impl fmt::Display for ExportBitDepth {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Bits16 => write!(f, "16-bit"),
            Self::Bits24 => write!(f, "24-bit"),
            Self::Bits32 => write!(f, "32-bit"),
        }
    }
}

impl ExportBitDepth {
    const ALL: &'static [Self] = &[Self::Bits16, Self::Bits24, Self::Bits32];

    fn bits(self) -> u16 {
        match self {
            Self::Bits16 => 16,
            Self::Bits24 => 24,
            Self::Bits32 => 32,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExportSampleRate {
    Hz22050,
    Hz44100,
    Hz48000,
    Hz88200,
    Hz96000,
    Hz192000,
}

impl fmt::Display for ExportSampleRate {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} Hz", self.value())
    }
}

impl ExportSampleRate {
    const ALL: &'static [Self] = &[
        Self::Hz22050,
        Self::Hz44100,
        Self::Hz48000,
        Self::Hz88200,
        Self::Hz96000,
        Self::Hz192000,
    ];

    fn value(self) -> u32 {
        match self {
            Self::Hz22050 => 22_050,
            Self::Hz44100 => 44_100,
            Self::Hz48000 => 48_000,
            Self::Hz88200 => 88_200,
            Self::Hz96000 => 96_000,
            Self::Hz192000 => 192_000,
        }
    }
}

#[derive(Debug, Clone)]
struct ExportMarkersDialog {
    directory: Option<PathBuf>,
    format: ExportFormat,
    bit_depth: ExportBitDepth,
    sample_rate: ExportSampleRate,
}

impl Default for ExportMarkersDialog {
    fn default() -> Self {
        Self {
            directory: None,
            format: ExportFormat::Wav,
            bit_depth: ExportBitDepth::Bits24,
            sample_rate: ExportSampleRate::Hz48000,
        }
    }
}

#[derive(Debug, Clone)]
struct PreferencesDialog {
    output_devices: Vec<AudioDeviceOption>,
    input_devices: Vec<AudioDeviceOption>,
    output_device: Option<AudioDeviceOption>,
    input_device: Option<AudioDeviceOption>,
}

impl PreferencesDialog {
    fn from_setup(setup: &StartupSetup) -> Self {
        Self {
            output_devices: setup.output_devices.clone(),
            input_devices: setup.input_devices.clone(),
            output_device: setup.output_device.clone(),
            input_device: setup.input_device.clone(),
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct AudioRegion {
    offset: usize,
    length: usize,
}

#[derive(Debug, Clone, Copy, Default, PartialEq)]
struct AudioEdits {
    fade_in_samples: usize,
    fade_out_samples: usize,
    gain_db: f32,
    reversed: bool,
}

impl AudioEdits {
    fn needs_rendered_preview_file(self) -> bool {
        self.fade_in_samples != 0 || self.fade_out_samples != 0 || self.gain_db != 0.0
    }
}

#[derive(Debug, Clone)]
struct DocumentSnapshot {
    samples: Vec<f32>,
    edits: AudioEdits,
    markers: Vec<(usize, String)>,
}

const EDIT_HISTORY_SOURCE: &str = "edit";
const EDIT_HISTORY_MAX_ENTRIES: usize = 1000;

struct EditHistory {
    history: EngineHistory,
    snapshots: Vec<DocumentSnapshot>,
}

impl std::fmt::Debug for EditHistory {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EditHistory")
            .field("snapshots", &self.snapshots.len())
            .finish()
    }
}

impl Default for EditHistory {
    fn default() -> Self {
        Self {
            history: EngineHistory::new(EDIT_HISTORY_MAX_ENTRIES),
            snapshots: Vec::new(),
        }
    }
}

impl EditHistory {
    fn new(initial: DocumentSnapshot) -> Self {
        Self {
            history: EngineHistory::new(EDIT_HISTORY_MAX_ENTRIES),
            snapshots: vec![initial],
        }
    }

    fn is_dirty(&self) -> bool {
        self.history.is_dirty()
    }

    fn mark_saved(&mut self) {
        self.history.mark_save_point();
    }

    fn record(&mut self, previous: DocumentSnapshot, current: DocumentSnapshot) {
        let previous_index = self.push_snapshot(previous);
        let current_index = self.push_snapshot(current);
        self.history.record(UndoEntry {
            forward_actions: vec![snapshot_action(current_index)],
            inverse_actions: vec![snapshot_action(previous_index)],
        });
    }

    fn undo(&mut self) -> Option<DocumentSnapshot> {
        let actions = self.history.undo()?;
        let index = parse_snapshot_index(&actions[0])?;
        self.snapshots.get(index).cloned()
    }

    fn redo(&mut self) -> Option<DocumentSnapshot> {
        let actions = self.history.redo()?;
        let index = parse_snapshot_index(&actions[0])?;
        self.snapshots.get(index).cloned()
    }

    fn push_snapshot(&mut self, snapshot: DocumentSnapshot) -> usize {
        let index = self.snapshots.len();
        self.snapshots.push(snapshot);
        index
    }
}

fn snapshot_action(index: usize) -> EngineAction {
    EngineAction::Log {
        source: EDIT_HISTORY_SOURCE.to_string(),
        message: index.to_string(),
    }
}

fn parse_snapshot_index(action: &EngineAction) -> Option<usize> {
    match action {
        EngineAction::Log { source, message } if source == EDIT_HISTORY_SOURCE => {
            message.parse().ok()
        }
        _ => None,
    }
}

#[derive(Debug)]
struct EnginePlayback {
    client: EngineClient,
}

struct EngineDocumentRequest {
    path: PathBuf,
    samples: Vec<f32>,
    channels: usize,
    sample_rate: u32,
    clip_len: usize,
    clip_offset: usize,
    render_preview: bool,
    reversed: bool,
}

#[derive(Debug, Clone)]
pub struct AudioDeviceOption {
    pub(crate) id: String,
    pub(crate) label: String,
    pub(crate) supported_bits: Vec<usize>,
    pub(crate) supported_sample_rates: Vec<i32>,
    pub(crate) max_channels: usize,
    pub(crate) max_buffer_bytes: usize,
    pub(crate) supports_input: bool,
    pub(crate) supports_output: bool,
}

impl PartialEq for AudioDeviceOption {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}

impl Eq for AudioDeviceOption {}

impl std::hash::Hash for AudioDeviceOption {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.id.hash(state);
    }
}

impl AudioDeviceOption {
    pub(crate) fn with_supported_caps(
        id: impl Into<String>,
        label: impl Into<String>,
        mut supported_bits: Vec<usize>,
        mut supported_sample_rates: Vec<i32>,
    ) -> Self {
        supported_bits.sort_by(|a, b| b.cmp(a));
        supported_bits.dedup();
        supported_sample_rates.retain(|rate| *rate > 0);
        supported_sample_rates.sort_unstable();
        supported_sample_rates.dedup();
        Self {
            id: id.into(),
            label: label.into(),
            supported_bits,
            supported_sample_rates,
            max_channels: 0,
            max_buffer_bytes: 0,
            supports_input: true,
            supports_output: true,
        }
    }

    pub(crate) fn with_oss_caps(
        id: impl Into<String>,
        label: impl Into<String>,
        supported_bits: Vec<usize>,
        supported_sample_rates: Vec<i32>,
        max_channels: usize,
        max_buffer_bytes: usize,
    ) -> Self {
        let mut out = Self::with_supported_caps(id, label, supported_bits, supported_sample_rates);
        out.max_channels = max_channels;
        out.max_buffer_bytes = max_buffer_bytes;
        out
    }

    #[cfg(target_os = "linux")]
    pub(crate) fn with_supported_direction_caps(
        id: impl Into<String>,
        label: impl Into<String>,
        mut supported_bits: Vec<usize>,
        mut supported_sample_rates: Vec<i32>,
        supports_input: bool,
        supports_output: bool,
    ) -> Self {
        supported_bits.sort_by(|a, b| b.cmp(a));
        supported_bits.dedup();
        supported_sample_rates.retain(|rate| *rate > 0);
        supported_sample_rates.sort_unstable();
        supported_sample_rates.dedup();
        Self {
            id: id.into(),
            label: label.into(),
            supported_bits,
            supported_sample_rates,
            max_channels: 0,
            max_buffer_bytes: 0,
            supports_input,
            supports_output,
        }
    }
}

#[cfg(target_os = "freebsd")]
impl From<maolan_engine::audio_devices::AudioDeviceDescriptor> for AudioDeviceOption {
    fn from(device: maolan_engine::audio_devices::AudioDeviceDescriptor) -> Self {
        let mut out = Self::with_oss_caps(
            device.id,
            device.label,
            device.supported_bits,
            device.supported_sample_rates,
            device.max_channels,
            device.max_buffer_bytes,
        );
        out.supports_input = device.supports_input;
        out.supports_output = device.supports_output;
        out
    }
}

impl fmt::Display for AudioDeviceOption {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.supported_bits.is_empty() {
            return f.write_str(&self.label);
        }
        let formats = self
            .supported_bits
            .iter()
            .map(|bits| format!("{bits}"))
            .collect::<Vec<_>>()
            .join("/");
        write!(f, "{} [{}-bit]", self.label, formats)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct StartupSetup {
    audio_engine: AudioEngineOption,
    output_devices: Vec<AudioDeviceOption>,
    input_devices: Vec<AudioDeviceOption>,
    output_device: Option<AudioDeviceOption>,
    input_device: Option<AudioDeviceOption>,
    sample_rate_hz: i32,
    bits: usize,
    exclusive: bool,
    period_frames: usize,
    nperiods: usize,
    sync_mode: bool,
}

impl StartupSetup {
    fn with_preferences(
        preferences: &EditorPreferences,
        output_devices: Vec<AudioDeviceOption>,
        input_devices: Vec<AudioDeviceOption>,
    ) -> Self {
        let audio_engine = AudioEngineOption::default();
        let output_device = preferences
            .default_output_device_id
            .as_deref()
            .and_then(|id| {
                output_devices
                    .iter()
                    .find(|device| device.id == id)
                    .cloned()
            })
            .or_else(|| output_devices.first().cloned());
        let input_device = preferences
            .default_input_device_id
            .as_deref()
            .and_then(|id| input_devices.iter().find(|device| device.id == id).cloned())
            .or_else(|| input_devices.first().cloned());
        let mut setup = Self {
            audio_engine,
            output_devices,
            input_devices,
            output_device,
            input_device,
            sample_rate_hz: 48_000,
            bits: 32,
            exclusive: true,
            period_frames: 1024,
            nperiods: maolan_widgets::audio_setup::DEFAULT_N_PERIODS,
            sync_mode: false,
        };
        setup.sample_rate_hz = pick_sample_rate(&setup);
        setup.bits = pick_bits(&setup);
        setup.period_frames = pick_period_frames(&setup);
        setup
    }
}

impl Default for StartupSetup {
    fn default() -> Self {
        let preferences = EditorPreferences::load();
        let audio_engine = AudioEngineOption::default();
        let output_devices = discover_output_audio_devices(audio_engine);
        let input_devices = discover_input_audio_devices(audio_engine);
        Self::with_preferences(&preferences, output_devices, input_devices)
    }
}

#[derive(Debug, Clone, Default)]
struct EditorPreferences {
    default_output_device_id: Option<String>,
    default_input_device_id: Option<String>,
}

impl EditorPreferences {
    fn load() -> Self {
        let Some(config_path) = edit_config_path() else {
            return Self::default();
        };
        Self::load_from_path(&config_path)
    }

    fn load_from_path(config_path: &Path) -> Self {
        let Ok(contents) = std::fs::read_to_string(config_path) else {
            return Self::default();
        };
        let Ok(value) = toml::from_str::<toml::Value>(&contents) else {
            return Self::default();
        };
        Self {
            default_output_device_id: preference_device_id(&value, "default_output_device_id"),
            default_input_device_id: preference_device_id(&value, "default_input_device_id"),
        }
    }

    fn save(&self) -> Result<(), String> {
        let Some(config_path) = edit_config_path() else {
            return Err(String::from("Could not determine config directory."));
        };
        self.save_to_path(&config_path)
    }

    fn save_to_path(&self, config_path: &Path) -> Result<(), String> {
        let mut lines: Vec<String> = if config_path.exists() {
            std::fs::read_to_string(config_path)
                .map_err(|err| err.to_string())?
                .lines()
                .map(ToOwned::to_owned)
                .collect()
        } else {
            Vec::new()
        };

        let mut output_set = false;
        let mut input_set = false;
        for line in &mut lines {
            let trimmed = line.trim_start();
            if trimmed.starts_with("default_output_device_id") {
                if let Some(id) = self.default_output_device_id.as_deref() {
                    *line = format!("default_output_device_id = \"{id}\"");
                } else {
                    *line = String::new();
                }
                output_set = true;
            } else if trimmed.starts_with("default_input_device_id") {
                if let Some(id) = self.default_input_device_id.as_deref() {
                    *line = format!("default_input_device_id = \"{id}\"");
                } else {
                    *line = String::new();
                }
                input_set = true;
            }
        }
        lines.retain(|line| !line.is_empty());

        if !output_set && let Some(id) = self.default_output_device_id.as_deref() {
            lines.push(format!("default_output_device_id = \"{id}\""));
        }
        if !input_set && let Some(id) = self.default_input_device_id.as_deref() {
            lines.push(format!("default_input_device_id = \"{id}\""));
        }

        if let Some(parent) = config_path.parent() {
            std::fs::create_dir_all(parent).map_err(|err| err.to_string())?;
        }
        let content = if lines.is_empty() {
            String::new()
        } else {
            format!("{}\n", lines.join("\n"))
        };
        std::fs::write(config_path, content).map_err(|err| err.to_string())?;
        Ok(())
    }
}

fn preference_device_id(value: &toml::Value, key: &str) -> Option<String> {
    value
        .get(key)
        .and_then(toml::Value::as_str)
        .filter(|id| !id.is_empty() && *id != "__auto__")
        .map(ToOwned::to_owned)
}

fn edit_config_path() -> Option<PathBuf> {
    let home = std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE"))?;
    Some(
        PathBuf::from(home)
            .join(".config")
            .join("maolan")
            .join("edit.toml"),
    )
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AudioEngineOption {
    #[cfg(target_os = "linux")]
    #[default]
    Alsa,
    #[cfg(unix)]
    #[cfg_attr(
        not(any(target_os = "linux", target_os = "freebsd", target_os = "openbsd")),
        default
    )]
    Jack,
    #[cfg(target_os = "freebsd")]
    #[default]
    Oss,
    #[cfg(target_os = "openbsd")]
    #[default]
    Sndio,
    #[cfg(target_os = "windows")]
    #[default]
    Wasapi,
}

impl AudioEngineOption {
    const ALL: &'static [Self] = &[
        #[cfg(target_os = "linux")]
        Self::Alsa,
        #[cfg(target_os = "freebsd")]
        Self::Oss,
        #[cfg(target_os = "openbsd")]
        Self::Sndio,
        #[cfg(target_os = "windows")]
        Self::Wasapi,
        #[cfg(unix)]
        Self::Jack,
    ];

    fn is_jack(self) -> bool {
        #[cfg(unix)]
        {
            self == Self::Jack
        }
        #[cfg(not(unix))]
        {
            false
        }
    }
}

impl fmt::Display for AudioEngineOption {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            #[cfg(target_os = "linux")]
            Self::Alsa => write!(f, "ALSA"),
            #[cfg(unix)]
            Self::Jack => write!(f, "JACK"),
            #[cfg(target_os = "freebsd")]
            Self::Oss => write!(f, "OSS"),
            #[cfg(target_os = "openbsd")]
            Self::Sndio => write!(f, "sndio"),
            #[cfg(target_os = "windows")]
            Self::Wasapi => write!(f, "WASAPI"),
        }
    }
}

fn pick_sample_rate(setup: &StartupSetup) -> i32 {
    let options = sample_rate_options(setup);
    if options.contains(&setup.sample_rate_hz) {
        setup.sample_rate_hz
    } else {
        options
            .iter()
            .min_by_key(|candidate| ((*candidate).saturating_sub(setup.sample_rate_hz)).abs())
            .copied()
            .unwrap_or(48_000)
    }
}

fn pick_bits(setup: &StartupSetup) -> usize {
    let options = bit_options(setup);
    if options.contains(&setup.bits) {
        setup.bits
    } else {
        options.first().copied().unwrap_or(32)
    }
}

fn pick_period_frames(setup: &StartupSetup) -> usize {
    let options = period_frame_options(setup);
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

impl EditApp {
    fn plugins_loaded(&self) -> bool {
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

pub fn new() -> (EditApp, Task<Message>) {
    let client = EngineClient::default();
    let mut scan_tasks = vec![
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
    ];
    #[cfg(unix)]
    scan_tasks.push(Task::perform(
        scan_plugins_startup(client.clone(), PluginFormat::Lv2),
        |loaded| {
            if loaded {
                Message::Lv2PluginsLoaded
            } else {
                Message::Lv2PluginsUnavailable
            }
        },
    ));
    (
        EditApp {
            status: String::from("Choose audio hardware and open the engine."),
            ..EditApp::default()
        },
        Task::batch(scan_tasks),
    )
}

#[derive(Debug, Clone, Copy)]
enum PluginFormat {
    Vst3,
    Clap,
    #[cfg(unix)]
    Lv2,
}

async fn scan_plugins_startup(client: EngineClient, format: PluginFormat) -> bool {
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
    let accepts = |action: &EngineAction| match format {
        PluginFormat::Vst3 => {
            matches!(
                action,
                EngineAction::Vst3Plugins(_) | EngineAction::Vst3PluginsUnavailable { .. }
            )
        }
        PluginFormat::Clap => {
            matches!(
                action,
                EngineAction::ClapPlugins(_) | EngineAction::ClapPluginsUnavailable { .. }
            )
        }
        #[cfg(unix)]
        PluginFormat::Lv2 => {
            matches!(
                action,
                EngineAction::Lv2Plugins(_) | EngineAction::Lv2PluginsUnavailable { .. }
            )
        }
    };
    wait_for_engine_response(&mut rx, accepts).await.is_ok()
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
            app.status = String::from("Scanning plugins and opening audio device...");
            let setup = app.setup.clone();
            Task::perform(open_standalone_engine(setup), Message::StartupOpened)
        }
        Message::StartupOpened(Ok(client)) => {
            app.busy = false;
            app.busy_progress = 1.0;
            app.standalone_ready = true;
            app.engine_playback = Some(EnginePlayback { client });
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
            app.busy = true;
            app.busy_progress = 0.0;
            app.status = String::from("Opening audio file...");
            Task::perform(open_audio_dialog(), Message::FileOpened)
        }
        Message::Close => {
            let engine_playback = if app.standalone_ready {
                app.engine_playback.take()
            } else {
                None
            };
            *app = EditApp {
                status: String::from("Open an audio file to view its waveform."),
                standalone_ready: app.standalone_ready,
                setup: app.setup.clone(),
                engine_playback,
                ..EditApp::default()
            };
            Task::none()
        }
        Message::OpenPath(path) => load_document(app, path, None, None),
        Message::OpenClip {
            path,
            offset,
            length,
            timeline_start,
        } => load_document(
            app,
            path,
            Some(AudioRegion { offset, length }),
            timeline_start.map(|offset| AudioRegion { offset, length }),
        ),
        Message::Save => {
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
        Message::SaveAs => {
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
            if let Some(playback) = app.engine_playback.as_ref() {
                let client = playback.client.clone();
                Task::perform(
                    async move { send_engine(&client, EngineAction::Stop).await },
                    Message::StandalonePlaybackStopped,
                )
            } else {
                Task::none()
            }
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
        Message::FadeIn => apply_standalone_edit(app, EditOperation::FadeIn),
        Message::FadeOut => apply_standalone_edit(app, EditOperation::FadeOut),
        Message::IncreaseVolume => apply_standalone_edit(app, EditOperation::IncreaseVolume),
        Message::DecreaseVolume => apply_standalone_edit(app, EditOperation::DecreaseVolume),
        Message::Reverse => reverse_clip(app),
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
        Message::FileOpened(Some(path)) => load_document(app, path, None, None),
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
            maolan_widgets::iced::widget::operation::focus(marker_name_input_id())
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

pub fn subscription(app: &EditApp) -> Subscription<Message> {
    let mut subscriptions = Vec::new();
    if app.standalone_ready {
        subscriptions.push(keyboard::listen().map(keyboard_message));
    }
    subscriptions.push(window::close_requests().map(Message::WindowCloseRequested));
    if app.playing {
        subscriptions.push(time::every(Duration::from_millis(40)).map(|_| Message::PlaybackTick));
    }
    Subscription::batch(subscriptions)
}

fn keyboard_message(event: keyboard::Event) -> Message {
    match event {
        keyboard::Event::KeyPressed {
            key: keyboard::Key::Named(keyboard::key::Named::Space),
            modifiers,
            repeat: false,
            ..
        } if modifiers.is_empty() => Message::TogglePlayback,
        keyboard::Event::KeyPressed {
            key: keyboard::Key::Character(c),
            modifiers,
            repeat: false,
            ..
        } if modifiers.is_empty() && c.as_str() == "z" => Message::JumpToNextZeroCrossing,
        keyboard::Event::KeyPressed {
            key: keyboard::Key::Character(c),
            modifiers,
            repeat: false,
            ..
        } if modifiers.command() => match c.as_str() {
            "z" | "Z" if modifiers.shift() => Message::Redo,
            "z" | "Z" => Message::Undo,
            "y" | "Y" => Message::Redo,
            _ => Message::None,
        },
        keyboard::Event::KeyPressed {
            key: keyboard::Key::Named(keyboard::key::Named::Delete),
            repeat: false,
            ..
        } => Message::DeleteSelection,
        keyboard::Event::KeyPressed {
            key: keyboard::Key::Named(keyboard::key::Named::Escape),
            repeat: false,
            ..
        } => Message::MarkerNameCancel,
        _ => Message::None,
    }
}

pub fn view(app: &EditApp) -> Element<'_, Message> {
    if !app.standalone_ready {
        return startup_view(app);
    }
    view_with_chrome(app, true, true, true)
}

pub fn embedded_view(app: &EditApp) -> Element<'_, Message> {
    view_with_chrome(app, false, true, true)
}

pub fn embedded_view_with_play_disabled(
    app: &EditApp,
    play_disabled: bool,
) -> Element<'_, Message> {
    view_with_chrome_options(app, false, true, true, play_disabled)
}

pub fn embedded_view_without_vu_meter(app: &EditApp) -> Element<'_, Message> {
    view_with_chrome(app, false, true, false)
}

fn view_with_chrome(
    app: &EditApp,
    show_menu: bool,
    show_toolbar: bool,
    show_vu_meter: bool,
) -> Element<'_, Message> {
    view_with_chrome_options(app, show_menu, show_toolbar, show_vu_meter, false)
}

fn view_with_chrome_options(
    app: &EditApp,
    show_menu: bool,
    show_toolbar: bool,
    show_vu_meter: bool,
    play_disabled: bool,
) -> Element<'_, Message> {
    let waveform: Element<'_, Message> = match app.audio.as_ref() {
        Some(audio) => {
            let markers = audio
                .markers
                .iter()
                .map(|(sample, name)| (*sample, name.clone()))
                .collect::<Vec<_>>();
            SampleWaveform::new(audio.channel_samples.iter().map(Vec::as_slice), audio.peak)
                .playhead_ratio(playhead_ratio(app))
                .selection_ratio(selection_ratio(app))
                .markers(markers)
                .on_selection_start(Message::SelectionStart)
                .on_selection_drag(Message::SelectionDrag)
                .on_selection_finish(Message::SelectionFinish)
                .on_click(Message::PlayheadMoved)
                .on_double_click(Message::SelectMarkerRegion)
                .on_right_click(|ratio| {
                    let sample = sample_at_ratio(app, ratio).unwrap_or(0);
                    Message::MarkerCreateDialog { sample }
                })
                .on_middle_click(|ratio| {
                    let sample = app
                        .audio
                        .as_ref()
                        .and_then(|audio| nearest_marker_sample(audio, ratio))
                        .unwrap_or(0);
                    Message::MarkerDelete { sample }
                })
                .on_middle_click_away(Message::SelectionResize)
                .view()
        }
        None => SampleWaveform::<Message>::new(std::iter::empty::<&[f32]>(), 0.0).view(),
    };

    let mut waveform = row![
        container(waveform)
            .width(Length::Fill)
            .height(Length::Fill)
            .style(panel_style),
    ]
    .spacing(8);
    if show_vu_meter {
        waveform = waveform.push(vu_meter(app));
    }
    let waveform = container(waveform)
        .width(Length::Fill)
        .height(Length::Fill)
        .style(|_theme| container::Style::default());
    let mut content = column![]
        .spacing(10)
        .padding(12)
        .width(Length::Fill)
        .height(Length::Fill);
    if show_menu {
        content = content.push(standalone_menu());
    }
    if show_toolbar {
        content = content.push(toolbar_for_app(app, play_disabled));
    }
    let mut content = content.push(waveform.width(Length::Fill).height(Length::Fill));
    if app.busy {
        content = content.push(progress_view(app.busy_progress));
    }
    let content = content.push(text(&app.status).size(12));

    let mut view: Element<'_, Message> = container(content)
        .width(Length::Fill)
        .height(Length::Fill)
        .style(app_style)
        .into();
    if let Some(dialog) = app.preferences_dialog.as_ref() {
        view = row![view, preferences_dialog_view(dialog)]
            .width(Length::Fill)
            .height(Length::Fill)
            .into();
    } else if let Some(dialog) = app.export_markers_dialog.as_ref() {
        view = row![view, export_markers_dialog_view(dialog)]
            .width(Length::Fill)
            .height(Length::Fill)
            .into();
    } else if let Some(dialog) = app.detect_markers_dialog.as_ref() {
        view = row![view, detect_markers_dialog_view(dialog)]
            .width(Length::Fill)
            .height(Length::Fill)
            .into();
    } else if let Some(dialog) = app.marker_dialog.as_ref() {
        view = row![view, marker_dialog_view(dialog)]
            .width(Length::Fill)
            .height(Length::Fill)
            .into();
    }

    view
}

pub fn menu(show_open: bool) -> Element<'static, Message> {
    let file_items = if show_open {
        maolan_widgets::iced_aw::menu::Menu::new(menu_items!(
            (menu_item("Open", Message::Open)),
            (menu_item("Close", Message::Close)),
            (menu_item("Save", Message::Save)),
            (menu_item("Save As", Message::SaveAs)),
        ))
    } else {
        maolan_widgets::iced_aw::menu::Menu::new(menu_items!(
            (menu_item("Close", Message::Close)),
            (menu_item("Save", Message::Save)),
            (menu_item("Save As", Message::SaveAs)),
        ))
    }
    .width(180.0)
    .offset(15.0)
    .spacing(5.0);

    let edit_items = maolan_widgets::iced_aw::menu::Menu::new(menu_items!(
        (menu_item("Undo", Message::Undo)),
        (menu_item("Redo", Message::Redo)),
        (menu_item("Next Zero Crossing", Message::JumpToNextZeroCrossing)),
        (menu_item("Reverse", Message::Reverse)),
        (menu_item("Detect Markers", Message::DetectMarkersDialog)),
        (menu_item("Export Markers", Message::ExportMarkersDialog)),
        (menu_item("Preferences", Message::PreferencesDialog)),
    ))
    .width(180.0)
    .offset(15.0)
    .spacing(5.0);

    menu_bar!(
        (menu_dropdown("File", Message::None), { file_items }),
        (menu_dropdown("Edit", Message::None), { edit_items }),
    )
    .draw_path(DrawPath::Backdrop)
    .close_on_item_click_global(true)
    .width(Length::Fill)
    .into()
}

pub fn standalone_menu() -> Element<'static, Message> {
    let file_items = maolan_widgets::iced_aw::menu::Menu::new(menu_items!(
        (menu_item("Open", Message::Open)),
        (menu_item("Close", Message::Close)),
        (menu_item("Save", Message::Save)),
        (menu_item("Save As", Message::SaveAs)),
    ))
    .width(180.0)
    .offset(15.0)
    .spacing(5.0);

    let edit_items = maolan_widgets::iced_aw::menu::Menu::new(menu_items!(
        (menu_item("Undo", Message::Undo)),
        (menu_item("Redo", Message::Redo)),
        (menu_item("Next Zero Crossing", Message::JumpToNextZeroCrossing)),
        (menu_item("Reverse", Message::Reverse)),
        (menu_item("Detect Markers", Message::DetectMarkersDialog)),
        (menu_item("Export Markers", Message::ExportMarkersDialog)),
        (menu_item("Preferences", Message::PreferencesDialog)),
    ))
    .width(180.0)
    .offset(15.0)
    .spacing(5.0);

    menu_bar!(
        (menu_dropdown("File", Message::None), { file_items }),
        (menu_dropdown("Edit", Message::None), { edit_items }),
    )
    .draw_path(DrawPath::Backdrop)
    .close_on_item_click_global(true)
    .width(Length::Fill)
    .into()
}

pub fn toolbar() -> Element<'static, Message> {
    toolbar_with_playhead("00:00.000", false)
}

pub fn toolbar_with_playhead(label: impl Into<String>, playing: bool) -> Element<'static, Message> {
    toolbar_with_playhead_options(label, playing, false)
}

fn toolbar_with_playhead_options(
    label: impl Into<String>,
    playing: bool,
    play_disabled: bool,
) -> Element<'static, Message> {
    let label = label.into();
    let play_button = if play_disabled {
        toolbar_button_disabled(play().size(16), "Play")
    } else {
        toolbar_button(play().size(16), "Play", Message::Play)
    };
    container(
        row![
            toolbar_button(undo().size(16), "Undo", Message::Undo),
            toolbar_button(redo().size(16), "Redo", Message::Redo),
            toolbar_button(rewind().size(16), "Rewind to start", Message::RewindToStart),
            play_button,
            toolbar_button(square().size(16), "Stop", Message::Stop),
            toolbar_button(fast_forward().size(16), "Go to end", Message::GoToEnd),
            toolbar_button(
                arrow_right().size(16),
                "Next zero crossing",
                Message::JumpToNextZeroCrossing
            ),
            container(text(label).size(14))
                .padding([4, 8])
                .style(if playing {
                    playhead_active_style
                } else {
                    playhead_style
                }),
            toolbar_button(trending_up().size(16), "Fade in", Message::FadeIn),
            toolbar_button(trending_down().size(16), "Fade out", Message::FadeOut),
            toolbar_button(
                flag().size(16),
                "Detect markers",
                Message::DetectMarkersDialog
            ),
            toolbar_button(
                arrow_up().size(16),
                "Increase volume",
                Message::IncreaseVolume
            ),
            toolbar_button(
                arrow_down().size(16),
                "Decrease volume",
                Message::DecreaseVolume
            ),
            Space::new().width(Length::Fill),
        ]
        .spacing(4)
        .align_y(maolan_widgets::iced::Alignment::Center),
    )
    .width(Length::Fill)
    .height(Length::Fixed(34.0))
    .padding([4, 8])
    .style(toolbar_style)
    .into()
}

fn toolbar_for_app(app: &EditApp, play_disabled: bool) -> Element<'static, Message> {
    toolbar_with_playhead_options(playhead_label(app), app.playing, play_disabled)
}

fn toolbar_button<'a>(
    icon: impl Into<Element<'a, Message>>,
    label: &'static str,
    message: Message,
) -> Element<'a, Message> {
    tooltip(
        button(icon)
            .width(Length::Fixed(30.0))
            .height(Length::Fixed(26.0))
            .style(toolbar_button_style)
            .on_press(message),
        container(text(label).size(12))
            .padding([4, 8])
            .style(tooltip_style),
        tooltip::Position::Bottom,
    )
    .gap(4)
    .into()
}

fn toolbar_button_disabled<'a>(
    icon: impl Into<Element<'a, Message>>,
    label: &'static str,
) -> Element<'a, Message> {
    tooltip(
        button(icon)
            .width(Length::Fixed(30.0))
            .height(Length::Fixed(26.0))
            .style(toolbar_button_style),
        container(text(label).size(12))
            .padding([4, 8])
            .style(tooltip_style),
        tooltip::Position::Bottom,
    )
    .gap(4)
    .into()
}

fn load_document(
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

fn document_status(audio: &AudioDocument) -> String {
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

fn progress_view(progress: f32) -> Element<'static, Message> {
    let progress = progress.clamp(0.0, 1.0);
    let percent = (progress * 100.0).round() as u8;
    row![
        container(progress_bar(0.0..=1.0, progress)).width(Length::Fill),
        text(format!("{percent}%"))
            .size(12)
            .width(Length::Fixed(44.0)),
    ]
    .spacing(8)
    .align_y(maolan_widgets::iced::Alignment::Center)
    .into()
}

fn marker_name_input_id() -> Id {
    Id::new("edit-marker-name-input")
}

fn detect_markers_threshold_input_id() -> Id {
    Id::new("edit-detect-markers-threshold-input")
}

fn marker_dialog_view(dialog: &MarkerDialog) -> Element<'_, Message> {
    let can_confirm = !dialog.name.trim().is_empty();
    let confirm_button = if can_confirm {
        button("Create").on_press(Message::MarkerNameConfirm)
    } else {
        button("Create")
    };

    container(
        column![
            text("Add Marker"),
            text_input("Enter marker name", &dialog.name)
                .id(marker_name_input_id())
                .on_input(Message::MarkerNameInput)
                .on_submit(Message::MarkerNameConfirm)
                .width(Length::Fill),
            row![
                confirm_button,
                button("Cancel")
                    .on_press(Message::MarkerNameCancel)
                    .style(button::secondary)
            ]
            .spacing(10),
        ]
        .spacing(10),
    )
    .style(|_theme| container::Style {
        border: Border {
            color: Color::from_rgba(0.34, 0.42, 0.56, 0.72),
            width: 1.0,
            ..Border::default()
        },
        background: Some(Background::Color(Color::from_rgb(0.12, 0.13, 0.16))),
        ..container::Style::default()
    })
    .padding(12)
    .width(Length::Fixed(320.0))
    .into()
}

fn detect_markers_dialog_view(dialog: &DetectMarkersDialog) -> Element<'_, Message> {
    let threshold_valid = dialog.threshold_db.trim().parse::<f32>().is_ok();
    let silence_valid = dialog
        .silence_samples
        .trim()
        .parse::<usize>()
        .is_ok_and(|value| value > 0);
    let can_confirm = threshold_valid && silence_valid;
    let confirm_button = if can_confirm {
        button("Detect").on_press(Message::DetectMarkersConfirm)
    } else {
        button("Detect")
    };

    container(
        column![
            text("Detect Markers"),
            text("Silence threshold (dB)").size(12),
            text_input("-60.0", &dialog.threshold_db)
                .id(detect_markers_threshold_input_id())
                .on_input(Message::DetectMarkersThresholdInput)
                .on_submit(Message::DetectMarkersConfirm)
                .width(Length::Fill),
            text("Silent samples").size(12),
            text_input("1000", &dialog.silence_samples)
                .on_input(Message::DetectMarkersSilenceSamplesInput)
                .on_submit(Message::DetectMarkersConfirm)
                .width(Length::Fill),
            row![
                confirm_button,
                button("Cancel")
                    .on_press(Message::DetectMarkersCancel)
                    .style(button::secondary)
            ]
            .spacing(10),
        ]
        .spacing(10),
    )
    .style(|_theme| container::Style {
        border: Border {
            color: Color::from_rgba(0.34, 0.42, 0.56, 0.72),
            width: 1.0,
            ..Border::default()
        },
        background: Some(Background::Color(Color::from_rgb(0.12, 0.13, 0.16))),
        ..container::Style::default()
    })
    .padding(12)
    .width(Length::Fixed(320.0))
    .into()
}

fn export_markers_dialog_view(dialog: &ExportMarkersDialog) -> Element<'_, Message> {
    let directory_label = dialog
        .directory
        .as_ref()
        .map(|path| path.to_string_lossy().to_string())
        .unwrap_or_else(|| String::from("No directory selected"));
    let can_confirm = dialog.directory.is_some();
    let confirm_button = if can_confirm {
        button("Export").on_press(Message::ExportMarkersConfirm)
    } else {
        button("Export")
    };
    let show_bit_depth = dialog.format != ExportFormat::Mp3;
    let bit_depth_input: Element<'_, Message> = if show_bit_depth {
        pick_list(
            ExportBitDepth::ALL,
            Some(dialog.bit_depth),
            Message::ExportMarkersBitDepthSelected,
        )
        .width(Length::Fill)
        .into()
    } else {
        container(text("MP3 uses 16-bit PCM internally.").size(12))
            .width(Length::Fill)
            .into()
    };

    container(
        column![
            text("Export Marker Ranges"),
            button("Choose Directory...")
                .on_press(Message::ExportMarkersDialog)
                .width(Length::Fill),
            text(directory_label).size(12),
            text("Format").size(12),
            pick_list(
                ExportFormat::ALL,
                Some(dialog.format),
                Message::ExportMarkersFormatSelected
            )
            .width(Length::Fill),
            text("Sample rate").size(12),
            pick_list(
                ExportSampleRate::ALL,
                Some(dialog.sample_rate),
                Message::ExportMarkersSampleRateSelected
            )
            .width(Length::Fill),
            text("Bit depth").size(12),
            bit_depth_input,
            row![
                confirm_button,
                button("Cancel")
                    .on_press(Message::ExportMarkersCancel)
                    .style(button::secondary)
            ]
            .spacing(10),
        ]
        .spacing(10),
    )
    .style(|_theme| container::Style {
        border: Border {
            color: Color::from_rgba(0.34, 0.42, 0.56, 0.72),
            width: 1.0,
            ..Border::default()
        },
        background: Some(Background::Color(Color::from_rgb(0.12, 0.13, 0.16))),
        ..container::Style::default()
    })
    .padding(12)
    .width(Length::Fixed(360.0))
    .into()
}

fn preferences_dialog_view(dialog: &PreferencesDialog) -> Element<'_, Message> {
    const HAS_SEPARATE_AUDIO_INPUT_DEVICE: bool = cfg!(any(
        target_os = "freebsd",
        target_os = "linux",
        target_os = "openbsd",
        target_os = "windows"
    ));
    let show_input_device = !dialog.output_devices.is_empty() && HAS_SEPARATE_AUDIO_INPUT_DEVICE;
    let mut content = column![text("Preferences")].spacing(10);
    if show_input_device {
        content = content.push(
            row![
                text("Default input device:").width(Length::Fixed(160.0)),
                pick_list(
                    dialog.input_devices.clone(),
                    dialog.input_device.clone(),
                    Message::PreferencesInputDeviceSelected
                )
                .placeholder("Choose input device")
                .width(Length::Fill),
            ]
            .spacing(10)
            .align_y(maolan_widgets::iced::Alignment::Center),
        );
    }
    content = content.push(
        row![
            text("Default output device:").width(Length::Fixed(160.0)),
            pick_list(
                dialog.output_devices.clone(),
                dialog.output_device.clone(),
                Message::PreferencesOutputDeviceSelected
            )
            .placeholder("Choose output device")
            .width(Length::Fill),
        ]
        .spacing(10)
        .align_y(maolan_widgets::iced::Alignment::Center),
    );
    content = content.push(
        row![
            button("Save").on_press(Message::PreferencesSave),
            button("Cancel")
                .on_press(Message::PreferencesCancel)
                .style(button::secondary),
        ]
        .spacing(10),
    );

    container(content)
        .style(|_theme| container::Style {
            border: Border {
                color: Color::from_rgba(0.34, 0.42, 0.56, 0.72),
                width: 1.0,
                ..Border::default()
            },
            background: Some(Background::Color(Color::from_rgb(0.12, 0.13, 0.16))),
            ..container::Style::default()
        })
        .padding(12)
        .width(Length::Fixed(420.0))
        .into()
}

fn audio_setup_state(app: &EditApp) -> AudioSetupState<AudioEngineOption, AudioDeviceOption> {
    let is_jack = app.setup.audio_engine.is_jack();
    let show_input_device = !is_jack
        && cfg!(any(
            target_os = "freebsd",
            target_os = "linux",
            target_os = "openbsd",
            target_os = "windows"
        ));
    let show_bit_depth = !is_jack
        && cfg!(any(
            target_os = "freebsd",
            target_os = "linux",
            target_os = "openbsd",
            target_os = "windows"
        ));
    let output_devices: Vec<AudioDeviceOption> = app
        .setup
        .output_devices
        .iter()
        .filter(|device| backend_matches_device(app.setup.audio_engine, &device.id))
        .cloned()
        .collect();
    let input_devices: Vec<AudioDeviceOption> = app
        .setup
        .input_devices
        .iter()
        .filter(|device| backend_matches_device(app.setup.audio_engine, &device.id))
        .cloned()
        .collect();
    let selected_output_device = app
        .setup
        .output_device
        .as_ref()
        .and_then(|device| output_devices.iter().find(|d| d.id == device.id).cloned());
    let selected_input_device = app
        .setup
        .input_device
        .as_ref()
        .and_then(|device| input_devices.iter().find(|d| d.id == device.id).cloned());
    let sample_rates = sample_rate_options(&app.setup);
    let selected_sample_rate = if sample_rates.contains(&app.setup.sample_rate_hz) {
        Some(app.setup.sample_rate_hz)
    } else {
        sample_rates
            .iter()
            .min_by_key(|candidate| ((*candidate).saturating_sub(app.setup.sample_rate_hz)).abs())
            .copied()
    };
    let bit_depths = bit_options(&app.setup);
    let selected_bit_depth = if show_bit_depth {
        Some(if bit_depths.contains(&app.setup.bits) {
            app.setup.bits
        } else {
            bit_depths.first().copied().unwrap_or(32)
        })
    } else {
        None
    };
    let period_frames = period_frame_options(&app.setup);
    let selected_period_frames = if period_frames.contains(&app.setup.period_frames) {
        Some(app.setup.period_frames)
    } else {
        period_frames
            .iter()
            .copied()
            .find(|value| *value >= app.setup.period_frames)
            .or_else(|| period_frames.last().copied())
    };
    let n_periods: Vec<usize> = (1..=16).collect();
    let plugins_loaded = app.plugins_loaded();
    const HAS_SEPARATE_AUDIO_INPUT_DEVICE: bool = cfg!(any(
        target_os = "freebsd",
        target_os = "linux",
        target_os = "openbsd",
        target_os = "windows"
    ));
    const REQUIRE_SAMPLE_RATES_FOR_HW_READY: bool = cfg!(any(
        target_os = "linux",
        target_os = "freebsd",
        target_os = "openbsd"
    ));
    let hw_ready = is_jack
        || (selected_output_device.is_some()
            && (!HAS_SEPARATE_AUDIO_INPUT_DEVICE || selected_input_device.is_some())
            && (!REQUIRE_SAMPLE_RATES_FOR_HW_READY || !sample_rates.is_empty()));

    AudioSetupState {
        backends: AudioEngineOption::ALL.to_vec(),
        selected_backend: app.setup.audio_engine,
        show_input_device,
        input_devices,
        selected_input_device,
        show_output_device: !is_jack,
        output_devices,
        selected_output_device,
        show_sample_rate: !is_jack,
        sample_rates,
        selected_sample_rate,
        show_bit_depth,
        bit_depths,
        selected_bit_depth,
        show_period_frames: !is_jack,
        period_frames,
        selected_period_frames,
        show_n_periods: !is_jack,
        n_periods,
        selected_n_periods: Some(app.setup.nperiods),
        show_exclusive: !is_jack,
        exclusive: app.setup.exclusive,
        show_sync_mode: !is_jack,
        sync_mode: app.setup.sync_mode,
        plugins_loaded,
        can_start: plugins_loaded && hw_ready,
        status_message: String::new(),
    }
}

fn startup_view(app: &EditApp) -> Element<'_, Message> {
    let setup_state = audio_setup_state(app);

    let content = audio_setup(setup_state, move |action| match action {
        AudioSetupAction::BackendSelected(b) => Message::StartupBackendSelected(b),
        AudioSetupAction::InputDeviceSelected(d) => Message::StartupInputDeviceSelected(d),
        AudioSetupAction::OutputDeviceSelected(d) => Message::StartupOutputDeviceSelected(d),
        AudioSetupAction::SampleRateSelected(r) => Message::StartupSampleRateSelected(r),
        AudioSetupAction::BitDepthSelected(b) => Message::StartupBitsSelected(b),
        AudioSetupAction::PeriodFramesSelected(p) => Message::StartupPeriodFramesSelected(p),
        AudioSetupAction::NPeriodsSelected(n) => Message::StartupNPeriodsSelected(n),
        AudioSetupAction::ExclusiveToggled(e) => Message::StartupExclusiveToggled(e),
        AudioSetupAction::SyncModeToggled(s) => Message::StartupSyncModeToggled(s),
        AudioSetupAction::Start => Message::StartupOpen,
    });

    container(content)
        .style(app_style)
        .width(Length::Fill)
        .height(Length::Fill)
        .align_x(maolan_widgets::iced::Alignment::Center)
        .align_y(maolan_widgets::iced::Alignment::Center)
        .into()
}

fn backend_matches_device(engine: AudioEngineOption, device_id: &str) -> bool {
    match engine {
        #[cfg(unix)]
        AudioEngineOption::Jack => false,
        #[cfg(target_os = "freebsd")]
        AudioEngineOption::Oss => device_id.starts_with("/dev/dsp"),
        #[cfg(target_os = "openbsd")]
        AudioEngineOption::Sndio => !device_id.is_empty(),
        #[cfg(target_os = "linux")]
        AudioEngineOption::Alsa => device_id.starts_with("hw:"),
        #[cfg(target_os = "windows")]
        AudioEngineOption::Wasapi => device_id.starts_with("wasapi:"),
    }
}

async fn save_document(
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

async fn choose_export_directory() -> Option<PathBuf> {
    rfd::FileDialog::new().pick_folder()
}

async fn export_marker_ranges(
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

fn marker_ranges(markers: &[(usize, String)], frames: usize) -> Vec<(usize, usize)> {
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

fn marker_range_samples(audio: &AudioDocument, start: usize, end: usize) -> Vec<f32> {
    let channels = audio.channels.max(1);
    let frames = audio.frames();
    let start = start.min(frames);
    let end = end.min(frames);
    if start >= end {
        return Vec::new();
    }
    audio.preview_samples[start * channels..end * channels].to_vec()
}

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

fn export_filename(stem: &str, index: usize, extension: &str) -> String {
    format!("{stem}_{index:03}.{extension}")
}

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

fn detect_markers(
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

impl AudioDocument {
    fn open_with_progress<F>(
        path: PathBuf,
        region: Option<AudioRegion>,
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
        progress_callback(0.78, &format!("Applying preview edits to {name}..."));
        let preview = render_preview_samples(&samples, channels, edits);
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
            markers: Vec::new(),
        })
    }

    fn frames(&self) -> usize {
        self.samples.len() / self.channels.max(1)
    }

    fn rebuild_preview(&mut self) {
        let preview = render_preview_samples(&self.samples, self.channels, self.edits);
        self.preview_samples = preview.clone();
        self.channel_samples = deinterleave(&preview, self.channels);
        self.peak = peak(&preview);
    }

    fn rendered_save_samples(&self) -> Vec<f32> {
        render_preview_samples(&self.samples, self.channels, self.edits)
    }

    fn next_zero_crossing_frame(&self, start_frame: usize) -> Option<usize> {
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

#[derive(Debug, Clone, Copy)]
enum EditOperation {
    FadeIn,
    FadeOut,
    IncreaseVolume,
    DecreaseVolume,
}

fn apply_standalone_edit(app: &mut EditApp, operation: EditOperation) -> Task<Message> {
    let Some(audio) = app.audio.as_mut() else {
        app.status = String::from("No audio file is open.");
        return Task::none();
    };

    let previous_snapshot = DocumentSnapshot {
        samples: audio.samples.clone(),
        edits: audio.edits,
        markers: audio.markers.clone(),
    };

    if let Some((start, end)) = app.selection_samples {
        let frames = audio.frames();
        let start = start.min(frames);
        let end = end.min(frames);
        let length = end.saturating_sub(start);
        if length == 0 {
            app.status = String::from("Selection is empty.");
            return Task::none();
        }
        let region = AudioRegion {
            offset: start,
            length,
        };
        apply_edit_to_samples(&mut audio.samples, audio.channels, region, operation);
        match operation {
            EditOperation::FadeIn => app.status = String::from("Fade in applied to selection."),
            EditOperation::FadeOut => app.status = String::from("Fade out applied to selection."),
            EditOperation::IncreaseVolume | EditOperation::DecreaseVolume => {
                app.status = String::from("Volume adjusted on selection.")
            }
        }
    } else {
        match operation {
            EditOperation::FadeIn => {
                audio.edits.fade_in_samples = default_fade_samples(audio.frames());
                app.status = String::from("Fade in applied to preview.");
            }
            EditOperation::FadeOut => {
                audio.edits.fade_out_samples = default_fade_samples(audio.frames());
                app.status = String::from("Fade out applied to preview.");
            }
            EditOperation::IncreaseVolume => {
                audio.edits.gain_db = (audio.edits.gain_db + 1.0).min(24.0);
                app.status = format!("Preview gain: {:+.1} dB.", audio.edits.gain_db);
            }
            EditOperation::DecreaseVolume => {
                audio.edits.gain_db = (audio.edits.gain_db - 1.0).max(-48.0);
                app.status = format!("Preview gain: {:+.1} dB.", audio.edits.gain_db);
            }
        }
    }
    app.history.record(
        previous_snapshot,
        DocumentSnapshot {
            samples: audio.samples.clone(),
            edits: audio.edits,
            markers: audio.markers.clone(),
        },
    );
    audio.rebuild_preview();
    prepare_document_track(app)
}

fn reverse_clip(app: &mut EditApp) -> Task<Message> {
    let Some(audio) = app.audio.as_mut() else {
        app.status = String::from("No audio file is open.");
        return Task::none();
    };

    let previous_snapshot = DocumentSnapshot {
        samples: audio.samples.clone(),
        edits: audio.edits,
        markers: audio.markers.clone(),
    };
    audio.edits.reversed = !audio.edits.reversed;
    app.status = if audio.edits.reversed {
        String::from("Clip reversed.")
    } else {
        String::from("Clip restored to forward playback.")
    };
    app.history.record(
        previous_snapshot,
        DocumentSnapshot {
            samples: audio.samples.clone(),
            edits: audio.edits,
            markers: audio.markers.clone(),
        },
    );
    audio.rebuild_preview();
    prepare_document_track(app)
}

fn delete_selection(app: &mut EditApp) -> Task<Message> {
    let Some(audio) = app.audio.as_mut() else {
        return Task::none();
    };
    let Some((start, end)) = app.selection_samples else {
        return Task::none();
    };
    if start >= end || end > audio.frames() {
        return Task::none();
    }

    let previous_snapshot = DocumentSnapshot {
        samples: audio.samples.clone(),
        edits: audio.edits,
        markers: audio.markers.clone(),
    };
    let channels = audio.channels.max(1);
    let sample_start = start * channels;
    let sample_end = end.min(audio.frames()) * channels;
    audio.samples.drain(sample_start..sample_end);

    if let Some(region) = audio.clip_region.as_mut() {
        let region_start = region.offset;
        let region_end = region.offset + region.length;
        if end <= region_start {
            region.offset = region.offset.saturating_sub(end - start);
        } else if start >= region_end {
            // Region is entirely before the deleted range; no change.
        } else {
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

    app.selection_anchor_samples = None;
    app.selection_samples = None;
    app.status = format!("Deleted {}..{} samples.", start, end);

    app.history.record(
        previous_snapshot,
        DocumentSnapshot {
            samples: audio.samples.clone(),
            edits: audio.edits,
            markers: audio.markers.clone(),
        },
    );
    audio.rebuild_preview();
    prepare_document_track(app)
}

fn restore_document(audio: &mut AudioDocument, snapshot: DocumentSnapshot) {
    audio.samples = snapshot.samples;
    audio.edits = snapshot.edits;
    audio.markers = snapshot.markers;
}

fn apply_edit_to_samples(
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

fn default_fade_samples(frames: usize) -> usize {
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

fn render_preview_samples(samples: &[f32], channels: usize, edits: AudioEdits) -> Vec<f32> {
    let edited = apply_edits(samples, channels, edits);
    if edits.reversed {
        reverse_samples(&edited, channels)
    } else {
        edited
    }
}

fn apply_edits(samples: &[f32], channels: usize, edits: AudioEdits) -> Vec<f32> {
    let channels = channels.max(1);
    let frames = samples.len() / channels;
    let gain = 10.0f32.powf(edits.gain_db / 20.0);
    let mut output = samples.to_vec();

    for frame in 0..frames {
        let mut envelope = 1.0f32;
        if edits.fade_in_samples > 0 && frame < edits.fade_in_samples {
            envelope *= frame as f32 / edits.fade_in_samples as f32;
        }
        if edits.fade_out_samples > 0 {
            let fade_start = frames.saturating_sub(edits.fade_out_samples);
            if frame >= fade_start {
                envelope *= (frames.saturating_sub(frame) as f32 / edits.fade_out_samples as f32)
                    .clamp(0.0, 1.0);
            }
        }
        for channel in 0..channels {
            let index = frame * channels + channel;
            output[index] = (output[index] * envelope * gain).clamp(-1.0, 1.0);
        }
    }

    output
}

fn peak(samples: &[f32]) -> f32 {
    samples
        .iter()
        .fold(0.0f32, |peak, sample| peak.max(sample.abs()))
}

fn play_standalone(app: &mut EditApp) -> Task<Message> {
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
        async move { start_engine_playback(client, start).await },
        Message::StandalonePlaybackStarted,
    )
}

fn refresh_standalone_playhead(app: &mut EditApp) -> bool {
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

fn playhead_ratio(app: &EditApp) -> Option<f32> {
    let frames = app.audio.as_ref()?.frames().max(1);
    Some((app.playhead_samples as f32 / frames as f32).clamp(0.0, 1.0))
}

fn selection_ratio(app: &EditApp) -> Option<(f32, f32)> {
    let frames = app.audio.as_ref()?.frames().max(1) as f32;
    let (start, end) = app.selection_samples?;
    Some((start as f32 / frames, end as f32 / frames))
}

fn sample_at_ratio(app: &EditApp, ratio: f32) -> Option<usize> {
    let frames = app.audio.as_ref()?.frames();
    Some(((ratio.clamp(0.0, 1.0) * frames as f32).round() as usize).min(frames))
}

fn nearest_marker_sample(audio: &AudioDocument, ratio: f32) -> Option<usize> {
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

fn selection_duration_seconds(app: &EditApp) -> f32 {
    let Some(audio) = app.audio.as_ref() else {
        return 0.0;
    };
    let Some((start, end)) = app.selection_samples else {
        return 0.0;
    };
    end.saturating_sub(start) as f32 / audio.sample_rate.max(1) as f32
}

fn vu_meter(app: &EditApp) -> Element<'_, Message> {
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

fn prepare_document_track(app: &mut EditApp) -> Task<Message> {
    if !app.standalone_ready {
        return Task::none();
    }
    let (Some(audio), Some(playback)) = (app.audio.as_ref(), app.engine_playback.as_ref()) else {
        return Task::none();
    };
    let client = playback.client.clone();
    let path = audio.source_path.clone();
    let samples = apply_edits(&audio.samples, audio.channels, audio.edits);
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
        preview_path(&path)
    } else {
        path.clone()
    });
    app.preparing_playback = true;
    app.status = String::from("Preparing audio for playback...");
    let request = EngineDocumentRequest {
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
        async move { prepare_engine_document(client, request).await },
        Message::EngineDocumentPrepared,
    )
}

async fn open_standalone_engine(setup: StartupSetup) -> Result<EngineClient, String> {
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
    wait_for_engine_response(rx, |action| {
        matches!(
            action,
            EngineAction::Lv2Plugins(_) | EngineAction::Lv2PluginsUnavailable { .. }
        )
    })
    .await?;
    wait_for_engine_response(rx, |action| {
        matches!(
            action,
            EngineAction::Vst3Plugins(_) | EngineAction::Vst3PluginsUnavailable { .. }
        )
    })
    .await?;
    wait_for_engine_response(rx, |action| {
        matches!(
            action,
            EngineAction::ClapPlugins(_) | EngineAction::ClapPluginsUnavailable { .. }
        )
    })
    .await?;
    Ok(())
}

async fn prepare_engine_document(
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

async fn start_engine_playback(client: EngineClient, start: usize) -> Result<(), String> {
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
            .unwrap_or_else(|| default_audio_device(setup.audio_engine).to_string())
    }
}

fn selected_input_device(setup: &StartupSetup) -> Option<String> {
    if setup.audio_engine.is_jack() {
        None
    } else {
        setup.input_device.as_ref().map(|device| device.id.clone())
    }
}

fn selected_bits(setup: &StartupSetup) -> i32 {
    if setup.audio_engine.is_jack() {
        32
    } else {
        setup.bits as i32
    }
}

fn selected_period_frames(setup: &StartupSetup) -> usize {
    let options = period_frame_options(setup);
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

fn period_frame_options(setup: &StartupSetup) -> Vec<usize> {
    #[cfg(target_os = "freebsd")]
    {
        if !setup.audio_engine.is_jack()
            && let Some(device) = setup.output_device.as_ref()
            && let Some(options) = oss_period_frame_options(device, selected_bits(setup) as usize)
        {
            return options;
        }
    }
    vec![
        16, 32, 64, 128, 256, 512, 1024, 2048, 4096, 8192, 16384, 32768, 65536,
    ]
}

#[cfg(target_os = "freebsd")]
fn oss_period_frame_options(device: &AudioDeviceOption, bits: usize) -> Option<Vec<usize>> {
    if device.max_channels == 0 || device.max_buffer_bytes == 0 {
        return None;
    }
    let channels = device.max_channels.max(1);
    let bytes_per_sample = match bits {
        8 => 1,
        16 => 2,
        24 => 3,
        32 => 4,
        _ => return None,
    };
    let frame_bytes = channels.checked_mul(bytes_per_sample)?.max(1);
    let min_bytes = frame_bytes.next_power_of_two();
    let max_fragment_bytes = 1_usize << 16;
    let max_bytes = device
        .max_buffer_bytes
        .min(max_fragment_bytes)
        .max(min_bytes);
    if min_bytes > max_bytes {
        return None;
    }
    let mut out = Vec::new();
    let mut bytes = min_bytes;
    while bytes <= max_bytes {
        out.push(bytes.div_ceil(frame_bytes).max(1));
        match bytes.checked_mul(2) {
            Some(next) => bytes = next,
            None => break,
        }
    }
    out.sort_unstable();
    out.dedup();
    (!out.is_empty()).then_some(out)
}

async fn send_engine(client: &EngineClient, action: EngineAction) -> Result<(), String> {
    client.send(EngineMessage::Request(action)).await
}

async fn wait_for_engine_response(
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

fn preview_path(source: &Path) -> PathBuf {
    let mut path = std::env::temp_dir();
    let stem = source
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or("maolan-editor-preview");
    path.push(format!("maolan-editor-preview-{stem}.wav"));
    path
}

fn playhead_label(app: &EditApp) -> String {
    let sample_rate = app
        .audio
        .as_ref()
        .map(|audio| audio.sample_rate)
        .unwrap_or(48_000)
        .max(1);
    let seconds = app.playhead_samples as f64 / sample_rate as f64;
    let minutes = (seconds / 60.0).floor() as u64;
    let secs = (seconds % 60.0).floor() as u64;
    let millis = ((seconds.fract()) * 1000.0).floor() as u64;
    format!("{minutes:02}:{secs:02}.{millis:03}")
}

fn discover_output_audio_devices(engine: AudioEngineOption) -> Vec<AudioDeviceOption> {
    if engine.is_jack() {
        return vec![simple_audio_device("jack")];
    }
    let mut devices = platform_audio_devices()
        .into_iter()
        .filter(|device| device.supports_output)
        .collect::<Vec<_>>();
    if devices.is_empty() {
        devices.push(simple_audio_device(default_audio_device(engine)));
    }
    devices.sort_by_key(|device| device.label.to_lowercase());
    devices.dedup_by(|a, b| a.id == b.id);
    devices
}

fn discover_input_audio_devices(engine: AudioEngineOption) -> Vec<AudioDeviceOption> {
    if engine.is_jack() {
        return Vec::new();
    }
    let mut devices = platform_audio_devices()
        .into_iter()
        .filter(|device| device.supports_input)
        .collect::<Vec<_>>();
    devices.sort_by_key(|device| device.label.to_lowercase());
    devices.dedup_by(|a, b| a.id == b.id);
    devices
}

fn platform_audio_devices() -> Vec<AudioDeviceOption> {
    #[cfg(target_os = "freebsd")]
    {
        maolan_engine::audio_devices::discover_freebsd_audio_devices()
            .into_iter()
            .map(AudioDeviceOption::from)
            .collect()
    }
    #[cfg(target_os = "linux")]
    {
        let mut output_devices = platform_linux::discover_alsa_output_devices();
        let mut input_devices = platform_linux::discover_alsa_input_devices();
        output_devices.append(&mut input_devices);
        output_devices.sort_by_key(|device| device.label.to_lowercase());
        output_devices.dedup_by(|a, b| a.id == b.id);
        output_devices
    }
    #[cfg(target_os = "openbsd")]
    {
        vec![simple_audio_device("default")]
    }
    #[cfg(target_os = "windows")]
    {
        vec![simple_audio_device("default")]
    }
    #[cfg(not(any(
        target_os = "linux",
        target_os = "freebsd",
        target_os = "openbsd",
        target_os = "windows"
    )))]
    {
        vec![simple_audio_device("default")]
    }
}

#[cfg(target_os = "linux")]
mod platform_linux {
    use alsa::{
        Direction,
        pcm::{Access, Format, HwParams, PCM},
    };

    const SAMPLE_RATE_CANDIDATES: [u32; 12] = [
        8_000, 11_025, 16_000, 22_050, 32_000, 44_100, 48_000, 88_200, 96_000, 176_400, 192_000,
        384_000,
    ];

    fn read_alsa_card_labels() -> std::collections::HashMap<u32, String> {
        let mut labels = std::collections::HashMap::new();
        let Ok(contents) = std::fs::read_to_string("/proc/asound/cards") else {
            return labels;
        };
        for line in contents.lines() {
            let line = line.trim_start();
            let Some((num_str, rest)) = line.split_once(' ') else {
                continue;
            };
            let Ok(card) = num_str.parse::<u32>() else {
                continue;
            };
            let Some((_, desc)) = rest.split_once("]:") else {
                continue;
            };
            let desc = desc.trim();
            if !desc.is_empty() {
                labels.insert(card, desc.to_string());
            }
        }
        labels
    }

    fn probe_alsa_supported_bits(device: &str, direction: Direction) -> Vec<usize> {
        let Ok(pcm) = PCM::new(device, direction, false) else {
            return Vec::new();
        };
        let Ok(hwp) = HwParams::any(&pcm) else {
            return Vec::new();
        };
        if hwp.set_access(Access::RWInterleaved).is_err() {
            return Vec::new();
        }

        fn supports(hwp: &HwParams<'_>, fmt: Format) -> bool {
            hwp.test_format(fmt).is_ok()
        }

        let candidates: Vec<(usize, Vec<Format>)> = vec![
            (32, vec![native_s32(), foreign_s32()]),
            (24, vec![native_s24(), foreign_s24()]),
            (16, vec![native_s16(), foreign_s16()]),
            (8, vec![Format::S8]),
        ];

        let mut supported = Vec::new();
        for (bits, formats) in candidates {
            if formats.iter().any(|f| supports(&hwp, *f)) {
                supported.push(bits);
            }
        }
        supported
    }

    fn probe_alsa_supported_sample_rates(device: &str, direction: Direction) -> Vec<i32> {
        let Ok(pcm) = PCM::new(device, direction, false) else {
            return Vec::new();
        };
        let Ok(hwp) = HwParams::any(&pcm) else {
            return Vec::new();
        };
        if hwp.set_access(Access::RWInterleaved).is_err() {
            return Vec::new();
        }

        let mut supported = Vec::new();
        for rate in SAMPLE_RATE_CANDIDATES {
            if hwp.test_rate(rate).is_ok() {
                supported.push(rate as i32);
            }
        }
        supported
    }

    #[cfg(target_endian = "little")]
    fn native_s16() -> Format {
        Format::S16LE
    }
    #[cfg(target_endian = "big")]
    fn native_s16() -> Format {
        Format::S16BE
    }
    #[cfg(target_endian = "little")]
    fn foreign_s16() -> Format {
        Format::S16BE
    }
    #[cfg(target_endian = "big")]
    fn foreign_s16() -> Format {
        Format::S16LE
    }

    #[cfg(target_endian = "little")]
    fn native_s24() -> Format {
        Format::S24LE
    }
    #[cfg(target_endian = "big")]
    fn native_s24() -> Format {
        Format::S24BE
    }
    #[cfg(target_endian = "little")]
    fn foreign_s24() -> Format {
        Format::S24BE
    }
    #[cfg(target_endian = "big")]
    fn foreign_s24() -> Format {
        Format::S24LE
    }

    #[cfg(target_endian = "little")]
    fn native_s32() -> Format {
        Format::S32LE
    }
    #[cfg(target_endian = "big")]
    fn native_s32() -> Format {
        Format::S32BE
    }
    #[cfg(target_endian = "little")]
    fn foreign_s32() -> Format {
        Format::S32BE
    }
    #[cfg(target_endian = "big")]
    fn foreign_s32() -> Format {
        Format::S32LE
    }

    fn discover_alsa_devices(
        direction_marker: &str,
        direction: Direction,
    ) -> Vec<super::AudioDeviceOption> {
        let mut devices = Vec::new();
        let card_labels = read_alsa_card_labels();
        if let Ok(contents) = std::fs::read_to_string("/proc/asound/pcm") {
            for line in contents.lines() {
                let Some((card_dev, rest)) = line.split_once(':') else {
                    continue;
                };
                if !rest.contains(direction_marker) {
                    continue;
                }
                let mut parts = card_dev.trim().split('-');
                let (Some(card), Some(dev)) = (parts.next(), parts.next()) else {
                    continue;
                };
                let Ok(card) = card.parse::<u32>() else {
                    continue;
                };
                let Ok(dev) = dev.parse::<u32>() else {
                    continue;
                };
                let device_name = rest.split(':').next().unwrap_or("").trim();
                let card_label = card_labels
                    .get(&card)
                    .cloned()
                    .unwrap_or_else(|| format!("Card {card}"));
                let base_label = if device_name.is_empty() {
                    card_label
                } else {
                    format!("{card_label} - {device_name}")
                };
                let id = format!("hw:{card},{dev}");
                let label = format!("{base_label} (hw:{card},{dev})");
                let supported_bits = probe_alsa_supported_bits(&id, direction);
                let supported_sample_rates = {
                    let rates = probe_alsa_supported_sample_rates(&id, direction);
                    if rates.is_empty() {
                        super::fallback_sample_rates()
                    } else {
                        rates
                    }
                };
                let (supports_input, supports_output) = match direction {
                    Direction::Playback => (false, true),
                    Direction::Capture => (true, false),
                };
                devices.push(super::AudioDeviceOption::with_supported_direction_caps(
                    id,
                    label,
                    supported_bits,
                    supported_sample_rates,
                    supports_input,
                    supports_output,
                ));
            }
        }
        devices.sort_by_key(|a| a.label.to_lowercase());
        devices.dedup_by(|a, b| a.id == b.id);
        devices
    }

    pub(crate) fn discover_alsa_output_devices() -> Vec<super::AudioDeviceOption> {
        discover_alsa_devices("playback", Direction::Playback)
    }

    pub(crate) fn discover_alsa_input_devices() -> Vec<super::AudioDeviceOption> {
        discover_alsa_devices("capture", Direction::Capture)
    }
}

fn simple_audio_device(id: impl Into<String>) -> AudioDeviceOption {
    let id = id.into();
    AudioDeviceOption::with_supported_caps(
        id.clone(),
        id,
        vec![32, 24, 16, 8],
        fallback_sample_rates(),
    )
}

fn fallback_sample_rates() -> Vec<i32> {
    vec![
        8_000, 11_025, 16_000, 22_050, 32_000, 44_100, 48_000, 88_200, 96_000, 176_400, 192_000,
        384_000,
    ]
}

fn fallback_bits() -> Vec<usize> {
    vec![32, 24, 16, 8]
}

fn sample_rate_options(setup: &StartupSetup) -> Vec<i32> {
    if setup.audio_engine.is_jack() {
        return fallback_sample_rates();
    }
    setup
        .output_device
        .as_ref()
        .map(|device| device.supported_sample_rates.clone())
        .filter(|rates| !rates.is_empty())
        .unwrap_or_else(fallback_sample_rates)
}

fn bit_options(setup: &StartupSetup) -> Vec<usize> {
    if setup.audio_engine.is_jack() {
        return fallback_bits();
    }
    setup
        .output_device
        .as_ref()
        .map(|device| {
            if device.supported_bits.is_empty() {
                fallback_bits()
            } else {
                device.supported_bits.clone()
            }
        })
        .unwrap_or_else(fallback_bits)
}

fn default_audio_device(engine: AudioEngineOption) -> &'static str {
    if engine.is_jack() {
        return "jack";
    }
    #[cfg(target_os = "linux")]
    {
        "default"
    }
    #[cfg(target_os = "freebsd")]
    {
        "/dev/dsp"
    }
    #[cfg(target_os = "openbsd")]
    {
        "default"
    }
    #[cfg(target_os = "windows")]
    {
        "default"
    }
    #[cfg(not(any(
        target_os = "linux",
        target_os = "freebsd",
        target_os = "openbsd",
        target_os = "windows"
    )))]
    {
        "default"
    }
}

async fn open_audio_dialog() -> Option<PathBuf> {
    rfd::FileDialog::new()
        .add_filter(
            "Audio",
            &["wav", "flac", "mp3", "ogg", "vorbis", "m4a", "aac", "alac"],
        )
        .pick_file()
}

async fn save_audio_dialog(current: Option<PathBuf>) -> Option<PathBuf> {
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

async fn close_confirmation_dialog() -> rfd::MessageDialogResult {
    rfd::AsyncMessageDialog::new()
        .set_title("Unsaved Changes")
        .set_description("You have unsaved changes. Save before closing?")
        .set_buttons(rfd::MessageButtons::YesNoCancel)
        .show()
        .await
}

fn clip_samples(samples: &[f32], channels: usize, region: Option<AudioRegion>) -> Vec<f32> {
    let channels = channels.max(1);
    let Some(region) = region else {
        return samples.to_vec();
    };
    let frames = samples.len() / channels;
    let start = region.offset.min(frames);
    let end = start.saturating_add(region.length).min(frames);
    samples[start * channels..end * channels].to_vec()
}

fn deinterleave(samples: &[f32], channels: usize) -> Vec<Vec<f32>> {
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

fn encode_format_for_path(path: &Path) -> Result<AudioEncodeFormat, String> {
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

fn app_style(_theme: &Theme) -> container::Style {
    container::Style {
        background: Some(Color::from_rgb(0.055, 0.06, 0.075).into()),
        text_color: Some(Color::from_rgb(0.88, 0.90, 0.94)),
        ..container::Style::default()
    }
}

fn panel_style(_theme: &Theme) -> container::Style {
    container::Style {
        border: maolan_widgets::iced::Border {
            color: Color::from_rgb(0.18, 0.20, 0.24),
            width: 1.0,
            radius: 4.0.into(),
        },
        ..container::Style::default()
    }
}

fn toolbar_style(_theme: &Theme) -> container::Style {
    container::Style {
        background: Some(Background::Color(Color::from_rgb(0.075, 0.08, 0.095))),
        border: Border {
            color: Color::from_rgb(0.16, 0.18, 0.22),
            width: 1.0,
            radius: 2.0.into(),
        },
        ..container::Style::default()
    }
}

fn playhead_style(_theme: &Theme) -> container::Style {
    container::Style {
        text_color: Some(Color::from_rgb(0.92, 0.92, 0.92)),
        background: Some(Background::Color(Color::from_rgb(0.10, 0.10, 0.115))),
        border: Border {
            color: Color::from_rgb(0.22, 0.24, 0.28),
            width: 1.0,
            radius: 2.0.into(),
        },
        ..container::Style::default()
    }
}

fn playhead_active_style(_theme: &Theme) -> container::Style {
    container::Style {
        text_color: Some(Color::from_rgb(0.92, 0.98, 0.92)),
        background: Some(Background::Color(Color::from_rgb(0.10, 0.16, 0.12))),
        border: Border {
            color: Color::from_rgb(0.22, 0.45, 0.26),
            width: 1.0,
            radius: 2.0.into(),
        },
        ..container::Style::default()
    }
}

fn toolbar_button_style(theme: &Theme, status: button::Status) -> button::Style {
    let mut style = button::secondary(theme, status);
    style.border.radius = 3.0.into();
    style.border.width = 1.0;
    style.border.color = Color::from_rgb(0.18, 0.20, 0.24);
    style.text_color = Color::from_rgb(0.92, 0.92, 0.92);
    style.background = Some(Background::Color(Color::TRANSPARENT));
    style
}

fn tooltip_style(_theme: &Theme) -> container::Style {
    container::Style {
        text_color: Some(Color::from_rgb(0.94, 0.94, 0.94)),
        background: Some(Background::Color(Color::from_rgba(0.08, 0.08, 0.08, 0.96))),
        border: Border {
            color: Color::from_rgba(0.32, 0.32, 0.32, 1.0),
            width: 1.0,
            radius: 3.0.into(),
        },
        ..container::Style::default()
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
                Some(AudioRegion {
                    offset: 1,
                    length: 2
                })
            ),
            vec![2.0, 20.0, 3.0, 30.0]
        );
    }

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

    #[test]
    fn history_tracks_dirty_state_against_save_point() {
        let mut history = EditHistory::new(DocumentSnapshot {
            samples: vec![1.0f32],
            edits: AudioEdits::default(),
            markers: Vec::new(),
        });
        assert!(!history.is_dirty());

        history.record(
            DocumentSnapshot {
                samples: vec![1.0f32],
                edits: AudioEdits::default(),
                markers: Vec::new(),
            },
            DocumentSnapshot {
                samples: vec![2.0f32],
                edits: AudioEdits::default(),
                markers: Vec::new(),
            },
        );
        assert!(history.is_dirty());

        history.mark_saved();
        assert!(!history.is_dirty());
    }

    #[test]
    fn history_undo_redo_restores_states() {
        let mut history = EditHistory::new(DocumentSnapshot {
            samples: vec![1.0f32],
            edits: AudioEdits::default(),
            markers: Vec::new(),
        });
        history.record(
            DocumentSnapshot {
                samples: vec![1.0f32],
                edits: AudioEdits::default(),
                markers: Vec::new(),
            },
            DocumentSnapshot {
                samples: vec![2.0f32],
                edits: AudioEdits::default(),
                markers: Vec::new(),
            },
        );
        history.record(
            DocumentSnapshot {
                samples: vec![2.0f32],
                edits: AudioEdits::default(),
                markers: Vec::new(),
            },
            DocumentSnapshot {
                samples: vec![3.0f32],
                edits: AudioEdits::default(),
                markers: Vec::new(),
            },
        );

        let undone = history.undo().expect("can undo");
        assert_eq!(undone.samples, vec![2.0f32]);

        let undone_again = history.undo().expect("can undo again");
        assert_eq!(undone_again.samples, vec![1.0f32]);
        assert!(history.undo().is_none());

        let redone = history.redo().expect("can redo");
        assert_eq!(redone.samples, vec![2.0f32]);
    }

    #[test]
    fn history_record_clears_redo_stack() {
        let mut history = EditHistory::new(DocumentSnapshot {
            samples: vec![1.0f32],
            edits: AudioEdits::default(),
            markers: Vec::new(),
        });
        history.record(
            DocumentSnapshot {
                samples: vec![1.0f32],
                edits: AudioEdits::default(),
                markers: Vec::new(),
            },
            DocumentSnapshot {
                samples: vec![2.0f32],
                edits: AudioEdits::default(),
                markers: Vec::new(),
            },
        );
        history.undo();
        history.record(
            DocumentSnapshot {
                samples: vec![1.0f32],
                edits: AudioEdits::default(),
                markers: Vec::new(),
            },
            DocumentSnapshot {
                samples: vec![3.0f32],
                edits: AudioEdits::default(),
                markers: Vec::new(),
            },
        );
        assert!(history.redo().is_none());
    }

    fn test_document(preview: Vec<f32>, channels: usize) -> AudioDocument {
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
            markers: Vec::new(),
        }
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
    fn marker_ranges_split_at_sorted_markers() {
        let markers = vec![(50, "A".to_string()), (20, "B".to_string())];
        assert_eq!(
            marker_ranges(&markers, 100),
            vec![(0, 20), (20, 50), (50, 100)]
        );
    }

    #[test]
    fn marker_ranges_ignores_out_of_bounds_markers() {
        let markers = vec![(150, "A".to_string())];
        assert_eq!(marker_ranges(&markers, 100), vec![(0, 100)]);
    }

    #[test]
    fn marker_range_samples_extracts_interleaved_range() {
        let audio = test_document(vec![1.0f32, 2.0, 3.0, 4.0, 5.0, 6.0], 2);
        assert_eq!(marker_range_samples(&audio, 0, 2), vec![1.0, 2.0, 3.0, 4.0]);
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
        let mut audio = test_document(vec![0.5f32; 48_000], 1);
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

    #[test]
    fn preferences_save_to_path_preserves_other_keys() {
        let dir = std::env::temp_dir().join(format!(
            "maolan-edit-prefs-preserve-{}",
            std::time::UNIX_EPOCH.elapsed().unwrap().as_secs()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        let config_path = dir.join("config.toml");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            &config_path,
            "existing_key = \"keep me\"\ndefault_output_device_id = \"old\"\n",
        )
        .unwrap();

        let preferences = EditorPreferences {
            default_output_device_id: Some(String::from("new_out")),
            default_input_device_id: Some(String::from("new_in")),
        };
        preferences.save_to_path(&config_path).unwrap();

        let saved = std::fs::read_to_string(&config_path).unwrap();
        assert!(saved.contains("existing_key = \"keep me\""));
        assert!(saved.contains("default_output_device_id = \"new_out\""));
        assert!(saved.contains("default_input_device_id = \"new_in\""));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn preferences_save_removes_empty_ids() {
        let dir = std::env::temp_dir().join(format!(
            "maolan-edit-prefs-remove-{}",
            std::time::UNIX_EPOCH.elapsed().unwrap().as_secs()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        let config_path = dir.join("config.toml");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            &config_path,
            "default_output_device_id = \"old\"\ndefault_input_device_id = \"old\"\n",
        )
        .unwrap();

        let preferences = EditorPreferences {
            default_output_device_id: None,
            default_input_device_id: None,
        };
        preferences.save_to_path(&config_path).unwrap();

        let saved = std::fs::read_to_string(&config_path).unwrap();
        assert!(!saved.contains("default_output_device_id"));
        assert!(!saved.contains("default_input_device_id"));
        let _ = std::fs::remove_dir_all(&dir);
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
            preferences_dialog: Some(PreferencesDialog::from_setup(&StartupSetup::default())),
            ..EditApp::default()
        };
        let _ = update(&mut app, Message::PreferencesCancel);
        assert!(app.preferences_dialog.is_none());
    }

    #[test]
    fn audio_device_option_equality_compares_id_only() {
        let a = AudioDeviceOption::with_supported_caps("dev", "A", vec![16], vec![44_100]);
        let b = AudioDeviceOption::with_supported_caps("dev", "B", vec![32], vec![48_000]);
        assert_eq!(a, b);
    }

    #[cfg(target_os = "freebsd")]
    #[test]
    fn audio_setup_state_selects_saved_device_from_discovered_list() {
        let mut app = EditApp::default();
        app.setup.audio_engine = AudioEngineOption::Oss;
        app.setup.output_devices = vec![AudioDeviceOption::with_oss_caps(
            "/dev/dsp0",
            "Out",
            vec![16, 24, 32],
            vec![44_100, 48_000],
            2,
            65_536,
        )];
        app.setup.output_device = Some(AudioDeviceOption::with_oss_caps(
            "/dev/dsp0",
            "Out",
            vec![32],
            vec![48_000],
            2,
            65_536,
        ));
        app.setup.input_devices = vec![AudioDeviceOption::with_oss_caps(
            "/dev/dsp1",
            "In",
            vec![16, 24, 32],
            vec![44_100, 48_000],
            2,
            65_536,
        )];
        app.setup.input_device = Some(AudioDeviceOption::with_oss_caps(
            "/dev/dsp1",
            "In",
            vec![32],
            vec![48_000],
            2,
            65_536,
        ));

        let state = audio_setup_state(&app);

        assert_eq!(
            state.selected_output_device.as_ref().map(|d| d.id.as_str()),
            Some("/dev/dsp0")
        );
        assert_eq!(
            state.selected_input_device.as_ref().map(|d| d.id.as_str()),
            Some("/dev/dsp1")
        );
        assert_eq!(
            state.selected_output_device,
            Some(app.setup.output_devices[0].clone())
        );
        assert_eq!(
            state.selected_input_device,
            Some(app.setup.input_devices[0].clone())
        );
    }

    #[test]
    fn preferences_load_from_path_reads_device_ids() {
        let dir = std::env::temp_dir().join(format!(
            "maolan-edit-prefs-load-{}",
            std::time::UNIX_EPOCH.elapsed().unwrap().as_secs()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let config_path = dir.join("edit.toml");
        let contents =
            "default_output_device_id = \"/dev/dsp2\"\ndefault_input_device_id = \"/dev/dsp3\"\n";
        std::fs::write(&config_path, contents).unwrap();

        let preferences = EditorPreferences::load_from_path(&config_path);

        assert_eq!(
            preferences.default_output_device_id.as_deref(),
            Some("/dev/dsp2")
        );
        assert_eq!(
            preferences.default_input_device_id.as_deref(),
            Some("/dev/dsp3")
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn startup_setup_with_preferences_selects_saved_devices() {
        let preferences = EditorPreferences {
            default_output_device_id: Some(String::from("out2")),
            default_input_device_id: Some(String::from("in2")),
        };
        let output_devices = vec![
            AudioDeviceOption::with_supported_caps("out1", "Out One", vec![32], vec![48_000]),
            AudioDeviceOption::with_supported_caps("out2", "Out Two", vec![32], vec![48_000]),
        ];
        let input_devices = vec![
            AudioDeviceOption::with_supported_caps("in1", "In One", vec![32], vec![48_000]),
            AudioDeviceOption::with_supported_caps("in2", "In Two", vec![32], vec![48_000]),
        ];

        let setup = StartupSetup::with_preferences(&preferences, output_devices, input_devices);

        assert_eq!(
            setup.output_device.as_ref().map(|d| d.id.as_str()),
            Some("out2")
        );
        assert_eq!(
            setup.input_device.as_ref().map(|d| d.id.as_str()),
            Some("in2")
        );
    }

    #[test]
    fn startup_setup_with_preferences_falls_back_to_first_device() {
        let preferences = EditorPreferences {
            default_output_device_id: Some(String::from("missing")),
            default_input_device_id: Some(String::from("missing")),
        };
        let output_devices = vec![AudioDeviceOption::with_supported_caps(
            "out1",
            "Out One",
            vec![32],
            vec![48_000],
        )];
        let input_devices = vec![AudioDeviceOption::with_supported_caps(
            "in1",
            "In One",
            vec![32],
            vec![48_000],
        )];

        let setup = StartupSetup::with_preferences(&preferences, output_devices, input_devices);

        assert_eq!(
            setup.output_device.as_ref().map(|d| d.id.as_str()),
            Some("out1")
        );
        assert_eq!(
            setup.input_device.as_ref().map(|d| d.id.as_str()),
            Some("in1")
        );
    }
}
