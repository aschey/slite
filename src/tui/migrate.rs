use std::marker::PhantomData;
use std::rc::Rc;

use ansi_to_tui::IntoText;
use chrono::Local;
use elm_ui::{Command, Message, Model, OptionalCommand};
use futures::StreamExt;
use ratatui::buffer::Buffer;
use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{
    Block, BorderType, Borders, Clear, Paragraph, StatefulWidget, Widget, Wrap,
};
use tokio_stream::wrappers::BroadcastStream;
use tracing::error;

use super::{
    BiPanel, BiPanelState, BroadcastWriter, Button, MigratorFactory, Scrollable, ScrollableState,
    panel,
};
use crate::Options;
use crate::error::{InitializationError, SqlFormatError};

pub enum MigrationMessage {
    ProcessCompleted,
    MigrationCompleted,
    Log(String),
}

#[derive(Default)]
pub struct MigrationView<'a> {
    _phantom: PhantomData<&'a ()>,
}

impl<'a> StatefulWidget for MigrationView<'a> {
    type State = MigrationState<'a>;

    fn render(
        self,
        area: ratatui::layout::Rect,
        buf: &mut ratatui::buffer::Buffer,
        state: &mut Self::State,
    ) {
        let chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Length(22), Constraint::Min(0)])
            .split(area);

        Paragraph::new(vec![
            Line::from(
                Button::new("   Dry Run         ")
                    .fg(Color::Blue)
                    .selected(state.selected == 0)
                    .enabled(state.controls_enabled)
                    .build(),
            ),
            Line::from(""),
            Line::from(
                Button::new("   Generate Script ")
                    .fg(Color::Blue)
                    .selected(state.selected == 1)
                    .enabled(state.controls_enabled)
                    .build(),
            ),
            Line::from(""),
            Line::from(
                Button::new("   Migrate         ")
                    .fg(Color::Green)
                    .selected(state.selected == 2)
                    .enabled(state.controls_enabled)
                    .build(),
            ),
            Line::from(""),
            Line::from(
                Button::new("   Clear Output     ")
                    .fg(Color::Magenta)
                    .selected(state.selected == 3)
                    .enabled(state.controls_enabled)
                    .build(),
            ),
        ])
        .alignment(Alignment::Center)
        .block(state.bipanel_state.left_block("Controls"))
        .render(chunks[0], buf);

        Scrollable::new(
            Paragraph::new(state.formatted_logs.clone())
                .block(state.bipanel_state.right_block(&state.log_title())),
        )
        .render(chunks[1], buf, &mut state.scroller);

        if state.show_popup {
            let text = Paragraph::new(vec![
                Line::from(vec![Span::from("Run database migration?")]),
                Line::from(""),
            ])
            .wrap(Wrap { trim: false });
            let buttons = Paragraph::new(Line::from(vec![
                Button::new("  Cancel ")
                    .fg(Color::Yellow)
                    .selected(state.popup_button_index == 0)
                    .build(),
                Span::from("  "),
                Button::new("  Migrate ")
                    .fg(Color::Green)
                    .selected(state.popup_button_index == 1)
                    .build(),
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

            Clear.render(area, buf);
            block.render(area, buf);
            text.render(popup_chunks[0], buf);
            buttons.render(popup_chunks[1], buf);
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

#[derive(Debug, Clone)]
pub struct MigrationState<'a> {
    selected: i32,
    num_buttons: i32,
    show_popup: bool,
    popup_button_index: i32,
    logs: String,
    log_start_time: Option<chrono::DateTime<Local>>,
    formatted_logs: Text<'static>,
    scroller: ScrollableState,
    bipanel_state: BiPanelState,
    controls_enabled: bool,
    migrator_factory: MigratorFactory,
    _phantom: PhantomData<&'a ()>,
}

impl<'a> MigrationState<'a> {
    pub fn new(migrator_factory: MigratorFactory) -> Self {
        Self {
            migrator_factory,
            selected: 0,
            scroller: ScrollableState::new(0),
            num_buttons: 4,
            show_popup: false,
            popup_button_index: 0,
            logs: "".to_owned(),
            bipanel_state: BiPanelState::default(),
            formatted_logs: Text::default(),
            log_start_time: None,
            controls_enabled: true,
            _phantom: Default::default(),
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

    #[cfg(feature = "crossterm-events")]
    pub fn handle_event(
        &mut self,
        event: &crossterm::event::Event,
    ) -> Result<Option<Box<dyn FnOnce() -> MigrationMessage + Send>>, InitializationError> {
        use crossterm::event::{Event, KeyCode, KeyEventKind};

        if let Event::Key(key) = event
            && key.kind == KeyEventKind::Press {
                match key.code {
                    KeyCode::Up => self.previous(),
                    KeyCode::Down => self.next(),
                    KeyCode::Left | KeyCode::Right | KeyCode::Tab if self.popup_active() => {
                        self.toggle_popup_confirm()
                    }
                    KeyCode::Tab => self.toggle_focus(),
                    KeyCode::Enter => return self.execute(),
                    _ => {}
                }
            }

        Ok(None)
    }

    pub fn execute(
        &mut self,
    ) -> Result<Option<Box<dyn FnOnce() -> MigrationMessage + Send>>, InitializationError> {
        if !self.controls_enabled {
            return Ok(None);
        }

        if self.show_popup {
            let popup_button_index = self.popup_button_index;
            self.popup_button_index = 0;
            self.show_popup = false;
            if popup_button_index == 1 {
                self.clear_logs();
                BroadcastWriter::enable();
                self.log_start_time = Some(chrono::Local::now());
                let migrator = self.migrator_factory.create_migrator(Options {
                    allow_deletions: true,
                    dry_run: false,
                })?;

                self.controls_enabled = false;
                return Ok(Some(Box::new(move || {
                    if let Err(e) = migrator.migrate() {
                        error!("{e}");
                    }
                    MigrationMessage::MigrationCompleted
                })));
            }
        } else {
            match self.selected {
                0 => {
                    self.clear_logs();
                    BroadcastWriter::enable();
                    self.log_start_time = Some(chrono::Local::now());
                    let migrator = self.migrator_factory.create_migrator(Options {
                        allow_deletions: true,
                        dry_run: true,
                    })?;

                    self.controls_enabled = false;
                    return Ok(Some(Box::new(move || {
                        if let Err(e) = migrator.migrate() {
                            error!("{e}");
                        }
                        MigrationMessage::ProcessCompleted
                    })));
                }
                1 => {
                    self.clear_logs();
                    self.log_start_time = Some(chrono::Local::now());

                    let migrator = self.migrator_factory.create_migrator(Options {
                        allow_deletions: true,
                        dry_run: true,
                    })?;

                    self.controls_enabled = false;
                    return Ok(Some(Box::new(move || {
                        let writer = BroadcastWriter::default();

                        if let Err(e) = migrator.migrate_with_callback(|statement| {
                            writer.force_send(format!("{statement}\n"));
                        }) {
                            error!("{e}");
                        };
                        MigrationMessage::ProcessCompleted
                    })));
                }
                2 => {
                    self.show_popup = true;
                }
                3 => {
                    self.clear_logs();
                }
                _ => {}
            }
        }

        Ok(None)
    }

    pub fn popup_active(&self) -> bool {
        self.show_popup
    }

    pub fn toggle_popup_confirm(&mut self) {
        self.popup_button_index = (self.popup_button_index + 1) % 2;
    }

    pub fn add_log(&mut self, log: &str) -> Result<(), SqlFormatError> {
        self.logs += log;
        self.formatted_logs = self
            .logs
            .into_text()
            .map_err(|e| SqlFormatError::AnsiConversionFailure(log.to_string(), e))?;
        self.scroller
            .set_content_height(self.formatted_logs.height() as u16);
        Ok(())
    }

    pub fn clear_logs(&mut self) {
        self.logs = "".to_owned();
        self.formatted_logs = Text::default();
        self.scroller.set_content_height(0);
        self.log_start_time = None;
    }

    pub fn migrator_factory(&mut self) -> &mut MigratorFactory {
        &mut self.migrator_factory
    }

    fn log_title(&self) -> String {
        match self.log_start_time {
            Some(start_time) => format!("Logs {}", start_time.format("%Y-%m-%d %H:%M:%S")),
            None => "Logs".to_owned(),
        }
    }
}

impl<'a> BiPanel for MigrationState<'a> {
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

impl<'a> Model for MigrationState<'a> {
    type Writer = (Rect, &'a mut Buffer);

    type Error = SqlFormatError;

    fn init(&mut self) -> Result<OptionalCommand, Self::Error> {
        Ok(Some(Command::new_async(
            |_, cancellation_token| async move {
                let log_stream = BroadcastStream::new(BroadcastWriter::default().receiver());
                Some(Message::Stream(Box::pin(
                    log_stream
                        .map(|log| Message::custom(MigrationMessage::Log(log.unwrap())))
                        .take_until(cancellation_token.cancelled_owned()),
                )))
            },
        )))
    }

    fn update(&mut self, msg: Rc<Message>) -> Result<OptionalCommand, Self::Error> {
        match msg.as_ref() {
            Message::TermEvent(msg) => {
                if let Some(func) = self.handle_event(msg).unwrap() {
                    return Ok(Some(Command::new_blocking(|_, _| {
                        let msg = func();
                        Some(Message::Custom(Box::new(msg)))
                    })));
                }
            }
            Message::Custom(msg) => {
                if let Some(msg) = msg.downcast_ref::<MigrationMessage>() {
                    match msg {
                        MigrationMessage::Log(log) => {
                            self.add_log(log)?;
                        }
                        MigrationMessage::ProcessCompleted
                        | MigrationMessage::MigrationCompleted => {
                            self.controls_enabled = true;
                            BroadcastWriter::disable();
                        }
                    }
                }
            }
            _ => {}
        }
        Ok(None)
    }

    fn view(&self, (rect, buf): &mut Self::Writer) -> Result<(), Self::Error> {
        MigrationView::default().render(*rect, buf, &mut self.clone());
        Ok(())
    }
}
