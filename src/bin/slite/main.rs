#[cfg(feature = "application")]
mod app;

#[cfg(feature = "application")]
pub fn main() -> Result<(), color_eyre::eyre::Report> {
    let (result, _scope) = rooibos::run_system(run);
    result
}

#[cfg(feature = "application")]
#[tokio::main]
pub async fn run(cx: rooibos::reactive::Scope) -> Result<(), color_eyre::eyre::Report> {
    use tilia::tower_rpc::{
        transport::{
            ipc::{self, OnConflict, SecurityAttributes},
            CodecTransport,
        },
        LengthDelimitedCodec,
    };
    use tracing_subscriber::{fmt::Layer, prelude::*, EnvFilter};

    let (ipc_writer, mut guard) = tilia::Writer::new(1024, move || {
        Box::pin(async move {
            let transport = ipc::create_endpoint(
                "slite",
                SecurityAttributes::allow_everyone_create().unwrap(),
                OnConflict::Overwrite,
            )
            .unwrap();
            CodecTransport::new(transport, LengthDelimitedCodec)
        })
    });

    tracing_subscriber::registry()
        .with(EnvFilter::from_default_env())
        .with({
            Layer::new()
                .compact()
                .with_writer(ipc_writer)
                .with_filter(tilia::Filter::default())
        })
        .init();

    let app = app::App::from_args()?;
    app.run(cx).await?;
    guard.stop().await.ok();
    Ok(())
}

#[cfg(not(feature = "application"))]
pub fn main() {}
