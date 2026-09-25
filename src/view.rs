use maolan_widgets::iced::{
    Background, Border, Color, Element, Length, Theme,
    widget::{Space, button, column, container, progress_bar, row, text, tooltip},
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
};

use crate::devices::{
    AudioDeviceOption, AudioEngineOption, bit_options, playhead_label, sample_rate_options,
};
use crate::dialogs::{
    detect_markers_dialog_view, export_markers_dialog_view, marker_dialog_view,
    preferences_dialog_view,
};
use crate::markers::nearest_marker_sample;
use crate::message::Message;
use crate::state::EditApp;
use crate::transport::{playhead_ratio, sample_at_ratio, selection_ratio, vu_meter};

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

pub(crate) fn progress_view(progress: f32) -> Element<'static, Message> {
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

pub(crate) fn audio_setup_state(
    app: &EditApp,
) -> AudioSetupState<AudioEngineOption, AudioDeviceOption> {
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
    let period_frames = crate::devices::period_frame_options(&app.setup);
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

#[cfg(all(test, feature = "standalone", target_os = "freebsd"))]
mod tests {
    use super::*;

    #[cfg(all(feature = "standalone", target_os = "freebsd"))]
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
}
