use std::{
    collections::{HashMap, hash_map::Entry},
    fmt::Debug,
    net::IpAddr,
    sync::Arc,
    time::Duration,
};

use async_trait::async_trait;
use eyre::{Report, Result, WrapErr};
use tokio::time::sleep;
use tracing::{info, warn};

use crate::{config::router::RouterConfig, router::traefik::TraefikRouter, server::Server};

pub mod traefik;

#[derive(Debug, Clone)]
pub struct Route {
    pub host: String,
    pub upstream: IpAddr,
}

#[async_trait(?Send)]
pub trait Router: Debug {
    async fn get_routes(&self) -> Result<Vec<Route>>;
}

impl TryFrom<&RouterConfig> for Box<dyn Router> {
    type Error = Report;

    fn try_from(value: &RouterConfig) -> Result<Self, Self::Error> {
        Ok(Box::new(match value {
            RouterConfig::Traefik(config) => TraefikRouter::try_from(config)?,
        }))
    }
}

pub async fn refresh_routes(srv: &'static Server) -> Result<()> {
    let routers = srv
        .config
        .routers
        .iter()
        .map(|c| c.try_into())
        .collect::<Result<Vec<Box<dyn Router>>>>()
        .context("collect routers")?;

    loop {
        let mut old_routes = (**srv.routes.read()).clone();
        let mut new_routes = HashMap::<String, Route>::new();
        for r in &routers {
            let routes = match r.get_routes().await {
                Ok(routes) => routes,
                Err(err) => {
                    warn!(router = ?r, ?err, "fetch routes");
                    continue;
                }
            };

            for r in routes {
                match new_routes.entry(r.host.clone()) {
                    Entry::Occupied(e) => {
                        if e.get().upstream != r.upstream {
                            warn!(host = r.host, "duplicate route");
                        }
                    }
                    Entry::Vacant(e) => {
                        if old_routes.remove(&r.host).is_none() {
                            info!(?r, "added route");
                        }
                        e.insert(r);
                    }
                }
            }
        }
        for r in old_routes.values() {
            info!(?r, "removed route");
        }
        *srv.routes.write() = Arc::new(new_routes);

        sleep(Duration::from_secs(10)).await;
    }
}
