use tui::{
    style::{Color, Modifier, Style},
    text::Span,
    widgets::{Block, BorderType, Borders},
};

pub trait BiPanel {
    fn left_next(&mut self);
    fn right_next(&mut self);
    fn left_previous(&mut self);
    fn right_previous(&mut self);
}

pub fn next(bipanel: &mut impl BiPanel, state: &BiPanelState) {
    if state.focused_index == 0 {
        bipanel.left_next();
    } else {
        bipanel.right_next();
    }
}

pub fn previous(bipanel: &mut impl BiPanel, state: &BiPanelState) {
    if state.focused_index == 0 {
        bipanel.left_previous();
    } else {
        bipanel.right_previous();
    }
}

#[derive(Debug, Clone, Default)]
pub struct BiPanelState {
    focused_index: u8,
}

impl BiPanelState {
    pub fn toggle_focus(&mut self) {
        self.focused_index = (self.focused_index + 1) % 2;
    }

    pub fn left_block<'a, 'b>(&self, title: &'a str) -> Block<'b>
    where
        'a: 'b,
    {
        self.block(title, self.focused_index == 0)
    }

    pub fn right_block<'a, 'b>(&self, title: &'a str) -> Block<'b>
    where
        'a: 'b,
    {
        self.block(title, self.focused_index == 1)
    }

    fn block<'a, 'b>(&self, title: &'a str, focused: bool) -> Block<'b>
    where
        'a: 'b,
    {
        let modifier = if focused {
            Modifier::BOLD | Modifier::ITALIC
        } else {
            Modifier::empty()
        };
        let border_fg = if focused { Color::Green } else { Color::White };

        Block::default()
            .title(Span::styled(title, Style::default().add_modifier(modifier)))
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(border_fg))
    }
}
