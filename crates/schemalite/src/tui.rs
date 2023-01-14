use tui::{
    style::Style,
    text::{Span, Spans},
};

use crate::{
    ansi_sql_printer::{to_colored, SYNTAXES},
    Color, SqlPrinter,
};

impl SqlPrinter {
    pub fn print_spans(&mut self, sql: &str) -> Vec<Spans> {
        self.print_spans_inner(sql, None)
    }

    pub fn print_spans_on(&mut self, sql: &str, background: Color) -> Vec<Spans> {
        self.print_spans_inner(sql, Some(background))
    }

    fn print_spans_inner(&mut self, sql: &str, background: Option<Color>) -> Vec<Spans> {
        let spans = sql.split('\n').map(|line| {
            let line = format!("{}\n", line);
            let regions = self
                .highlighter
                .highlight_line(&line, SYNTAXES.get().unwrap())
                .unwrap();

            Spans(to_tui_colored(&regions[..], background))
        });
        spans.collect()
    }
}

fn to_tui_colored<'a>(
    v: &[(syntect::highlighting::Style, &str)],
    background: Option<crate::Color>,
) -> Vec<Span<'a>> {
    to_colored(
        v,
        background,
        |output: &mut Vec<Span>, style, text, background| {
            if style.foreground.a == 0 {
                let color: Color = style.foreground.r.into();
                let color: tui::style::Color = color.into();

                let colored = match background {
                    Some(background) => Span::styled(
                        text.to_owned(),
                        Style::default().fg(color).bg(background.into()),
                    ),
                    None => Span::styled(text.to_owned(), Style::default().fg(color)),
                };
                output.push(colored);
            } else if style.foreground.a == 1 {
                let text = match background {
                    Some(background) => {
                        Span::styled(text.to_owned(), Style::default().bg(background.into()))
                    }
                    None => Span::raw(text.to_owned()),
                };
                output.push(text);
            } else {
                let colored = match background {
                    Some(background) => Span::styled(
                        text.to_owned(),
                        Style::default()
                            .fg(tui::style::Color::Rgb(
                                style.foreground.r,
                                style.foreground.g,
                                style.foreground.b,
                            ))
                            .bg(background.into()),
                    ),
                    None => Span::styled(
                        text.to_owned(),
                        Style::default().fg(tui::style::Color::Rgb(
                            style.foreground.r,
                            style.foreground.g,
                            style.foreground.b,
                        )),
                    ),
                };

                output.push(colored);
            };
        },
    )
}
