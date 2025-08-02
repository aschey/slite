use std::rc::Rc;

use ansi_to_tui::IntoText;
use elm_ui::{Message, Model, OptionalCommand};
use ratatui::buffer::Buffer;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::Color;
use ratatui::text::Text;
use ratatui::widgets::{Paragraph, StatefulWidget, Wrap};
use tui_syntax_highlight::Highlighter;

use super::{
    BiPanel, BiPanelState, Objects, ObjectsState, Scrollable, ScrollableState, StyledObject,
    StyledObjects, panel,
};
use crate::error::SqlFormatError;
use crate::{Metadata, MigrationMetadata, SYNTAXES, THEMES, diff_metadata};

#[derive(Debug, Clone)]
pub struct SqlView<'a> {
    title: &'a str,
}

impl<'a> SqlView<'a> {
    pub fn new(title: &'a str) -> Self {
        Self { title }
    }
}

impl<'a> StatefulWidget for SqlView<'a> {
    type State = SqlState<'a>;

    fn render(
        self,
        area: ratatui::layout::Rect,
        buf: &mut ratatui::buffer::Buffer,
        state: &mut Self::State,
    ) {
        let chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Length(state.state.view_width() as u16),
                Constraint::Min(0),
            ])
            .split(area);

        Objects::new(state.bipanel_state.left_block(self.title)).render(
            chunks[0],
            buf,
            &mut state.state,
        );

        Scrollable::new(
            Paragraph::new(
                state
                    .sql
                    .get(state.state.selected_index())
                    .cloned()
                    .unwrap_or_default(),
            )
            .wrap(Wrap { trim: false })
            .block(state.bipanel_state.right_block("SQL")),
        )
        .render(chunks[1], buf, &mut state.scroller);
    }
}

#[derive(Debug, Clone)]
pub struct SqlState<'a> {
    sql: Vec<Text<'a>>,
    title: &'a str,
    state: ObjectsState,
    scroller: ScrollableState,
    bipanel_state: BiPanelState,
}

impl<'a> SqlState<'a> {
    pub fn diff(title: &'a str, schemas: MigrationMetadata) -> Result<Self, SqlFormatError> {
        let diffs = diff_metadata(schemas);

        let objects = diffs.iter().map(|(object_type, objects)| {
            (
                object_type.to_owned(),
                objects
                    .iter()
                    .map(|(name, diff)| {
                        if diff.diff_text.is_empty() {
                            StyledObject {
                                object: name.to_owned(),
                                foreground: Color::Reset,
                            }
                        } else if diff.original_text.is_empty() {
                            StyledObject {
                                object: name.to_owned(),
                                foreground: Color::Red,
                            }
                        } else if diff.new_text.is_empty() {
                            StyledObject {
                                object: name.to_owned(),
                                foreground: Color::Green,
                            }
                        } else {
                            StyledObject {
                                object: name.to_owned(),
                                foreground: Color::Yellow,
                            }
                        }
                    })
                    .collect::<Vec<_>>(),
            )
        });

        let styled = StyledObjects::from_iter(objects);

        let list_items: Result<Vec<_>, _> = diffs
            .iter()
            .flat_map(|(_, objects)| {
                objects.values().map(|diff| {
                    let text = if diff.diff_text.is_empty() {
                        diff.original_text.to_owned()
                    } else {
                        diff.diff_text.to_owned()
                    };
                    text.into_text()
                        .map_err(|e| SqlFormatError::AnsiConversionFailure(text, e))
                })
            })
            .collect();

        let state = ObjectsState::new(styled);

        Ok(Self::new(title, list_items?, state))
    }

    pub fn schema(title: &'a str, schema: Metadata) -> Result<Self, SqlFormatError> {
        let objects = schema.iter().map(|(object_type, objects)| {
            (
                object_type.to_owned(),
                objects
                    .keys()
                    .map(|name| StyledObject {
                        object: name.to_owned(),
                        foreground: Color::Reset,
                    })
                    .collect::<Vec<_>>(),
            )
        });
        let styled = StyledObjects::from_iter(objects);
        let state = ObjectsState::new(styled);
        let theme = THEMES
            .themes
            .get("ansi")
            .expect("Failed to load ansi theme");
        let sql_syntax = SYNTAXES
            .find_syntax_by_name("SQL")
            .expect("Failed to load SQL syntax")
            .to_owned();

        let highlighter = Highlighter::new(theme.clone()).line_numbers(false);

        let list_items: Result<Vec<_>, _> = schema
            .iter()
            .flat_map(|(_, objects)| {
                objects.values().map(|text| {
                    Ok(highlighter
                        .highlight_lines(text.clone(), &sql_syntax, &SYNTAXES)
                        .map_err(|e| SqlFormatError::TextFormattingFailure(text.to_owned(), e))?
                        .into_text())
                })
            })
            .collect();

        Ok(Self::new(title, list_items?, state))
    }

    fn new(title: &'a str, sql: Vec<Text<'static>>, state: ObjectsState) -> Self {
        let height = sql.first().map(|s| s.height()).unwrap_or(0) as u16;
        let scroller = ScrollableState::new(height);
        Self {
            sql,
            title,
            state,
            scroller,
            bipanel_state: BiPanelState::default(),
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

    pub fn selected_item(&self) -> Option<String> {
        self.state.selected_item()
    }

    pub fn select(&mut self, item: &str) {
        self.state.select(item);
    }

    pub fn refresh_schema(&mut self, metadata: Metadata) -> Result<(), SqlFormatError> {
        let selected = self.selected_item();
        let mut new_state = SqlState::schema(self.title, metadata)?;
        if let Some(selected) = selected {
            new_state.select(&selected);
        }
        std::mem::swap(self, &mut new_state);
        Ok(())
    }

    pub fn refresh_diff(&mut self, metadata: MigrationMetadata) -> Result<(), SqlFormatError> {
        let selected = self.selected_item();
        let mut new_state = SqlState::diff(self.title, metadata)?;
        if let Some(selected) = selected {
            new_state.select(&selected);
        }
        std::mem::swap(self, &mut new_state);
        Ok(())
    }

    #[cfg(feature = "crossterm-events")]
    pub fn handle_event(&mut self, event: &crossterm::event::Event) {
        use crossterm::event::{Event, KeyCode, KeyEventKind};

        if let Event::Key(key) = event {
            if key.kind == KeyEventKind::Press {
                match key.code {
                    KeyCode::Up => self.previous(),
                    KeyCode::Down => self.next(),
                    KeyCode::Tab => self.toggle_focus(),
                    _ => {}
                }
            }
        }
    }
}

impl<'a> BiPanel for SqlState<'a> {
    fn left_next(&mut self) {
        if self.sql.is_empty() {
            return;
        }

        self.state.next();
        self.scroller
            .set_content_height(self.sql.get(self.state.selected_index()).unwrap().height() as u16);
        self.scroller.scroll_to_top();
    }

    fn right_next(&mut self) {
        self.scroller.scroll_down();
    }

    fn left_previous(&mut self) {
        if self.sql.is_empty() {
            return;
        }

        self.state.previous();
        self.scroller
            .set_content_height(self.sql.get(self.state.selected_index()).unwrap().height() as u16);
        self.scroller.scroll_to_top();
    }

    fn right_previous(&mut self) {
        self.scroller.scroll_up();
    }
}

impl<'a> Model for SqlState<'a> {
    type Writer = (Rect, &'a mut Buffer);
    type Error = std::io::Error;

    fn init(&mut self) -> Result<OptionalCommand, Self::Error> {
        Ok(None)
    }
    fn update(&mut self, msg: Rc<Message>) -> Result<OptionalCommand, Self::Error> {
        if let Message::TermEvent(msg) = msg.as_ref() {
            self.handle_event(msg);
        }
        Ok(None)
    }

    fn view(&self, (rect, buf): &mut Self::Writer) -> Result<(), Self::Error> {
        SqlView::new(self.title).render(*rect, buf, &mut self.clone());
        Ok(())
    }
}
