use crate::Color;
use once_cell::sync::OnceCell;
use owo_colors::{AnsiColors, OwoColorize};
use syntect::{
    highlighting::{Style, ThemeSet},
    parsing::SyntaxSet,
};

pub(crate) static SYNTAXES: OnceCell<SyntaxSet> = OnceCell::new();
static THEMES: OnceCell<ThemeSet> = OnceCell::new();

pub struct SqlPrinter {
    pub(crate) highlighter: syntect::easy::HighlightLines<'static>,
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
    pub fn print(&mut self, sql: &str) -> String {
        self.print_inner(sql, None)
    }

    pub fn print_on(&mut self, sql: &str, color: Color) -> String {
        self.print_inner(sql, Some(color))
    }

    fn print_inner(&mut self, sql: &str, background: Option<Color>) -> String {
        sql.split('\n')
            .map(|line| {
                let line = format!("{}\n", line);
                let regions = self
                    .highlighter
                    .highlight_line(&line, SYNTAXES.get().unwrap())
                    .unwrap();

                to_ansi_colored(&regions[..], background)
            })
            .collect::<Vec<_>>()
            .join("")
    }
}

fn to_ansi_colored(v: &[(Style, &str)], background: Option<Color>) -> String {
    to_colored(
        v,
        background,
        |output: &mut String, style, text, background| {
            let background: Option<AnsiColors> = background.map(|b| b.into());
            if style.foreground.a == 0 {
                let color: Color = style.foreground.r.into();
                let color: AnsiColors = color.into();
                let colored = match background {
                    Some(background) => text.color(color).on_color(background).to_string(),
                    None => text.color(color).to_string(),
                };
                output.push_str(&colored);
            } else if style.foreground.a == 1 {
                let ends_with_newline = text.ends_with('\n');
                let text = text.replace('\n', "");
                let mut text = match background {
                    Some(background) => text.on_color(background).to_string(),
                    None => text,
                };
                if ends_with_newline {
                    text.push('\n');
                }
                output.push_str(&text);
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

                output.push_str(&colored);
            };
        },
    )
}

pub(crate) fn to_colored<O>(
    v: &[(Style, &str)],
    background: Option<Color>,
    transform: impl Fn(&mut O, &Style, &str, Option<Color>),
) -> O
where
    O: Default,
{
    let mut output = O::default();
    for &(ref style, text) in v.iter() {
        transform(&mut output, style, text, background);
    }

    output
}
