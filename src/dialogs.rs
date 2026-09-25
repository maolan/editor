use std::fmt;
use std::path::PathBuf;

use maolan_widgets::iced::{
    Background, Border, Color, Element, Length,
    widget::{Id, button, column, container, pick_list, row, text, text_input},
};

use crate::devices::{AudioDeviceOption, StartupSetup};
use crate::message::Message;

#[derive(Debug, Clone)]
pub(crate) struct MarkerDialog {
    pub(crate) sample: usize,
    pub(crate) name: String,
}

#[derive(Debug, Clone)]
pub(crate) struct DetectMarkersDialog {
    pub(crate) threshold_db: String,
    pub(crate) silence_samples: String,
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
    pub(crate) const ALL: &'static [Self] = &[Self::Wav, Self::Flac, Self::OggFlac, Self::Mp3];

    #[cfg(feature = "standalone")]
    pub(crate) fn extension(self) -> &'static str {
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
    pub(crate) const ALL: &'static [Self] = &[Self::Bits16, Self::Bits24, Self::Bits32];

    #[cfg(feature = "standalone")]
    pub(crate) fn bits(self) -> u16 {
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
    pub(crate) const ALL: &'static [Self] = &[
        Self::Hz22050,
        Self::Hz44100,
        Self::Hz48000,
        Self::Hz88200,
        Self::Hz96000,
        Self::Hz192000,
    ];

    pub(crate) fn value(self) -> u32 {
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
pub(crate) struct ExportMarkersDialog {
    pub(crate) directory: Option<PathBuf>,
    pub(crate) format: ExportFormat,
    pub(crate) bit_depth: ExportBitDepth,
    pub(crate) sample_rate: ExportSampleRate,
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
pub(crate) struct PreferencesDialog {
    pub(crate) output_devices: Vec<AudioDeviceOption>,
    pub(crate) input_devices: Vec<AudioDeviceOption>,
    pub(crate) output_device: Option<AudioDeviceOption>,
    pub(crate) input_device: Option<AudioDeviceOption>,
}

impl PreferencesDialog {
    pub(crate) fn from_setup(setup: &StartupSetup) -> Self {
        Self {
            output_devices: setup.output_devices.clone(),
            input_devices: setup.input_devices.clone(),
            output_device: setup.output_device.clone(),
            input_device: setup.input_device.clone(),
        }
    }
}

pub(crate) fn marker_name_input_id() -> Id {
    Id::new("edit-marker-name-input")
}

pub(crate) fn detect_markers_threshold_input_id() -> Id {
    Id::new("edit-detect-markers-threshold-input")
}

pub(crate) fn marker_dialog_view(dialog: &MarkerDialog) -> Element<'_, Message> {
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

pub(crate) fn detect_markers_dialog_view(dialog: &DetectMarkersDialog) -> Element<'_, Message> {
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

pub(crate) fn export_markers_dialog_view(dialog: &ExportMarkersDialog) -> Element<'_, Message> {
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

pub(crate) fn preferences_dialog_view(dialog: &PreferencesDialog) -> Element<'_, Message> {
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

pub(crate) async fn close_confirmation_dialog() -> rfd::MessageDialogResult {
    rfd::AsyncMessageDialog::new()
        .set_title("Unsaved Changes")
        .set_description("You have unsaved changes. Save before closing?")
        .set_buttons(rfd::MessageButtons::YesNoCancel)
        .show()
        .await
}
