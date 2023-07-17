// #[cfg(feature = "application")]
// mod app;
// #[cfg(feature = "application")]
// mod app_tui;

// #[cfg(feature = "application")]
// #[tokio::main]
// pub async fn main() -> Result<(), color_eyre::eyre::Report> {
//     let app = app::App::from_args()?;
//     app.run().await
// }

// #[cfg(not(feature = "application"))]
pub fn main() {}
