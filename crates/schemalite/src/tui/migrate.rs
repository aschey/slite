use ansi_to_tui::IntoText;
use tui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Span, Spans, Text},
    widgets::{Block, BorderType, Borders, Clear, Paragraph, StatefulWidget, Widget, Wrap},
};

use crate::{
    error::{MigrationError, SqlFormatError},
    Migrator, Options,
};

use super::{panel, BiPanel, BiPanelState, Scrollable, ScrollableState};

pub struct MigrationView {}

impl StatefulWidget for MigrationView {
    type State = MigrationState;

    fn render(
        self,
        area: tui::layout::Rect,
        buf: &mut tui::buffer::Buffer,
        state: &mut Self::State,
    ) {
        let chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Length(21), Constraint::Min(0)])
            .split(area);

        Widget::render(
            Paragraph::new(vec![
                Spans::from(Span::styled(
                    "     Dry Run     ",
                    Style::default()
                        .bg(Color::Black)
                        .fg(Color::Blue)
                        .add_modifier(if state.selected == 0 {
                            Modifier::BOLD | Modifier::SLOW_BLINK | Modifier::REVERSED
                        } else {
                            Modifier::empty()
                        }),
                )),
                Spans::from(""),
                Spans::from(Span::styled(
                    " Generate Script ",
                    Style::default()
                        .bg(Color::Black)
                        .fg(Color::Blue)
                        .add_modifier(if state.selected == 1 {
                            Modifier::BOLD | Modifier::SLOW_BLINK | Modifier::REVERSED
                        } else {
                            Modifier::empty()
                        }),
                )),
                Spans::from(""),
                Spans::from(Span::styled(
                    "     Migrate     ",
                    Style::default()
                        .bg(Color::Black)
                        .fg(Color::Yellow)
                        .add_modifier(if state.selected == 2 && !state.show_popup {
                            Modifier::BOLD | Modifier::SLOW_BLINK | Modifier::REVERSED
                        } else {
                            Modifier::empty()
                        }),
                )),
                Spans::from(""),
                Spans::from(Span::styled(
                    "  Clear Output   ",
                    Style::default()
                        .bg(Color::Black)
                        .fg(Color::Magenta)
                        .add_modifier(if state.selected == 3 {
                            Modifier::BOLD | Modifier::SLOW_BLINK | Modifier::REVERSED
                        } else {
                            Modifier::empty()
                        }),
                )),
            ])
            .alignment(Alignment::Center)
            .block(state.bipanel_state.left_block("Controls")),
            chunks[0],
            buf,
        );

        StatefulWidget::render(
            Scrollable::new(
                Paragraph::new(state.formatted_logs.clone())
                    .block(state.bipanel_state.right_block("Logs")),
            ),
            chunks[1],
            buf,
            &mut state.scroller,
        );

        if state.show_popup {
            let text = Paragraph::new(vec![
                Spans::from(vec![Span::from("Run database migration?")]),
                Spans::from(""),
            ])
            .wrap(Wrap { trim: false });
            let buttons = Paragraph::new(Spans::from(vec![
                Span::styled(
                    " Cancel ",
                    Style::default()
                        .bg(Color::Black)
                        .fg(Color::Blue)
                        .add_modifier(if state.popup_button_index == 0 {
                            Modifier::BOLD | Modifier::SLOW_BLINK | Modifier::REVERSED
                        } else {
                            Modifier::empty()
                        }),
                ),
                Span::from("  "),
                Span::styled(
                    " Migrate ",
                    Style::default()
                        .bg(Color::Black)
                        .fg(Color::Yellow)
                        .add_modifier(if state.popup_button_index == 1 {
                            Modifier::BOLD | Modifier::SLOW_BLINK | Modifier::REVERSED
                        } else {
                            Modifier::empty()
                        }),
                ),
                Span::from(" "),
            ]))
            .alignment(Alignment::Right);
            let block = Block::default()
                .title(Span::styled(
                    "Confirm Action",
                    Style::default().add_modifier(Modifier::BOLD),
                ))
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(Style::default().fg(Color::Cyan));

            let area = centered_rect(30, 50, area);
            let popup_chunks = Layout::default()
                .direction(Direction::Vertical)
                .margin(1)
                .constraints([Constraint::Min(0), Constraint::Length(1)])
                .split(area);

            Widget::render(Clear, area, buf);
            Widget::render(block, area, buf);
            Widget::render(text, popup_chunks[0], buf);
            Widget::render(buttons, popup_chunks[1], buf);
        }
    }
}

fn centered_rect(percent_x: u16, percent_y: u16, r: Rect) -> Rect {
    let popup_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints(
            [
                Constraint::Percentage((100 - percent_y) / 2),
                Constraint::Max(7),
                Constraint::Percentage((100 - percent_y) / 2),
            ]
            .as_ref(),
        )
        .split(r);

    Layout::default()
        .direction(Direction::Horizontal)
        .constraints(
            [
                Constraint::Percentage((100 - percent_x) / 2),
                Constraint::Max(30),
                Constraint::Percentage((100 - percent_x) / 2),
            ]
            .as_ref(),
        )
        .split(popup_layout[1])[1]
}

pub struct MigrationState {
    selected: i32,
    num_buttons: i32,
    show_popup: bool,
    popup_button_index: i32,
    logs: String,
    formatted_logs: Text<'static>,
    scroller: ScrollableState,
    bipanel_state: BiPanelState,
    make_migrator: Box<dyn Fn(Options) -> Migrator>,
}

impl MigrationState {
    pub fn new(make_migrator: impl Fn(Options) -> Migrator + 'static) -> Self {
        Self {
            make_migrator: Box::new(make_migrator),
            selected: 0,
            scroller: ScrollableState::new(0),
            num_buttons: 4,
            show_popup: false,
            popup_button_index: 0,
            logs: "".to_owned(),
            bipanel_state: BiPanelState::default(),
            formatted_logs: Text::default(),
        }
    }

    pub fn next(&mut self) {
        panel::next(self, &self.bipanel_state.clone());
    }

    pub fn previous(&mut self) {
        panel::previous(self, &self.bipanel_state.clone());
    }

    pub fn toggle_focus(&mut self) {
        self.bipanel_state.toggle_focus();
    }

    pub fn execute(&mut self) -> Result<(), MigrationError> {
        if self.show_popup {
            let popup_button_index = self.popup_button_index;
            self.popup_button_index = 0;
            self.show_popup = false;
            if popup_button_index == 1 {
                self.clear_logs();
                let migrator = (self.make_migrator)(Options {
                    allow_deletions: true,
                    dry_run: false,
                });
                migrator.migrate()?;
            }
        } else if self.selected == 2 {
            self.show_popup = true;
        }

        Ok(())
    }

    pub fn popup_active(&self) -> bool {
        self.show_popup
    }

    pub fn toggle_popup_confirm(&mut self) {
        self.popup_button_index = (self.popup_button_index + 1) % 2;
    }

    pub fn add_log(&mut self, log: String) -> Result<(), SqlFormatError> {
        self.logs += &log;
        self.formatted_logs = self
            .logs
            .into_text()
            .map_err(|e| SqlFormatError::TextFormattingFailure(log, e))?;
        self.scroller
            .set_content_height(self.formatted_logs.height() as u16);
        Ok(())
    }

    pub fn clear_logs(&mut self) {
        self.logs = "".to_owned();
        self.formatted_logs = Text::default();
        self.scroller.set_content_height(0);
    }
}

impl BiPanel for MigrationState {
    fn left_next(&mut self) {
        if !self.show_popup {
            self.selected = (self.selected + 1).rem_euclid(self.num_buttons);
        }
    }

    fn right_next(&mut self) {
        self.scroller.scroll_down();
    }

    fn left_previous(&mut self) {
        if !self.show_popup {
            self.selected = (self.selected - 1).rem_euclid(self.num_buttons);
        }
    }

    fn right_previous(&mut self) {
        self.scroller.scroll_up();
    }
}
