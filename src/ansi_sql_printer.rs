use crate::{Color, OUTPUT_IS_TTY};
use once_cell::sync::OnceCell;
use owo_colors::{AnsiColors, OwoColorize};
use syntect::{
    easy::HighlightLines,
    highlighting::{Style, ThemeSet},
    parsing::SyntaxSet,
};
use tracing::error;

pub(crate) static SYNTAXES: OnceCell<SyntaxSet> = OnceCell::new();
static THEMES: OnceCell<ThemeSet> = OnceCell::new();

pub struct SqlPrinter {
    pub(crate) highlighter: HighlightLines<'static>,
}

impl Default for SqlPrinter {
    fn default() -> Self {
        let syntax_set = SYNTAXES.get_or_init(|| {
            syntect::dumps::from_uncompressed_data(include_bytes!("../assets/sqlite.packdump"))
                .expect("failed to load syntaxes")
        });
        let themes = THEMES.get_or_init(|| {
            syntect::dumps::from_binary(include_bytes!("../assets/themes.themedump"))
        });
        let theme = themes
            .themes
            .get("ansi")
            .expect("Failed to load ansi theme");
        let sql_syntax = syntax_set
            .find_syntax_by_name("SQL")
            .expect("Failed to load SQL syntax")
            .to_owned();
        let highlighter = HighlightLines::new(&sql_syntax, theme);

        Self { highlighter }
    }
}

impl SqlPrinter {
    pub fn print(&mut self, sql: &str) -> String {
        if *OUTPUT_IS_TTY {
            self.print_inner(sql, None)
        } else {
            sql.to_owned()
        }
    }

    pub fn print_on(&mut self, sql: &str, color: Color) -> String {
        if *OUTPUT_IS_TTY {
            self.print_inner(sql, Some(color))
        } else {
            sql.to_owned()
        }
    }

    fn print_inner(&mut self, sql: &str, background: Option<Color>) -> String {
        let formatted = sql
            .split('\n')
            .map(|line| {
                let line = format!("{}\n", line.replace("    ", " "));
                let regions = self
                    .highlighter
                    .highlight_line(&line, SYNTAXES.get().expect("Syntaxes weren't initialized"))?;

                Ok(to_ansi_colored(&regions[..], background))
            })
            .collect::<Result<Vec<_>, syntect::Error>>();
        match formatted {
            Ok(parts) => parts.join(""),
            Err(e) => {
                error!("Error highligting sql {sql}: {e}");
                sql.to_owned()
            }
        }
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
                    Some(background) => text.black().on_color(background).to_string(),
                    None => text.color(color).to_string(),
                };
                output.push_str(&colored);
            } else if style.foreground.a == 1 {
                let ends_with_newline = text.ends_with('\n');
                let text = text.replace('\n', "");
                let mut text = match background {
                    Some(background) => text.black().on_color(background).to_string(),
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
