use rooibos::dom::{col, row, Constrainable, Render};
use rooibos::reactive::effect::Effect;
use rooibos::reactive::signal::signal;
use rooibos::tui::style::Color;

use crate::tui::components::{objects_list, sql, StyledObject, StyledObjects};
use crate::ObjectType;

pub fn sql_objects(title: &'static str, id: &'static str) -> impl Render {
    let (objects, set_objects) = signal(StyledObjects::from_iter(vec![
        (
            ObjectType::Table,
            vec![StyledObject {
                object: "test".to_string(),
                foreground: Color::Reset,
            }],
        ),
        (
            ObjectType::Trigger,
            vec![StyledObject {
                object: "test".to_string(),
                foreground: Color::Reset,
            }],
        ),
        (
            ObjectType::Index,
            vec![StyledObject {
                object: "test".to_string(),
                foreground: Color::Reset,
            }],
        ),
        (
            ObjectType::View,
            vec![StyledObject {
                object: "test".to_string(),
                foreground: Color::Reset,
            }],
        ),
    ]));

    let (sql_view, set_sql_view) = signal("test".to_owned());

    row![col![objects_list(title, objects)].length(20), sql(sql_view)]
}
