use eyre::{Result, eyre};
use tokio::try_join;
use tracing::info;
use tracing_subscriber::fmt;

use crate::{config::load_config, server::Server};

mod config;
mod listener;
mod parser;
mod router;
mod server;

async fn spawn(fut: impl Future<Output = Result<()>> + Send + Sync + 'static) -> Result<()> {
    tokio::spawn(fut).await.unwrap_or(Err(eyre!("join error")))
}

struct EyreHandler;

impl eyre::EyreHandler for EyreHandler {
    fn debug(
        &self, error: &(dyn std::error::Error + 'static), f: &mut std::fmt::Formatter<'_>,
    ) -> std::fmt::Result {
        std::fmt::Debug::fmt(error, f)
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    eyre::set_hook(Box::new(|_| Box::new(EyreHandler))).unwrap();

    fmt().with_env_filter("sni_router=info").init();

    let config = load_config()?;
    info!(?config);

    let server = Box::leak(Box::new(Server { config, routes: Default::default() }));

    try_join!(
        router::refresh_routes(server),
        spawn(listener::tls::run(server)),
        spawn(listener::http::run(server))
    )?;

    Ok(())
}
