#[derive(Clone, Copy)]
pub enum Color {
    Black,
    Red,
    Green,
    Yellow,
    Blue,
    Magenta,
    Cyan,
    White,
    BrightBlack,
    BrightRed,
    BrightGreen,
    BrightYellow,
    BrightBlue,
    BrightMagenta,
    BrightCyan,
    BrightWhite,
}

impl From<u8> for Color {
    fn from(value: u8) -> Self {
        match value {
            0x00 => Self::Black,
            0x01 => Self::Red,
            0x02 => Self::Green,
            0x03 => Self::Yellow,
            0x04 => Self::Blue,
            0x05 => Self::Magenta,
            0x06 => Self::Cyan,
            0x07 => Self::White,
            0x08 => Self::BrightBlack,
            0x09 => Self::BrightRed,
            0x0A => Self::BrightGreen,
            0x0B => Self::BrightYellow,
            0x0C => Self::BrightBlue,
            0x0D => Self::BrightMagenta,
            0x0E => Self::BrightCyan,
            0x0F => Self::BrightWhite,
            _ => Self::White,
        }
    }
}

#[cfg(feature = "pretty-print")]
impl From<Color> for owo_colors::AnsiColors {
    fn from(value: Color) -> Self {
        match value {
            Color::Black => Self::Black,
            Color::Red => Self::Red,
            Color::Green => Self::Green,
            Color::Yellow => Self::Yellow,
            Color::Blue => Self::Blue,
            Color::Magenta => Self::Magenta,
            Color::Cyan => Self::Cyan,
            Color::White => Self::White,
            Color::BrightBlack => Self::BrightBlack,
            Color::BrightRed => Self::BrightRed,
            Color::BrightGreen => Self::BrightGreen,
            Color::BrightYellow => Self::BrightYellow,
            Color::BrightBlue => Self::BrightBlue,
            Color::BrightMagenta => Self::BrightMagenta,
            Color::BrightCyan => Self::BrightCyan,
            Color::BrightWhite => Self::BrightWhite,
        }
    }
}

#[cfg(feature = "tui")]
impl From<Color> for tui::style::Color {
    fn from(value: Color) -> Self {
        match value {
            Color::Black => Self::Black,
            Color::Red => Self::Red,
            Color::Green => Self::Green,
            Color::Yellow => Self::Yellow,
            Color::Blue => Self::Blue,
            Color::Magenta => Self::Magenta,
            Color::Cyan => Self::Cyan,
            Color::White => Self::White,
            Color::BrightBlack => Self::Black,
            Color::BrightRed => Self::LightRed,
            Color::BrightGreen => Self::LightGreen,
            Color::BrightYellow => Self::LightYellow,
            Color::BrightBlue => Self::LightBlue,
            Color::BrightMagenta => Self::LightMagenta,
            Color::BrightCyan => Self::LightCyan,
            Color::BrightWhite => Self::White,
        }
    }
}
