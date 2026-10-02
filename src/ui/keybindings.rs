use super::config::app_data_dir;
use eframe::egui;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::PathBuf;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Action {
    PlayPause,
    Stop,
    FitAll,
    NudgeLeft,
    NudgeRight,
    DeleteClip,
    Split,
    PrevMarker,
    NextMarker,
    AddMarker,
    ToggleSnap,
    ZoomIn,
    ZoomOut,
    Undo,
    Redo,
}

impl Action {
    pub const ALL: [Action; 15] = [
        Action::PlayPause,
        Action::Stop,
        Action::FitAll,
        Action::NudgeLeft,
        Action::NudgeRight,
        Action::DeleteClip,
        Action::Split,
        Action::PrevMarker,
        Action::NextMarker,
        Action::AddMarker,
        Action::ToggleSnap,
        Action::ZoomIn,
        Action::ZoomOut,
        Action::Undo,
        Action::Redo,
    ];

    pub fn loc_key(self) -> &'static str {
        match self {
            Action::PlayPause => "keys.act.playPause",
            Action::Stop => "keys.act.stop",
            Action::FitAll => "keys.act.fitAll",
            Action::NudgeLeft => "keys.act.nudgeLeft",
            Action::NudgeRight => "keys.act.nudgeRight",
            Action::DeleteClip => "keys.act.deleteClip",
            Action::Split => "keys.act.split",
            Action::PrevMarker => "keys.act.prevMarker",
            Action::NextMarker => "keys.act.nextMarker",
            Action::AddMarker => "keys.act.addMarker",
            Action::ToggleSnap => "keys.act.toggleSnap",
            Action::ZoomIn => "keys.act.zoomIn",
            Action::ZoomOut => "keys.act.zoomOut",
            Action::Undo => "keys.act.undo",
            Action::Redo => "keys.act.redo",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Chord {
    pub key: egui::Key,
    pub ctrl: bool,
    pub shift: bool,
    pub alt: bool,
}

impl Chord {
    pub fn plain(key: egui::Key) -> Self {
        Self {
            key,
            ctrl: false,
            shift: false,
            alt: false,
        }
    }
    pub fn ctrl(key: egui::Key) -> Self {
        Self {
            key,
            ctrl: true,
            shift: false,
            alt: false,
        }
    }
    pub fn ctrl_shift(key: egui::Key) -> Self {
        Self {
            key,
            ctrl: true,
            shift: true,
            alt: false,
        }
    }

    pub fn consume(&self, i: &mut egui::InputState, allow_repeat: bool) -> bool {
        let mut handled = false;
        let mut consumed = false;
        i.events.retain(|event| {
            if let egui::Event::Key {
                key,
                pressed: true,
                repeat,
                modifiers,
                ..
            } = event
            {
                if *key == self.key
                    && modifiers.ctrl == self.ctrl
                    && modifiers.shift == self.shift
                    && modifiers.alt == self.alt
                    && !modifiers.mac_cmd
                {
                    handled |= allow_repeat || !repeat;
                    consumed = true;
                    return false;
                }
            }
            true
        });
        if consumed && self.key == egui::Key::Space {
            i.events
                .retain(|event| !matches!(event, egui::Event::Text(text) if text == " "));
        }
        handled
    }

    pub fn to_display(self) -> String {
        let mut s = String::new();
        if self.ctrl {
            s.push_str("Ctrl+");
        }
        if self.alt {
            s.push_str("Alt+");
        }
        if self.shift {
            s.push_str("Shift+");
        }
        s.push_str(&key_name(self.key));
        s
    }

    pub fn parse(text: &str) -> Option<Chord> {
        let mut ctrl = false;
        let mut shift = false;
        let mut alt = false;
        let text = text.trim();
        let (prefix, name) = if let Some(prefix) = text.strip_suffix('+') {
            (prefix.strip_suffix('+').unwrap_or(prefix), "+")
        } else {
            text.rsplit_once('+').unwrap_or(("", text))
        };
        for part in prefix
            .split('+')
            .map(str::trim)
            .filter(|part| !part.is_empty())
        {
            match part.to_ascii_lowercase().as_str() {
                "ctrl" | "control" => ctrl = true,
                "shift" => shift = true,
                "alt" => alt = true,
                _ => return None,
            }
        }
        key_from_name(name.trim()).map(|key| Chord {
            key,
            ctrl,
            shift,
            alt,
        })
    }
}

fn key_name(k: egui::Key) -> String {
    match k {
        egui::Key::Comma => ",".into(),
        egui::Key::Period => ".".into(),
        egui::Key::Plus => "+".into(),
        egui::Key::Equals => "=".into(),
        egui::Key::Minus => "-".into(),
        other => format!("{other:?}"),
    }
}

fn key_from_name(name: &str) -> Option<egui::Key> {
    egui::Key::from_name(name).or_else(|| {
        egui::Key::ALL
            .iter()
            .copied()
            .find(|key| format!("{key:?}").eq_ignore_ascii_case(name))
    })
}

pub fn text_input_active(ctx: &egui::Context) -> bool {
    ctx.memory(|memory| memory.focused())
        .is_some_and(|id| egui::TextEdit::load_state(ctx, id).is_some())
}

pub struct KeyBindings {
    map: BTreeMap<Action, Chord>,
}

fn keys_path() -> PathBuf {
    app_data_dir().join("keys.json")
}

impl Default for KeyBindings {
    fn default() -> Self {
        use egui::Key;
        let mut map = BTreeMap::new();
        map.insert(Action::PlayPause, Chord::plain(Key::Space));
        map.insert(
            Action::Stop,
            Chord {
                key: Key::Space,
                ctrl: false,
                shift: true,
                alt: false,
            },
        );
        map.insert(
            Action::FitAll,
            Chord {
                key: Key::A,
                ctrl: false,
                shift: true,
                alt: false,
            },
        );
        map.insert(Action::NudgeLeft, Chord::plain(Key::ArrowLeft));
        map.insert(Action::NudgeRight, Chord::plain(Key::ArrowRight));
        map.insert(Action::DeleteClip, Chord::plain(Key::Delete));
        map.insert(Action::Split, Chord::plain(Key::S));
        map.insert(Action::PrevMarker, Chord::plain(Key::Comma));
        map.insert(Action::NextMarker, Chord::plain(Key::Period));
        map.insert(Action::AddMarker, Chord::plain(Key::M));
        map.insert(Action::ToggleSnap, Chord::plain(Key::J));
        map.insert(Action::ZoomIn, Chord::plain(Key::Plus));
        map.insert(Action::ZoomOut, Chord::plain(Key::Minus));
        map.insert(Action::Undo, Chord::ctrl(Key::Z));
        map.insert(Action::Redo, Chord::ctrl(Key::Y));
        Self { map }
    }
}

impl KeyBindings {
    pub fn consume_actions(&self, input: &mut egui::InputState) -> Vec<Action> {
        Action::ALL
            .into_iter()
            .filter(|action| {
                let allow_repeat = matches!(
                    action,
                    Action::NudgeLeft | Action::NudgeRight | Action::ZoomIn | Action::ZoomOut
                );
                self.get(*action).consume(input, allow_repeat)
            })
            .collect()
    }

    pub fn load() -> Self {
        let mut kb = KeyBindings::default();
        if let Ok(text) = std::fs::read_to_string(keys_path()) {
            if let Ok(raw) = serde_json::from_str::<BTreeMap<String, String>>(&text) {
                for (k, v) in raw {
                    if let (Some(action), Some(chord)) = (action_from_name(&k), Chord::parse(&v)) {
                        kb.map.insert(action, chord);
                    }
                }
            }
        }
        kb
    }

    pub fn save(&self) {
        let dir = app_data_dir();
        let _ = std::fs::create_dir_all(&dir);
        let raw: BTreeMap<String, String> = self
            .map
            .iter()
            .map(|(a, c)| (format!("{a:?}"), c.to_display()))
            .collect();
        if let Ok(json) = serde_json::to_string_pretty(&raw) {
            let _ = std::fs::write(keys_path(), json);
        }
    }

    pub fn get(&self, a: Action) -> Chord {
        self.map
            .get(&a)
            .copied()
            .unwrap_or_else(|| KeyBindings::default().map[&a])
    }

    pub fn set(&mut self, a: Action, c: Chord) {
        self.map.insert(a, c);
    }

    pub fn reset(&mut self) {
        *self = KeyBindings::default();
    }
}

fn action_from_name(name: &str) -> Option<Action> {
    Action::ALL.into_iter().find(|a| format!("{a:?}") == name)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key_event(key: egui::Key, modifiers: egui::Modifiers, repeat: bool) -> egui::Event {
        egui::Event::Key {
            key,
            physical_key: Some(key),
            pressed: true,
            repeat,
            modifiers,
        }
    }

    fn input(events: Vec<egui::Event>) -> egui::RawInput {
        egui::RawInput {
            events,
            focused: true,
            ..Default::default()
        }
    }

    #[test]
    fn saved_chords_round_trip_every_key_and_modifier() {
        for &key in egui::Key::ALL {
            for mask in 0..8 {
                let chord = Chord {
                    key,
                    ctrl: mask & 1 != 0,
                    shift: mask & 2 != 0,
                    alt: mask & 4 != 0,
                };
                assert_eq!(Chord::parse(&chord.to_display()), Some(chord));
                assert_eq!(
                    Chord::parse(&chord.to_display().to_ascii_lowercase()),
                    Some(chord)
                );
            }
        }
        assert_eq!(Chord::parse("Ctrl+Unknown"), None);
    }

    #[test]
    fn event_modifiers_distinguish_space_and_shift_space() {
        let keys = KeyBindings::default();
        for (modifiers, expected) in [
            (egui::Modifiers::NONE, Action::PlayPause),
            (egui::Modifiers::SHIFT, Action::Stop),
        ] {
            let mut i = egui::InputState::default();
            i.events = vec![
                key_event(egui::Key::Space, modifiers, false),
                egui::Event::Text(" ".into()),
            ];
            assert_eq!(keys.consume_actions(&mut i), vec![expected]);
            assert!(i.events.is_empty());
        }
    }

    #[test]
    fn held_space_is_consumed_without_repeating_playback() {
        let mut i = egui::InputState::default();
        i.events = vec![key_event(egui::Key::Space, egui::Modifiers::NONE, true)];
        assert!(KeyBindings::default().consume_actions(&mut i).is_empty());
        assert!(i.events.is_empty());
        i.events = vec![key_event(egui::Key::ArrowLeft, egui::Modifiers::NONE, true)];
        assert_eq!(
            KeyBindings::default().consume_actions(&mut i),
            vec![Action::NudgeLeft]
        );
    }

    #[test]
    fn custom_home_and_space_bindings_are_consumed() {
        for (action, key) in [
            (Action::PrevMarker, egui::Key::Home),
            (Action::Undo, egui::Key::Space),
        ] {
            let mut keys = KeyBindings::default();
            keys.set(Action::PlayPause, Chord::plain(egui::Key::P));
            keys.set(action, Chord::plain(key));
            let mut i = egui::InputState::default();
            i.events = vec![key_event(key, egui::Modifiers::NONE, false)];
            assert_eq!(keys.consume_actions(&mut i), vec![action]);
            assert!(i.events.is_empty());
        }
    }

    #[test]
    fn shortcut_prevents_focused_button_space_click() {
        for (consume, expected_click) in [(false, true), (true, false)] {
            let ctx = egui::Context::default();
            let _ = ctx.run(input(vec![]), |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    ui.button("Get").request_focus();
                });
            });
            let mut clicked = false;
            let _ = ctx.run(
                input(vec![key_event(
                    egui::Key::Space,
                    egui::Modifiers::NONE,
                    false,
                )]),
                |ctx| {
                    assert!(ctx.wants_keyboard_input());
                    assert!(!text_input_active(ctx));
                    if consume {
                        let actions = ctx.input_mut(|i| KeyBindings::default().consume_actions(i));
                        assert_eq!(actions, vec![Action::PlayPause]);
                    }
                    egui::CentralPanel::default().show(ctx, |ui| {
                        clicked = ui.button("Get").clicked();
                    });
                },
            );
            assert_eq!(clicked, expected_click);
        }
    }

    #[test]
    fn text_edit_retains_home_space_and_undo_events() {
        let ctx = egui::Context::default();
        let id = egui::Id::new("text");
        let mut value = "abc".to_owned();
        let _ = ctx.run(input(vec![]), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                ui.add(egui::TextEdit::singleline(&mut value).id(id))
                    .request_focus();
            });
        });
        let events = vec![
            key_event(egui::Key::Home, egui::Modifiers::NONE, false),
            key_event(egui::Key::Space, egui::Modifiers::NONE, false),
            egui::Event::Text(" ".into()),
        ];
        let _ = ctx.run(input(events), |ctx| {
            assert!(text_input_active(ctx));
            egui::CentralPanel::default().show(ctx, |ui| {
                ui.add(egui::TextEdit::singleline(&mut value).id(id));
            });
        });
        assert_eq!(value, " abc");
        let _ = ctx.run(
            input(vec![key_event(egui::Key::Z, egui::Modifiers::CTRL, false)]),
            |ctx| {
                assert!(text_input_active(ctx));
                assert!(ctx.input(|i| i.key_pressed(egui::Key::Z)));
                egui::CentralPanel::default().show(ctx, |ui| {
                    ui.add(egui::TextEdit::singleline(&mut value).id(id));
                });
            },
        );
    }

    #[test]
    fn clicked_numeric_editor_blocks_timeline_shortcuts() {
        let ctx = egui::Context::default();
        let mut value = 123;
        let mut rect = egui::Rect::NOTHING;
        let _ = ctx.run(input(vec![]), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                rect = ui.add(egui::DragValue::new(&mut value)).rect;
            });
        });
        for pressed in [true, false] {
            let events = vec![
                egui::Event::PointerMoved(rect.center()),
                egui::Event::PointerButton {
                    pos: rect.center(),
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: egui::Modifiers::NONE,
                },
            ];
            let _ = ctx.run(input(events), |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    ui.add(egui::DragValue::new(&mut value));
                });
            });
        }
        let _ = ctx.run(
            input(vec![key_event(
                egui::Key::Home,
                egui::Modifiers::NONE,
                false,
            )]),
            |ctx| {
                assert!(text_input_active(ctx));
                assert!(ctx.input(|i| i.key_pressed(egui::Key::Home)));
                egui::CentralPanel::default().show(ctx, |ui| {
                    ui.add(egui::DragValue::new(&mut value));
                });
            },
        );
        assert_eq!(value, 123);
    }
}
