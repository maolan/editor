use std::time::Duration;

use maolan_widgets::iced::{Subscription, keyboard, time, window};

use crate::message::Message;
use crate::state::EditApp;

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
