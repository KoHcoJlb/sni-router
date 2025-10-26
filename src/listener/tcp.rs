use std::{
    net::{Ipv4Addr, SocketAddr},
    time::Duration,
};

use eyre::{Result, WrapErr};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt, ReadBuf, copy_bidirectional_with_sizes},
    net::{TcpListener, TcpStream},
    spawn,
    time::timeout,
};
use tracing::debug;

use crate::listener::proxy_protocol_prefix;

#[derive(Debug)]
pub enum PeekResult {
    Upstream(SocketAddr),
    Default,
}

pub trait Peek: Clone + Send + Sync + 'static {
    fn peek(&self, buf: &ReadBuf) -> Result<Option<PeekResult>>;

    fn default_upstream(&self) -> SocketAddr;
}

async fn handle_connection_inner(peek: impl Peek, mut socket: TcpStream) -> Result<()> {
    let mut buf = [0; 4096];
    let mut buf = ReadBuf::new(&mut buf);

    let upstream = timeout(Duration::from_secs(5), async {
        loop {
            socket.read_buf(&mut buf).await?;

            if let Some(res) = peek.peek(&buf)? {
                return Result::<_>::Ok(res);
            }
        }
    })
    .await
    .unwrap_or(Ok(PeekResult::Default))?;

    debug!(?upstream);
    let mut upstream = TcpStream::connect(match upstream {
        PeekResult::Upstream(addr) => addr,
        PeekResult::Default => peek.default_upstream(),
    })
    .await
    .context("connect to upstream")?;

    upstream.write_all(&proxy_protocol_prefix(socket.peer_addr()?)).await?;
    upstream.write_all(buf.filled()).await?;

    #[cfg(target_os = "linux")]
    realm_io::bidi_zero_copy(&mut socket, &mut upstream).await?;
    #[cfg(not(target_os = "linux"))]
    copy_bidirectional_with_sizes(&mut socket, &mut upstream, 32 * 1024, 32 * 1024).await?;

    Ok(())
}

async fn handle_connection(peek: impl Peek, socket: TcpStream) {
    if let Err(err) = handle_connection_inner(peek, socket).await {
        debug!(?err, "handle connection");
    }
}

pub async fn run(peek: impl Peek, port: u16) -> Result<()> {
    let listener = TcpListener::bind((Ipv4Addr::UNSPECIFIED, port)).await?;

    loop {
        let (socket, _) = listener.accept().await?;
        spawn(handle_connection(peek.clone(), socket));
    }
}
