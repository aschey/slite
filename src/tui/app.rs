use std::io::Stdout;
use std::marker::PhantomData;
use std::path::PathBuf;
use std::rc::Rc;

use elm_ui::{Command, Message, Model, OptionalCommand};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;
use ratatui::layout::{Constraint, Direction, Layout};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, StatefulWidget, Tabs, Widget};

use super::{MigrationMessage, MigrationState, MigratorFactory, SqlState};
use crate::Config;
use crate::error::{InitializationError, RefreshError, SqlFormatError};

#[derive(PartialEq, Eq)]
pub enum ControlFlow {
    Quit,
    Continue,
}

#[derive(Clone, Debug)]
pub enum AppMessage {
    FileChanged,
    ConfigChanged(Config),
}

#[derive(Default, Debug)]
pub struct App<'a> {
    phantom: PhantomData<&'a ()>,
}

impl<'a> StatefulWidget for App<'a> {
    type State = AppState<'a>;

    fn render(
        self,
        area: ratatui::layout::Rect,
        buf: &mut ratatui::buffer::Buffer,
        state: &mut Self::State,
    ) {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .margin(1)
            .constraints([Constraint::Length(2), Constraint::Min(0)].as_ref())
            .split(area);

        let block = Block::default().style(Style::default());
        block.render(area, buf);

        let titles: Vec<_> = state
            .titles
            .iter()
            .enumerate()
            .map(|(i, t)| {
                if i as i32 == state.index {
                    Line::from(vec![
                        Span::styled(t.icon, Style::default().fg(Color::Cyan)),
                        Span::styled(
                            t.text,
                            Style::default()
                                .fg(Color::White)
                                .add_modifier(Modifier::BOLD),
                        ),
                    ])
                } else {
                    Line::from(vec![Span::styled(
                        format!("{}{}", t.icon, t.text),
                        Style::default().fg(Color::Black),
                    )])
                }
            })
            .collect();
        let tabs = Tabs::new(titles)
            .block(
                Block::default()
                    .borders(Borders::BOTTOM)
                    .border_style(Style::default().fg(Color::Black))
                    .border_type(BorderType::Rounded),
            )
            .select(state.index as usize)
            .style(Style::default())
            .highlight_style(Style::default())
            .divider(Span::styled("|", Style::default().fg(Color::Gray)));
        tabs.render(chunks[0], buf);

        match state.index {
            0 => state.source_schema.view(&mut (chunks[1], buf)).unwrap(),
            1 => state.target_schema.view(&mut (chunks[1], buf)).unwrap(),
            2 => state.diff_schema.view(&mut (chunks[1], buf)).unwrap(),
            3 => state.migration.view(&mut (chunks[1], buf)).unwrap(),
            _ => {}
        }
    }
}

#[derive(Debug, Clone)]
pub struct Title<'a> {
    icon: &'a str,
    text: &'a str,
}

#[derive(Debug, Clone)]
pub struct AppState<'a> {
    pub titles: Vec<Title<'a>>,
    pub index: i32,
    source_schema: SqlState<'a>,
    target_schema: SqlState<'a>,
    diff_schema: SqlState<'a>,
    migration: MigrationState<'a>,
}

impl<'a> AppState<'a> {
    pub fn new(migrator_factory: MigratorFactory) -> Result<AppState<'a>, SqlFormatError> {
        let schema = migrator_factory.metadata();
        Ok(AppState {
            titles: vec![
                Title {
                    icon: " ",
                    text: "Source",
                },
                Title {
                    icon: " ",
                    text: "Target",
                },
                Title {
                    icon: " ",
                    text: "Diff",
                },
                Title {
                    icon: " ",
                    text: "Migrate",
                },
            ],
            index: 0,
            source_schema: SqlState::schema("Source", schema.source.clone())?,
            target_schema: SqlState::schema("Target", schema.target.clone())?,
            diff_schema: SqlState::diff("Diff", schema.clone())?,
            migration: MigrationState::new(migrator_factory),
        })
    }

    pub fn update_config(&mut self, config: Config) -> Result<(), RefreshError> {
        self.migration.migrator_factory().set_config(config);
        self.refresh()
    }

    pub fn set_schema_dir(&mut self, dir: PathBuf) -> Result<(), RefreshError> {
        self.migration.migrator_factory().set_schema_dir(dir);
        self.refresh()
    }

    pub fn set_target_path(&mut self, path: PathBuf) -> Result<(), RefreshError> {
        self.migration.migrator_factory().set_target_path(path);
        self.refresh()
    }

    pub fn refresh(&mut self) -> Result<(), RefreshError> {
        let migrator_factory = self.migration.migrator_factory();
        migrator_factory
            .update_schemas()
            .map_err(RefreshError::InitializationFailure)?;
        let schema = migrator_factory.metadata();

        self.source_schema
            .refresh_schema(schema.source.clone())
            .map_err(RefreshError::SqlFormatFailure)?;

        self.target_schema
            .refresh_schema(schema.target.clone())
            .map_err(RefreshError::SqlFormatFailure)?;

        self.diff_schema
            .refresh_diff(schema.clone())
            .map_err(RefreshError::SqlFormatFailure)?;

        Ok(())
    }

    pub fn next_tab(&mut self) {
        self.index = (self.index + 1).rem_euclid(self.titles.len() as i32);
    }

    pub fn previous_tab(&mut self) {
        self.index = (self.index - 1).rem_euclid(self.titles.len() as i32);
    }

    #[cfg(feature = "crossterm-events")]
    pub fn handle_event(
        &mut self,
        event: &crossterm::event::Event,
    ) -> Result<ControlFlow, InitializationError> {
        use crossterm::event::{Event, KeyCode, KeyEventKind};

        if let Event::Key(key) = event {
            if key.kind == KeyEventKind::Press {
                match (key.code, self.index) {
                    (KeyCode::Char('q'), _) => return Ok(ControlFlow::Quit),
                    (KeyCode::Right, _) if !(self.index == 3 && self.migration.popup_active()) => {
                        self.next_tab()
                    }
                    (KeyCode::Left, _) if !(self.index == 3 && self.migration.popup_active()) => {
                        self.previous_tab()
                    }
                    _ => {}
                }
            }
        }

        Ok(ControlFlow::Continue)
    }
}

impl<'a> Model for AppState<'a> {
    type Writer = Terminal<CrosstermBackend<Stdout>>;

    type Error = RefreshError;

    fn init(&mut self) -> Result<OptionalCommand, Self::Error> {
        Ok(self.migration.init().unwrap())
    }

    fn update(&mut self, msg: Rc<elm_ui::Message>) -> Result<OptionalCommand, Self::Error> {
        let mut cmds = vec![];

        match self.index {
            0 => {
                if let Some(cmd) = self.source_schema.update(msg.clone()).unwrap() {
                    cmds.push(cmd);
                }
            }
            1 => {
                if let Some(cmd) = self.target_schema.update(msg.clone()).unwrap() {
                    cmds.push(cmd);
                }
            }
            2 => {
                if let Some(cmd) = self.diff_schema.update(msg.clone()).unwrap() {
                    cmds.push(cmd);
                }
            }
            3 => {
                if let Some(cmd) = self.migration.update(msg.clone()).unwrap() {
                    cmds.push(cmd);
                }
            }
            _ => {}
        }

        match msg.as_ref() {
            Message::TermEvent(e) => {
                let control_flow = self
                    .handle_event(e)
                    .map_err(RefreshError::InitializationFailure)?;
                if control_flow == ControlFlow::Quit {
                    return Ok(Some(Command::quit()));
                }
            }
            Message::Custom(msg) => {
                if let Some(msg) = msg.downcast_ref::<AppMessage>() {
                    match msg {
                        AppMessage::FileChanged => {
                            self.refresh()?;
                        }
                        AppMessage::ConfigChanged(config) => {
                            self.update_config(config.clone())?;
                        }
                    }
                }
                if let Some(MigrationMessage::MigrationCompleted) =
                    msg.downcast_ref::<MigrationMessage>()
                {
                    self.refresh()?;
                }
            }
            _ => {}
        };
        Ok(Some(Command::simple(Message::Batch(cmds))))
    }

    fn view(&self, writer: &mut Self::Writer) -> Result<(), Self::Error> {
        writer
            .draw(|f| f.render_stateful_widget(App::default(), f.area(), &mut self.clone()))
            .map_err(RefreshError::IoFailure)?;
        Ok(())
    }
}
