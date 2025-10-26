use std::net::SocketAddr;

use eyre::Result;
use tokio::io::ReadBuf;
use tracing::debug;
use winnow::{Partial, error::ErrMode};

use crate::{
    listener::{tcp, tcp::PeekResult},
    parser::tls::{Extension, Handshake, RecordData, tls_record},
    server::Server,
};

#[derive(Clone)]
struct Peek {
    srv: &'static Server,
}

impl tcp::Peek for Peek {
    fn peek(&self, buf: &ReadBuf) -> Result<Option<PeekResult>> {
        match tls_record(&mut Partial::new(buf.filled())) {
            Ok(record) => {
                if let RecordData::Handshake(Handshake::ClientHello(hello)) = record.data {
                    for ext in hello.extensions {
                        if let Extension::ServerName(sni) = ext {
                            return Ok(Some(
                                self.srv.routes.read().get(&sni).map_or(PeekResult::Default, |r| {
                                    PeekResult::Upstream(SocketAddr::new(r.upstream, 443))
                                }),
                            ));
                        }
                    }
                }

                return Ok(None);
            }
            Err(ErrMode::Incomplete(_)) => {}
            Err(err) => {
                debug!(?err, "parse tls record");
            }
        }

        Ok(None)
    }

    fn default_upstream(&self) -> SocketAddr {
        self.srv.config.default_upstream.https()
    }
}

pub async fn run(srv: &'static Server) -> Result<()> {
    tcp::run(Peek { srv }, srv.config.listener.https_port).await
}
