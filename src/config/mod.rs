use std::{
    env, fs,
    net::{IpAddr, SocketAddr},
};

use eyre::{Result, WrapErr};
use router::RouterConfig;
use serde::{
    Deserialize, Deserializer,
    de::{Error, value::CowStrDeserializer},
};
use smart_default::SmartDefault;

pub mod router;

pub fn expand_env<'de, D: Deserializer<'de>, T: Deserialize<'de>>(de: D) -> Result<T, D::Error> {
    let s = String::deserialize(de)?;
    let s = shellexpand::full(&s).map_err(Error::custom)?;
    T::deserialize(CowStrDeserializer::new(s))
}

#[derive(Debug, SmartDefault, Deserialize)]
#[serde(default)]
pub struct Listener {
    #[default = 80]
    pub http_port: u16,
    #[default = 443]
    pub https_port: u16,
}

#[derive(Debug, Deserialize)]
pub struct DefaultUpstream {
    pub addr: IpAddr,
    pub http_port: u16,
    pub https_port: u16,
}

impl DefaultUpstream {
    pub fn http(&self) -> SocketAddr {
        SocketAddr::new(self.addr, self.http_port)
    }
    pub fn https(&self) -> SocketAddr {
        SocketAddr::new(self.addr, self.https_port)
    }
}

#[derive(Debug, Deserialize)]
pub struct Config {
    #[serde(default)]
    pub listener: Listener,
    pub routers: Vec<RouterConfig>,
    pub default_upstream: DefaultUpstream,
}

pub fn load_config() -> Result<Config> {
    let path = env::var("SNI_ROUTER_CONFIG").unwrap_or_else(|_| "sni-router.toml".into());
    let text = fs::read_to_string(path).context("read config file")?;
    toml::from_str(&text).context("parse config file")
}
