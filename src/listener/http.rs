use std::net::SocketAddr;

use eyre::Result;
use httparse::{EMPTY_HEADER, Request};
use tokio::io::ReadBuf;
use tracing::debug;

use crate::{
    listener::{tcp, tcp::PeekResult},
    server::Server,
};

#[derive(Clone)]
struct Peek {
    srv: &'static Server,
}

impl tcp::Peek for Peek {
    fn peek(&self, buf: &ReadBuf) -> Result<Option<PeekResult>> {
        let mut headers = [EMPTY_HEADER; 64];
        let mut req = Request::new(&mut headers);
        if let Err(err) = req.parse(buf.filled()) {
            debug!(?err, "parse http header");
        } else if let Some(host) = req.headers.iter().find(|h| h.name.eq_ignore_ascii_case("host"))
        {
            return Ok(Some(
                self.srv
                    .routes
                    .read()
                    .get(str::from_utf8(host.value)?)
                    .map_or(PeekResult::Default, |r| {
                        PeekResult::Upstream(SocketAddr::new(r.upstream, 80))
                    }),
            ));
        }

        Ok(None)
    }

    fn default_upstream(&self) -> SocketAddr {
        self.srv.config.default_upstream.http()
    }
}

pub async fn run(srv: &'static Server) -> Result<()> {
    tcp::run(Peek { srv }, srv.config.listener.http_port).await
}
