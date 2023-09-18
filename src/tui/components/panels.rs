use rooibos::prelude::*;

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

    prop! {
        <Block
            borders=Borders::ALL
            border_type=BorderType::Rounded
            border_style=prop!(<Style fg=border_fg/>)
            title=prop! {
                <Span reset add_modifier=modifier>
                    {title}
                </Span>
            }
        />
    }
}
