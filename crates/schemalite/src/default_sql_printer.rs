#[derive(Default)]
pub struct SqlPrinter;

impl SqlPrinter {
    pub fn print(&mut self, sql: &str) -> String {
        sql.to_owned()
    }
}
