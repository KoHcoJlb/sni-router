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
            if socket.read_buf(&mut buf).await? == 0 {
                return Ok(PeekResult::Default);
            }

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

#[cfg(test)]
mod tests {
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };

    use super::*;

    #[derive(Clone)]
    struct NoRoute {
        upstream: SocketAddr,
        last_len: Arc<AtomicUsize>,
    }

    impl Peek for NoRoute {
        fn peek(&self, buf: &ReadBuf) -> Result<Option<PeekResult>> {
            let len = buf.filled().len();
            // Fail deterministically rather than spinning if a zero-byte read is ignored.
            assert!(len > self.last_len.swap(len, Ordering::Relaxed), "reparsed unchanged input");
            Ok(None)
        }

        fn default_upstream(&self) -> SocketAddr {
            self.upstream
        }
    }

    #[tokio::test]
    async fn sniffing_stops_when_reads_make_no_progress() -> Result<()> {
        timeout(Duration::from_secs(2), async {
            for len in [4096, 8192, 0, 3] {
                let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).await?;
                let upstream = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).await?;
                let mut client = TcpStream::connect(listener.local_addr()?).await?;
                let (socket, _) = listener.accept().await?;
                let peek = NoRoute { upstream: upstream.local_addr()?, last_len: Arc::default() };
                let payload = vec![b'x'; len];
                let mut expected = proxy_protocol_prefix(client.local_addr()?).to_vec();
                expected.extend_from_slice(&payload);

                let connection = spawn(handle_connection_inner(peek, socket));
                let backend = async {
                    let (mut socket, _) = upstream.accept().await?;
                    let mut received = vec![0; expected.len()];
                    socket.read_exact(&mut received).await?;
                    assert_eq!(received, expected);

                    socket.write_all(b"ok").await?;
                    Result::<_>::Ok(())
                };
                let request = async {
                    client.write_all(&payload).await?;
                    if len < 4096 {
                        client.shutdown().await?;
                    }

                    let mut response = [0; 2];
                    client.read_exact(&mut response).await?;
                    assert_eq!(&response, b"ok");

                    client.shutdown().await?;
                    Result::<_>::Ok(())
                };

                tokio::try_join!(backend, request, async { connection.await? })?;
            }

            Result::<_>::Ok(())
        })
        .await?
    }
}
