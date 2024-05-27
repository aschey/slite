#[cfg(feature = "application")]
mod app;

#[cfg(feature = "application")]
#[rooibos::main]
pub async fn main() -> Result<(), color_eyre::eyre::Report> {
    use tilia::tower_rpc::transport::ipc::{self, OnConflict, SecurityAttributes};
    use tilia::tower_rpc::transport::CodecTransport;
    use tilia::tower_rpc::LengthDelimitedCodec;
    use tracing_subscriber::fmt::Layer;
    use tracing_subscriber::prelude::*;
    use tracing_subscriber::EnvFilter;

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
    app.run().await?;
    guard.stop().await.ok();
    Ok(())
}

#[cfg(not(feature = "application"))]
pub fn main() {}
