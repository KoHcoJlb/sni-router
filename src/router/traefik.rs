use std::net::IpAddr;

use async_trait::async_trait;
use eyre::{Report, Result};
use secrecy::{ExposeSecret, SecretString};
use serde::Deserialize;
use tracing::{trace, warn};
use winnow::{
    Parser,
    combinator::{delimited, repeat, repeat_till},
    token::{any, take_till},
};

use crate::{
    config::expand_env,
    parser,
    router::{Route, Router},
};

const API_DOMAIN: &str = "traefik.local";

#[derive(Debug, Clone, Deserialize)]
pub struct TraefikRouterConfig {
    pub addr: IpAddr,
    #[serde(deserialize_with = "expand_env")]
    pub username: SecretString,
    #[serde(deserialize_with = "expand_env")]
    pub password: SecretString,
}

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
struct ApiRoute {
    name: String,
    status: String,
    rule: String,
}

fn host<'a>(buf: &mut &'a str) -> parser::Result<&'a str> {
    repeat_till(.., any, delimited("Host(`", take_till(1.., '`'), "`)"))
        .map(|((), host)| host)
        .parse_next(buf)
}

fn hosts<'a>(buf: &mut &'a str) -> parser::Result<Vec<&'a str>> {
    repeat(.., host).parse_next(buf)
}

fn parse_hosts(mut buf: &str) -> Result<Vec<&str>> {
    hosts.complete_err().parse_next(&mut buf).map_err(|e| e.into_inner().unwrap().into_inner())
}

pub struct TraefikRouter {
    config: TraefikRouterConfig,
    client: reqwest::Client,
}

impl TryFrom<&TraefikRouterConfig> for TraefikRouter {
    type Error = Report;

    fn try_from(value: &TraefikRouterConfig) -> Result<Self, Self::Error> {
        Ok(Self {
            config: value.clone(),
            client: reqwest::Client::builder()
                .resolve(API_DOMAIN, (value.addr, 0).into())
                .build()?,
        })
    }
}

#[async_trait(?Send)]
impl Router for TraefikRouter {
    async fn get_routes(&self) -> Result<Vec<Route>> {
        let resp: Vec<ApiRoute> = self
            .client
            .get(format!("http://{API_DOMAIN}/api/http/routers"))
            .basic_auth(
                self.config.username.expose_secret(),
                Some(self.config.password.expose_secret()),
            )
            .send()
            .await?
            .json()
            .await?;

        trace!(?resp);

        Ok(resp
            .iter()
            .filter_map(|r| parse_hosts(&r.rule).inspect_err(|err| warn!(?err, "parse rule")).ok())
            .flat_map(|hosts| {
                hosts
                    .into_iter()
                    .filter(|host| *host != API_DOMAIN)
                    .map(|host| Route { host: host.into(), upstream: self.config.addr })
            })
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use eyre::Result;

    use crate::router::traefik::parse_hosts;

    #[test]
    fn test_host_parser() -> Result<()> {
        parse_hosts("Host(`traefik.local`)")?;
        parse_hosts(
            "PathPrefix(`/api`) || Host(`test1.com`) && PathPrefix(`/dashboard`) || Host(`test2.com`)",
        )?;
        parse_hosts("Hostt(`.+`)")?;
        parse_hosts("")?;
        Ok(())
    }
}
