use once_cell::sync::OnceCell;
use owo_colors::{AnsiColors, OwoColorize};
use syntect::{
    highlighting::{Style, ThemeSet},
    parsing::SyntaxSet,
};

static SYNTAXES: OnceCell<SyntaxSet> = OnceCell::new();
static THEMES: OnceCell<ThemeSet> = OnceCell::new();

pub struct SqlPrinter {
    highlighter: syntect::easy::HighlightLines<'static>,
}

impl Default for SqlPrinter {
    fn default() -> Self {
        let syntax_set = SYNTAXES.get_or_init(|| {
            syntect::dumps::from_uncompressed_data(include_bytes!("../assets/sql.packdump"))
                .unwrap()
        });
        let themes = THEMES.get_or_init(|| {
            syntect::dumps::from_binary(include_bytes!("../assets/themes.themedump"))
        });
        let theme = themes.themes.get("ansi").unwrap();
        let sql_syntax = syntax_set.find_syntax_by_name("SQL").unwrap().to_owned();
        let highlighter = syntect::easy::HighlightLines::new(&sql_syntax, theme);

        Self { highlighter }
    }
}

impl SqlPrinter {
    pub fn print(&mut self, sql: &str, background: Option<AnsiColors>) -> String {
        sql.split('\n')
            .map(|line| {
                let line = format!("{}\n", line);
                let regions = self
                    .highlighter
                    .highlight_line(&line, SYNTAXES.get().unwrap())
                    .unwrap();

                as_terminal_escaped(&regions[..], background)
            })
            .collect::<Vec<_>>()
            .join("")
    }
}

fn as_terminal_escaped(v: &[(Style, &str)], background: Option<AnsiColors>) -> String {
    let mut s: String = String::new();
    for &(ref style, text) in v.iter() {
        if style.foreground.a == 0 {
            let color = match style.foreground.r {
                0x00 => AnsiColors::Black,
                0x01 => AnsiColors::Red,
                0x02 => AnsiColors::Green,
                0x03 => AnsiColors::Yellow,
                0x04 => AnsiColors::Blue,
                0x05 => AnsiColors::Magenta,
                0x06 => AnsiColors::Cyan,
                0x07 => AnsiColors::White,
                0x08 => AnsiColors::BrightBlack,
                0x09 => AnsiColors::BrightRed,
                0x0A => AnsiColors::BrightGreen,
                0x0B => AnsiColors::BrightYellow,
                0x0C => AnsiColors::BrightBlue,
                0x0D => AnsiColors::BrightMagenta,
                0x0E => AnsiColors::BrightCyan,
                0x0F => AnsiColors::BrightWhite,
                _ => AnsiColors::White,
            };
            let colored = match background {
                Some(background) => text.color(color).on_color(background).to_string(),
                None => text.color(color).to_string(),
            };
            s.push_str(&colored);
        } else if style.foreground.a == 1 {
            let ends_with_newline = text.ends_with('\n');
            let text = text.replace('\n', "");
            let mut text = match background {
                Some(background) => text.on_color(background).to_string(),
                None => text.to_owned(),
            };
            if ends_with_newline {
                text.push('\n');
            }
            s.push_str(&text);
        } else {
            let colored = match background {
                Some(background) => text
                    .color(owo_colors::Rgb(
                        style.foreground.r,
                        style.foreground.g,
                        style.foreground.b,
                    ))
                    .on_color(background)
                    .to_string(),
                None => text
                    .color(owo_colors::Rgb(
                        style.foreground.r,
                        style.foreground.g,
                        style.foreground.b,
                    ))
                    .to_string(),
            };

            s.push_str(&colored);
        }
    }

    s
}
