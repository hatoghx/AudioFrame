use eframe::egui::{self, emath::Numeric, Event, ImeEvent, Key, Widget};
use std::ops::RangeInclusive;

pub struct Integer<'a> {
    widget: egui::DragValue<'a>,
    signed: bool,
}

impl<'a> Integer<'a> {
    pub fn new<Num: Numeric>(value: &'a mut Num) -> Self {
        Self {
            widget: egui::DragValue::new(value).max_decimals(0),
            signed: Num::MIN.to_f64() < 0.0,
        }
    }

    pub fn range<Num: Numeric>(mut self, range: RangeInclusive<Num>) -> Self {
        self.signed = range.start().to_f64() < 0.0;
        self.widget = self.widget.range(range);
        self
    }

    pub fn speed(mut self, speed: impl Into<f64>) -> Self {
        self.widget = self.widget.speed(speed);
        self
    }

    pub fn suffix(mut self, suffix: impl ToString) -> Self {
        self.widget = self.widget.suffix(suffix);
        self
    }

}

impl Widget for Integer<'_> {
    fn ui(self, ui: &mut egui::Ui) -> egui::Response {
        let id = ui.next_auto_id();
        let editing = ui.memory_mut(|memory| {
            memory.interested_in_focus(id);
            memory.has_focus(id)
        });
        if editing {
            ui.input_mut(|input| filter_events(&mut input.events, self.signed));
        }
        let response = self.widget.ui(ui);
        if ui.memory(|memory| memory.has_focus(id)) {
            ui.output_mut(|output| output.ime = None);
        }
        response
    }
}

pub struct Decimal<'a> {
    widget: egui::DragValue<'a>,
    signed: bool,
}

impl<'a> Decimal<'a> {
    pub fn new(value: &'a mut f32) -> Self {
        Self {
            widget: egui::DragValue::new(value).max_decimals(2),
            signed: true,
        }
    }

    pub fn range(mut self, range: RangeInclusive<f32>) -> Self {
        self.widget = self.widget.range(range);
        self
    }

    pub fn speed(mut self, speed: impl Into<f64>) -> Self {
        self.widget = self.widget.speed(speed);
        self
    }

    pub fn fixed_decimals(mut self, decimals: usize) -> Self {
        self.widget = self.widget.fixed_decimals(decimals);
        self
    }

    pub fn suffix(mut self, suffix: impl ToString) -> Self {
        self.widget = self.widget.suffix(suffix);
        self
    }

    pub fn negative_infinity_at(mut self, threshold: f64) -> Self {
        self.widget = self
            .widget
            .custom_formatter(move |value, _| {
                if value <= threshold {
                    "-inf".to_owned()
                } else {
                    format!("{value:.1}")
                }
            })
            .custom_parser(move |text| {
                let text = text.trim();
                if text == "-∞" || text.eq_ignore_ascii_case("-inf") {
                    Some(threshold)
                } else {
                    text.parse::<f64>().ok()
                }
            });
        self
    }
}

impl Widget for Decimal<'_> {
    fn ui(self, ui: &mut egui::Ui) -> egui::Response {
        let id = ui.next_auto_id();
        let editing = ui.memory_mut(|memory| {
            memory.interested_in_focus(id);
            memory.has_focus(id)
        });
        if editing {
            ui.input_mut(|input| filter_decimal_events(&mut input.events, self.signed));
        }
        let response = self.widget.ui(ui);
        if ui.memory(|memory| memory.has_focus(id)) {
            ui.output_mut(|output| output.ime = None);
        }
        response
    }
}

fn filter_events(events: &mut Vec<Event>, signed: bool) {
    events.retain_mut(|event| {
        if let Event::Ime(ImeEvent::Commit(text)) = event {
            *event = Event::Text(std::mem::take(text));
        }
        match event {
            Event::Text(text) | Event::Paste(text) => {
                text.retain(|ch| ch.is_ascii_digit() || (signed && ch == '-'));
                !text.is_empty()
            }
            Event::Ime(_) => false,
            Event::Key { key, modifiers, .. } => {
                matches!(
                    key,
                    Key::ArrowLeft
                        | Key::ArrowRight
                        | Key::ArrowUp
                        | Key::ArrowDown
                        | Key::Home
                        | Key::End
                        | Key::Backspace
                        | Key::Delete
                        | Key::Enter
                        | Key::Escape
                        | Key::Tab
                ) || (modifiers.command
                    && matches!(key, Key::A | Key::C | Key::V | Key::X | Key::Z | Key::Y))
            }
            _ => true,
        }
    });
}

fn filter_decimal_events(events: &mut Vec<Event>, signed: bool) {
    events.retain_mut(|event| {
        if let Event::Ime(ImeEvent::Commit(text)) = event {
            *event = Event::Text(std::mem::take(text));
        }
        match event {
            Event::Text(text) | Event::Paste(text) => {
                text.retain(|ch| ch.is_ascii_digit() || ch == '.' || (signed && ch == '-'));
                !text.is_empty()
            }
            Event::Ime(_) => false,
            Event::Key { key, modifiers, .. } => {
                matches!(
                    key,
                    Key::ArrowLeft
                        | Key::ArrowRight
                        | Key::ArrowUp
                        | Key::ArrowDown
                        | Key::Home
                        | Key::End
                        | Key::Backspace
                        | Key::Delete
                        | Key::Enter
                        | Key::Escape
                        | Key::Tab
                ) || (modifiers.command
                    && matches!(key, Key::A | Key::C | Key::V | Key::X | Key::Z | Key::Y))
            }
            _ => true,
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(key: Key, modifiers: egui::Modifiers) -> Event {
        Event::Key {
            key,
            physical_key: Some(key),
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers {
                command: modifiers.ctrl,
                ..modifiers
            },
        }
    }

    struct Editor {
        ctx: egui::Context,
        value: i64,
        range: RangeInclusive<i64>,
        id: egui::Id,
        rect: egui::Rect,
    }

    impl Editor {
        fn new(value: i64, range: RangeInclusive<i64>) -> Self {
            let mut editor = Self {
                ctx: egui::Context::default(),
                value,
                range,
                id: egui::Id::NULL,
                rect: egui::Rect::NOTHING,
            };
            editor.frame(vec![]);
            for pressed in [true, false] {
                editor.frame(vec![
                    Event::PointerMoved(editor.rect.center()),
                    Event::PointerButton {
                        pos: editor.rect.center(),
                        button: egui::PointerButton::Primary,
                        pressed,
                        modifiers: egui::Modifiers::NONE,
                    },
                ]);
            }
            editor.frame(vec![]);
            assert!(editor.ctx.memory(|memory| memory.has_focus(editor.id)));
            editor
        }

        fn frame(&mut self, events: Vec<Event>) -> egui::FullOutput {
            self.ctx.run(
                egui::RawInput {
                    events,
                    focused: true,
                    ..Default::default()
                },
                |ctx| {
                    egui::CentralPanel::default().show(ctx, |ui| {
                        let response = ui.add(
                            Integer::new(&mut self.value)
                                .range(self.range.clone())
                                .speed(1.0),
                        );
                        self.id = response.id;
                        self.rect = response.rect;
                    });
                },
            )
        }

        fn text(&self) -> String {
            self.ctx
                .data(|data| data.get_temp::<String>(self.id))
                .unwrap()
        }
    }

    #[test]
    fn invalid_input_never_changes_selected_number_or_display() {
        let mut editor = Editor::new(123, 0..=i32::MAX as i64);
        for text in ["a", " ", ".", "+", "-", "あいう", "\n", "!@#$%"] {
            let output = editor.frame(vec![
                key(Key::Space, egui::Modifiers::NONE),
                Event::Text(text.into()),
            ]);
            assert_eq!(editor.value, 123);
            assert_eq!(editor.text(), "123");
            assert!(output.platform_output.ime.is_none());
            assert!(editor.ctx.input(|input| input.events.is_empty()));
        }
        editor.frame(vec![Event::Text("900".into())]);
        assert_eq!(editor.value, 900);
        assert_eq!(editor.text(), "900");
    }

    #[test]
    fn paste_keeps_digits_and_drops_nonnumeric_characters() {
        let mut editor = Editor::new(7, 0..=i32::MAX as i64);
        editor.frame(vec![Event::Paste("ab9日0 0.f\n".into())]);
        assert_eq!(editor.value, 900);
        assert_eq!(editor.text(), "900");
        editor.frame(vec![Event::Paste("invalid".into())]);
        assert_eq!(editor.text(), "900");
    }

    #[test]
    fn negative_end_and_offset_remain_editable() {
        for (range, text, expected) in
            [(-1..=i32::MAX as i64, "-1", -1), (-500..=500, "-120", -120)]
        {
            let mut editor = Editor::new(0, range);
            editor.frame(vec![Event::Text(text.into())]);
            assert_eq!(editor.value, expected);
            assert_eq!(editor.text(), text);
        }
    }

    #[test]
    fn editing_keys_and_selection_still_work() {
        let mut editor = Editor::new(123, 0..=i32::MAX as i64);
        editor.frame(vec![
            key(Key::Home, egui::Modifiers::NONE),
            Event::Text("4".into()),
        ]);
        assert_eq!(editor.text(), "4123");
        editor.frame(vec![
            key(Key::End, egui::Modifiers::NONE),
            key(Key::Backspace, egui::Modifiers::NONE),
        ]);
        assert_eq!(editor.text(), "412");
        editor.frame(vec![
            key(Key::Home, egui::Modifiers::NONE),
            key(Key::Delete, egui::Modifiers::NONE),
        ]);
        assert_eq!(editor.text(), "12");
        editor.frame(vec![
            key(Key::A, egui::Modifiers::CTRL),
            Event::Paste("90x0".into()),
        ]);
        assert_eq!(editor.text(), "900");
        assert_eq!(editor.value, 900);
        editor.frame(vec![key(Key::ArrowUp, egui::Modifiers::NONE)]);
        assert_eq!(editor.value, 901);
        editor.frame(vec![key(Key::Enter, egui::Modifiers::NONE)]);
        assert!(!editor.ctx.memory(|memory| memory.has_focus(editor.id)));
    }

    #[test]
    fn ime_preedit_is_hidden_and_invalid_commit_is_ignored() {
        let mut editor = Editor::new(123, 0..=i32::MAX as i64);
        for event in [
            Event::Ime(ImeEvent::Enabled),
            Event::Ime(ImeEvent::Preedit("あ".into())),
            Event::Ime(ImeEvent::Commit("あ".into())),
            Event::Ime(ImeEvent::Disabled),
        ] {
            let output = editor.frame(vec![event]);
            assert_eq!(editor.value, 123);
            assert_eq!(editor.text(), "123");
            assert!(output.platform_output.ime.is_none());
        }
        editor.frame(vec![Event::Ime(ImeEvent::Commit("900".into()))]);
        assert_eq!(editor.text(), "900");
        assert_eq!(editor.value, 900);
    }

    #[test]
    fn normal_name_fields_keep_text_input_and_ime() {
        let ctx = egui::Context::default();
        let mut number = 0;
        let mut name = String::new();
        for events in [vec![], vec![Event::Text("名前 abc".into())]] {
            let output = ctx.run(
                egui::RawInput {
                    events,
                    focused: true,
                    ..Default::default()
                },
                |ctx| {
                    egui::CentralPanel::default().show(ctx, |ui| {
                        ui.add(Integer::new(&mut number));
                        ui.text_edit_singleline(&mut name).request_focus();
                    });
                },
            );
            if !name.is_empty() {
                assert!(output.platform_output.ime.is_some());
            }
        }
        assert_eq!(name, "名前 abc");
        assert_eq!(number, 0);
    }
}
