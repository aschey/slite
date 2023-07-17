use tui::widgets::{Paragraph, StatefulWidget, Widget};

pub struct Scrollable<'a> {
    paragraph: Paragraph<'a>,
}

impl<'a> Scrollable<'a> {
    pub fn new(paragraph: Paragraph<'a>) -> Self {
        Self { paragraph }
    }
}

impl<'a> StatefulWidget for Scrollable<'a> {
    type State = ScrollableState;

    fn render(
        self,
        area: tui::layout::Rect,
        buf: &mut tui::buffer::Buffer,
        state: &mut Self::State,
    ) {
        let area_height = area.height - 2;
        if state.content_height < area_height {
            state.scroll_position = 0;
        }

        if state.content_height >= area_height
            && state.scroll_position + area_height >= state.content_height
        {
            state.scroll_position = state.content_height - area_height;
        }

        self.paragraph
            .scroll((state.scroll_position, 0))
            .render(area, buf);
    }
}

#[derive(Debug, Clone)]
pub struct ScrollableState {
    scroll_position: u16,
    content_height: u16,
}

impl ScrollableState {
    pub fn new(content_height: u16) -> Self {
        Self {
            scroll_position: 0,
            content_height,
        }
    }

    pub fn scroll_down(&mut self) {
        self.scroll_position += 1;
    }

    pub fn scroll_up(&mut self) {
        if self.scroll_position > 0 {
            self.scroll_position -= 1;
        }
    }

    pub fn scroll_to_top(&mut self) {
        self.scroll_position = 0;
    }

    pub fn set_content_height(&mut self, content_height: u16) {
        self.content_height = content_height;
    }
}
