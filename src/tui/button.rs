use ratatui::style::{Color, Modifier, Style};
use ratatui::text::Span;

pub struct Button<'a> {
    enabled: bool,
    selected: bool,
    fg: Color,
    text: &'a str,
}

impl<'a> Button<'a> {
    pub fn new(text: &'a str) -> Self {
        Self {
            text,
            enabled: true,
            selected: false,
            fg: Color::Reset,
        }
    }

    pub fn enabled(self, enabled: bool) -> Self {
        Self { enabled, ..self }
    }

    pub fn selected(self, selected: bool) -> Self {
        Self { selected, ..self }
    }

    pub fn fg(self, fg: Color) -> Self {
        Self { fg, ..self }
    }

    pub fn build(self) -> Span<'a> {
        if self.enabled {
            Span::styled(
                self.text,
                Style::default()
                    .bg(Color::Black)
                    .fg(self.fg)
                    .add_modifier(if self.selected {
                        Modifier::BOLD | Modifier::SLOW_BLINK | Modifier::REVERSED
                    } else {
                        Modifier::empty()
                    }),
            )
        } else {
            Span::styled(self.text, Style::default().fg(Color::Gray).bg(Color::Black))
        }
    }
}
