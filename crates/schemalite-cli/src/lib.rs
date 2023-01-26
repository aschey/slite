use color_eyre::Report;
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use schemalite::tui::{MigrationState, MigrationView, SqlState, SqlView};
use schemalite::MigrationMetadata;
use std::io::{self, Stdout};
use tui::{
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    text::{Span, Spans},
    widgets::{Block, BorderType, Borders, Tabs},
    Frame, Terminal,
};

struct App<'a> {
    pub titles: Vec<&'a str>,
    pub index: i32,
    source_schema: SqlState,
    target_schema: SqlState,
    diff_schema: SqlState,
    migration: MigrationState,
}

impl<'a> App<'a> {
    fn new(schema: MigrationMetadata) -> Result<App<'a>, Report> {
        Ok(App {
            titles: vec!["Source", "Target", "Diff", "Migrate"],
            index: 0,
            source_schema: SqlState::schema(schema.source.clone())?,
            target_schema: SqlState::schema(schema.target.clone())?,
            diff_schema: SqlState::diff(schema)?,
            migration: MigrationState::default(),
        })
    }

    pub fn next_tab(&mut self) {
        self.index = (self.index + 1).rem_euclid(self.titles.len() as i32);
    }

    pub fn previous_tab(&mut self) {
        self.index = (self.index - 1).rem_euclid(self.titles.len() as i32);
    }
}

pub fn run_tui(schema: MigrationMetadata) -> Result<(), Report> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let app = App::new(schema)?;
    let res = run_app(&mut terminal, app);

    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;

    if let Err(err) = res {
        println!("{err:?}")
    }

    Ok(())
}

fn run_app(terminal: &mut Terminal<CrosstermBackend<Stdout>>, mut app: App) -> io::Result<()> {
    loop {
        terminal.draw(|f| ui(f, &mut app))?;

        if let Event::Key(key) = event::read()? {
            match (key.code, app.index) {
                (KeyCode::Char('q'), _) => return Ok(()),
                (KeyCode::Left | KeyCode::Right | KeyCode::Tab, 3)
                    if app.migration.popup_active() =>
                {
                    app.migration.toggle_popup_confirm()
                }
                (KeyCode::Right, _) => app.next_tab(),
                (KeyCode::Left, _) => app.previous_tab(),
                (KeyCode::Down, 0) => app.source_schema.next(),
                (KeyCode::Down, 1) => app.target_schema.next(),
                (KeyCode::Down, 2) => app.diff_schema.next(),
                (KeyCode::Down, 3) => app.migration.next(),
                (KeyCode::Up, 0) => app.source_schema.previous(),
                (KeyCode::Up, 1) => app.target_schema.previous(),
                (KeyCode::Up, 2) => app.diff_schema.previous(),
                (KeyCode::Up, 3) => app.migration.previous(),
                (KeyCode::Tab, 0) => app.source_schema.toggle_focus(),
                (KeyCode::Tab, 1) => app.target_schema.toggle_focus(),
                (KeyCode::Tab, 2) => app.diff_schema.toggle_focus(),
                (KeyCode::Enter, 3) => app.migration.execute(),
                _ => {}
            }
        }
    }
}

fn ui(f: &mut Frame<CrosstermBackend<Stdout>>, app: &mut App) {
    let size = f.size();
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .margin(1)
        .constraints([Constraint::Length(3), Constraint::Min(0)].as_ref())
        .split(size);

    let block = Block::default().style(Style::default());
    f.render_widget(block, size);
    let titles = app
        .titles
        .iter()
        .map(|t| Spans::from(vec![Span::styled(*t, Style::default().fg(Color::Green))]))
        .collect();
    let tabs = Tabs::new(titles)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded),
        )
        .select(app.index as usize)
        .style(Style::default())
        .highlight_style(
            Style::default()
                .add_modifier(Modifier::BOLD)
                .bg(Color::Black),
        );
    f.render_widget(tabs, chunks[0]);
    match app.index {
        0 => {
            f.render_stateful_widget(SqlView::default(), chunks[1], &mut app.source_schema);
        }
        1 => {
            f.render_stateful_widget(SqlView::default(), chunks[1], &mut app.target_schema);
        }
        2 => {
            f.render_stateful_widget(SqlView::default(), chunks[1], &mut app.diff_schema);
        }
        3 => f.render_stateful_widget(MigrationView {}, chunks[1], &mut app.migration),
        _ => {}
    };
}
