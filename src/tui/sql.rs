use std::{marker::PhantomData, sync::Arc};

use super::{
    panel, BiPanel, BiPanelState, Objects, ObjectsState, Scrollable, ScrollableState, StyledObject,
    StyledObjects,
};
use crate::{diff_metadata, error::SqlFormatError, Metadata, MigrationMetadata, SqlPrinter};
use ansi_to_tui::IntoText;
use tui::{
    buffer::Buffer,
    layout::{Constraint, Direction, Layout, Rect},
    style::Color,
    text::Text,
    widgets::{Paragraph, StatefulWidget, Wrap},
};
use tui_elm::{Message, Model, OptionalCommand};

#[derive(Debug, Clone, Default)]
pub struct SqlView<'a> {
    _phantom: PhantomData<&'a ()>,
}

impl<'a> StatefulWidget for SqlView<'a> {
    type State = SqlState<'a>;

    fn render(
        self,
        area: tui::layout::Rect,
        buf: &mut tui::buffer::Buffer,
        state: &mut Self::State,
    ) {
        let chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Length(state.state.view_width() as u16),
                Constraint::Min(0),
            ])
            .split(area);

        StatefulWidget::render(
            Objects::new(state.bipanel_state.left_block("Objects")),
            chunks[0],
            buf,
            &mut state.state,
        );

        if !state.sql.is_empty() {
            StatefulWidget::render(
                Scrollable::new(
                    Paragraph::new(
                        state
                            .sql
                            .get(state.state.selected_index())
                            .expect("Selected index out of bounds")
                            .clone(),
                    )
                    .wrap(Wrap { trim: false })
                    .block(state.bipanel_state.right_block("SQL")),
                ),
                chunks[1],
                buf,
                &mut state.scroller,
            );
        }
    }
}

#[derive(Debug, Clone)]
pub struct SqlState<'a> {
    sql: Vec<Text<'a>>,
    state: ObjectsState,
    scroller: ScrollableState,
    bipanel_state: BiPanelState,
}

impl<'a> SqlState<'a> {
    pub fn diff(schemas: MigrationMetadata) -> Result<Self, SqlFormatError> {
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
                objects.iter().map(|(_, diff)| {
                    let text = if diff.diff_text.is_empty() {
                        diff.original_text.to_owned()
                    } else {
                        diff.diff_text.to_owned()
                    };
                    text.into_text()
                        .map_err(|e| SqlFormatError::TextFormattingFailure(text, e))
                })
            })
            .collect();

        let state = ObjectsState::new(styled);

        Ok(Self::new(list_items?, state))
    }

    pub fn schema(schema: Metadata) -> Result<Self, SqlFormatError> {
        let objects = schema.iter().map(|(object_type, objects)| {
            (
                object_type.to_owned(),
                objects
                    .iter()
                    .map(|(name, _)| StyledObject {
                        object: name.to_owned(),
                        foreground: Color::Reset,
                    })
                    .collect::<Vec<_>>(),
            )
        });
        let styled = StyledObjects::from_iter(objects);
        let state = ObjectsState::new(styled);

        let list_items: Result<Vec<_>, _> = schema
            .iter()
            .flat_map(|(_, objects)| {
                let mut printer = SqlPrinter::default();
                objects.values().map(move |text| {
                    printer
                        .print(text)
                        .into_text()
                        .map_err(|e| SqlFormatError::TextFormattingFailure(text.to_owned(), e))
                })
            })
            .collect();

        Ok(Self::new(list_items?, state))
    }

    fn new(sql: Vec<Text<'static>>, state: ObjectsState) -> Self {
        let height = sql.get(0).map(|s| s.height()).unwrap_or(0) as u16;
        let scroller = ScrollableState::new(height);
        Self {
            sql,
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

    #[cfg(feature = "crossterm-events")]
    pub fn handle_event(&mut self, event: &crossterm::event::Event) {
        use crossterm::event::{Event, KeyCode, KeyEventKind};

        if let Event::Key(key) = event {
            if key.kind == KeyEventKind::Press {
                match key.code {
                    KeyCode::Up => self.previous(),
                    KeyCode::Down => self.next(),
                    KeyCode::Left | KeyCode::Right => self.toggle_focus(),
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
    fn update(&mut self, msg: Arc<Message>) -> Result<OptionalCommand, Self::Error> {
        match msg.as_ref() {
            tui_elm::Message::TermEvent(msg) => {
                self.handle_event(msg);
            }
            tui_elm::Message::Custom(msg) => {}
            _ => {}
        }
        Ok(None)
    }

    fn view(&self, (rect, buf): &mut Self::Writer) -> Result<(), Self::Error> {
        StatefulWidget::render(SqlView::default(), *rect, buf, &mut self.clone());
        Ok(())
    }
}
