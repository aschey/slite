use crate::Color;

#[derive(Default)]
pub struct SqlPrinter;

impl SqlPrinter {
    pub fn print(&mut self, sql: &str) -> String {
        sql.to_owned()
    }

    pub fn print_on(&mut self, sql: &str, _color: Color) -> String {
        sql.to_owned()
    }
}
