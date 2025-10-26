use serde::Deserialize;

use crate::router::traefik::TraefikRouterConfig;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RouterConfig {
    Traefik(TraefikRouterConfig),
}
