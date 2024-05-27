use rooibos::tui::style::{Color, Modifier, Style, Stylize};
use rooibos::tui::text::Span;
use rooibos::tui::widgets::{Block, BorderType};

pub fn panel(title: &'static str, focused: bool) -> Block {
    let modifier = if focused {
        Modifier::BOLD
    } else {
        Modifier::empty()
    };
    let border_fg = if focused {
        Color::Reset
    } else {
        Color::DarkGray
    };

    Block::bordered()
        .border_type(BorderType::Rounded)
        .border_style(Style::new().fg(border_fg))
        .title(Span::from(title).reset().add_modifier(modifier))
}
