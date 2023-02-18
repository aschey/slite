use std::{
    io::Stdout,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
};

use ansi_to_tui::IntoText;
use chrono::Local;
use tokio::{sync::mpsc, task};
use tracing::error;
use tui::{
    backend::CrosstermBackend,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Span, Spans, Text},
    widgets::{Block, BorderType, Borders, Clear, Paragraph, StatefulWidget, Widget, Wrap},
    Terminal,
};
use tui_elm::Model;

use crate::{
    error::{InitializationError, SqlFormatError},
    Options,
};

use super::{
    panel, AppMessage, BiPanel, BiPanelState, BroadcastWriter, Button, MigratorFactory, Scrollable,
    ScrollableState,
};

pub enum MigrationMessage {
    ProcessCompleted,
    Log(String),
}

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
        // let controls_enabled = state.controls_enabled.load(Ordering::SeqCst);
        Widget::render(
            Paragraph::new(vec![
                Button::new("     Dry Run     ")
                    .fg(Color::Blue)
                    .selected(state.selected == 0)
                    .enabled(state.controls_enabled)
                    .build(),
                Spans::from(""),
                Button::new(" Generate Script ")
                    .fg(Color::Blue)
                    .selected(state.selected == 1)
                    .enabled(state.controls_enabled)
                    .build(),
                Spans::from(""),
                Button::new("     Migrate     ")
                    .fg(Color::Yellow)
                    .selected(state.selected == 2)
                    .enabled(state.controls_enabled)
                    .build(),
                Spans::from(""),
                Button::new("  Clear Output   ")
                    .fg(Color::Magenta)
                    .selected(state.selected == 3)
                    .enabled(state.controls_enabled)
                    .build(),
            ])
            .alignment(Alignment::Center)
            .block(state.bipanel_state.left_block("Controls")),
            chunks[0],
            buf,
        );

        StatefulWidget::render(
            Scrollable::new(
                Paragraph::new(state.formatted_logs.clone())
                    .block(state.bipanel_state.right_block(&state.log_title())),
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

#[derive(Debug, Clone)]
pub struct MigrationState {
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
}

impl MigrationState {
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
    ) -> Result<Option<Box<dyn FnOnce(mpsc::Sender<tui_elm::Command>) + Send>>, InitializationError>
    {
        use crossterm::event::{Event, KeyCode, KeyEventKind};

        if let Event::Key(key) = event {
            if key.kind == KeyEventKind::Press {
                match key.code {
                    KeyCode::Up => self.previous(),

                    KeyCode::Left | KeyCode::Right if self.popup_active() => {
                        self.toggle_popup_confirm()
                    }
                    KeyCode::Left | KeyCode::Right => self.toggle_focus(),
                    KeyCode::Enter => return self.execute(),
                    _ => {}
                }
            }
        }

        Ok(None)
    }

    pub fn execute(
        &mut self,
    ) -> Result<Option<Box<dyn FnOnce(mpsc::Sender<tui_elm::Command>) + Send>>, InitializationError>
    {
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
                // let migration_script_tx = self.message_tx.clone();

                self.controls_enabled = false;
                return Ok(Some(Box::new(move |_| {
                    if let Err(e) = migrator.migrate() {
                        error!("{e}");
                    }
                    //controls_enabled.store(true, Ordering::SeqCst);
                    // if let Err(e) =
                    //     migration_script_tx.blocking_send(AppMessage::MigrationCompleted)
                    // {
                    //     error!("{e}");
                    // }
                    BroadcastWriter::disable();
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
                    // let migration_script_tx = self.message_tx.clone();
                    // let controls_enabled = self.controls_enabled.clone();
                    // controls_enabled.store(false, Ordering::SeqCst);
                    self.controls_enabled = false;
                    return Ok(Some(Box::new(move |_| {
                        if let Err(e) = migrator.migrate() {
                            error!("{e}");
                        }
                        //controls_enabled.store(true, Ordering::SeqCst);
                        // if let Err(e) =
                        //     migration_script_tx.blocking_send(AppMessage::ProcessCompleted)
                        // {
                        //     error!("{e}");
                        // }
                        BroadcastWriter::disable();
                    })));
                }
                1 => {
                    self.clear_logs();
                    self.log_start_time = Some(chrono::Local::now());

                    let migrator = self.migrator_factory.create_migrator(Options {
                        allow_deletions: true,
                        dry_run: true,
                    })?;
                    //let migration_script_tx = self.message_tx.clone();
                    // let controls_enabled = self.controls_enabled.clone();
                    // controls_enabled.store(false, Ordering::SeqCst);
                    self.controls_enabled = false;
                    return Ok(Some(Box::new(move |tx| {
                        if let Err(e) = migrator.migrate_with_callback(|statement| {
                            if let Err(e) = tx.blocking_send(tui_elm::Command::simple(
                                tui_elm::Message::Custom(Box::new(MigrationMessage::Log(
                                    statement,
                                ))),
                            )) {
                                error!("{e}");
                            }
                        }) {
                            error!("{e}");
                        };

                        //  controls_enabled.store(true, Ordering::SeqCst);
                        // if let Err(e) =
                        //     migration_script_tx.blocking_send(AppMessage::ProcessCompleted)
                        // {
                        //     error!("{e}");
                        // }
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

impl Model for MigrationState {
    type Writer = Terminal<CrosstermBackend<Stdout>>;

    type Error = SqlFormatError;

    fn init(&self) -> Result<tui_elm::OptionalCommand, Self::Error> {
        Ok(None)
    }

    fn update(
        &mut self,
        msg: Arc<tui_elm::Message>,
    ) -> Result<tui_elm::OptionalCommand, Self::Error> {
        match msg.as_ref() {
            tui_elm::Message::TermEvent(msg) => {
                if let Some(func) = self.handle_event(msg).unwrap() {
                    return Ok(Some(tui_elm::Command::new_blocking(|tx| {
                        func(tx);
                        Some(tui_elm::Message::Custom(Box::new(
                            MigrationMessage::ProcessCompleted,
                        )))
                    })));
                }
            }
            tui_elm::Message::Custom(msg) => {
                if let Some(msg) = msg.downcast_ref::<MigrationMessage>() {
                    match msg {
                        MigrationMessage::Log(log) => {
                            self.add_log(format!("{log}\n"))?;
                        }
                        MigrationMessage::ProcessCompleted => {
                            self.controls_enabled = true;
                        }
                    }
                }
            }
            _ => {}
        }
        Ok(None)
    }

    fn view(&self, writer: &mut Self::Writer) -> Result<(), Self::Error> {
        todo!()
    }
}
