use std::fmt::{Display, Write};
use std::hash::Hash;
use std::ops::Range;

use imara_diff::intern::{InternedInput, Interner, Token};
use imara_diff::Sink;
use owo_colors::OwoColorize;
use tracing::error;

use crate::{Color, SqlPrinter};

pub struct UnifiedDiffBuilder<'a, W, T>
where
    W: Write,
    T: Hash + Eq + Display,
{
    before: &'a [Token],
    after: &'a [Token],
    interner: &'a Interner<T>,

    pos: u32,
    before_hunk_start: u32,
    after_hunk_start: u32,
    before_hunk_len: u32,
    after_hunk_len: u32,

    buffer: String,
    dst: W,

    sql_printer: SqlPrinter,
}

impl<'a, T> UnifiedDiffBuilder<'a, String, T>
where
    T: Hash + Eq + Display,
{
    pub fn new(input: &'a InternedInput<T>) -> Self {
        Self {
            before_hunk_start: 0,
            after_hunk_start: 0,
            before_hunk_len: 0,
            after_hunk_len: 0,
            buffer: String::with_capacity(8),
            dst: String::new(),
            interner: &input.interner,
            before: &input.before,
            after: &input.after,
            pos: 0,
            sql_printer: SqlPrinter::default(),
        }
    }
}

enum DiffType {
    Add,
    Remove,
    None,
}

impl<'a, W, T> UnifiedDiffBuilder<'a, W, T>
where
    W: Write,
    T: Hash + Eq + Display,
{
    fn print_tokens(
        &mut self,
        tokens: &[Token],
        diff_type: DiffType,
    ) -> Result<(), std::fmt::Error> {
        for &token in tokens {
            let raw_token = &self.interner[token];
            let line = match diff_type {
                DiffType::Add => format!(
                    "{}{}",
                    "+ ".white().on_green(),
                    self.sql_printer
                        .print_on(&format!("{}", raw_token), Color::Green)
                )
                .to_string(),
                DiffType::Remove => format!(
                    "{}{}",
                    "- ".white().on_red(),
                    self.sql_printer
                        .print_on(&format!("{}", raw_token), Color::Red)
                )
                .to_string(),
                DiffType::None => self.sql_printer.print(&format!("  {}", raw_token)),
            };

            write!(&mut self.buffer, "{}", line)?;
        }
        Ok(())
    }

    fn process_change(
        &mut self,
        before: Range<u32>,
        after: Range<u32>,
    ) -> Result<(), std::fmt::Error> {
        if before.start - self.pos > 6 {
            self.flush()?;
            self.pos = before.start - 3;
            self.before_hunk_start = self.pos;
            self.after_hunk_start = after.start - 3;
        }
        self.update_pos(before.start, before.end)?;
        self.before_hunk_len += before.end - before.start;
        self.after_hunk_len += after.end - after.start;
        self.print_tokens(
            &self.before[before.start as usize..before.end as usize],
            DiffType::Remove,
        )?;
        self.print_tokens(
            &self.after[after.start as usize..after.end as usize],
            DiffType::Add,
        )?;

        Ok(())
    }

    fn flush(&mut self) -> Result<(), std::fmt::Error> {
        if self.before_hunk_len == 0 && self.after_hunk_len == 0 {
            return Ok(());
        }

        let end = (self.pos + 3).min(self.before.len() as u32);
        self.update_pos(end, end)?;

        let header = format!(
            "@@ -{},{} +{},{} @@",
            self.before_hunk_start + 1,
            self.before_hunk_len,
            self.after_hunk_start + 1,
            self.after_hunk_len,
        )
        .cyan()
        .to_string();
        writeln!(&mut self.dst, "{}", header)?;
        write!(&mut self.dst, "{}", &self.buffer)?;
        self.buffer.clear();
        self.before_hunk_len = 0;
        self.after_hunk_len = 0;
        Ok(())
    }

    fn update_pos(&mut self, print_to: u32, move_to: u32) -> Result<(), std::fmt::Error> {
        self.print_tokens(
            &self.before[self.pos as usize..print_to as usize],
            DiffType::None,
        )?;
        let len = print_to - self.pos;
        self.pos = move_to;
        self.before_hunk_len += len;
        self.after_hunk_len += len;

        Ok(())
    }
}

impl<W, T> Sink for UnifiedDiffBuilder<'_, W, T>
where
    W: Write,
    T: Hash + Eq + Display,
{
    type Out = W;

    fn process_change(&mut self, before: Range<u32>, after: Range<u32>) {
        if let Err(e) = self.process_change(before, after) {
            error!("Error processing change: {e}");
        }
    }

    fn finish(mut self) -> Self::Out {
        if let Err(e) = self.flush() {
            error!("Error flushing: {e}");
        }
        self.dst
    }
}
