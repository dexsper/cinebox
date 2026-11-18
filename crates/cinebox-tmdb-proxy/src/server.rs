//! The accept loop: HTTP/1.1 and h2c on one port, bounded connections, slow and idle clients cut off, graceful
//! shutdown.

use std::io;
use std::net::SocketAddr;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};
use std::time::Duration;

use axum::Router;
use hyper::Request;
use hyper::body::Incoming;
use hyper_util::rt::{TokioExecutor, TokioIo, TokioTimer};
use hyper_util::server::conn::auto;
use hyper_util::server::graceful::GracefulShutdown;
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::Semaphore;
use tokio::task::JoinSet;
use tokio::time::{Instant, Sleep};
use tower_service::Service;

/// Against slowloris; on HTTP/1.1 it also bounds the idle time between requests, since hyper starts it when it waits
/// for the next request head.
const HEADER_READ_TIMEOUT: Duration = Duration::from_secs(10);

/// A connection that moves no bytes for between one and two of these is closed, which covers idle h2c connections.
const IDLE_TIMEOUT: Duration = Duration::from_secs(90);

const SHUTDOWN_GRACE: Duration = Duration::from_secs(10);

/// Pause after a failed accept, as when out of file descriptors, so that the loop does not spin.
const ACCEPT_BACKOFF: Duration = Duration::from_millis(50);

const OVERLOADED_RESPONSE: &str = concat!(
    "HTTP/1.1 503 Service Unavailable\r\n",
    "retry-after: 1\r\n",
    "content-length: 0\r\n",
    "connection: close\r\n\r\n",
);

/// The client's socket address, put into each request's extensions.
#[derive(Debug, Clone, Copy)]
pub(crate) struct PeerAddr(pub(crate) SocketAddr);

/// Accepts connections and keeps track of the open ones.
struct Acceptor {
    builder: auto::Builder<TokioExecutor>,
    router: Router,
    slots: Arc<Semaphore>,
    graceful: GracefulShutdown,
    connections: JoinSet<()>,
}

/// A socket that fails reads once no bytes moved in either direction for a whole `IDLE_TIMEOUT` tick.
struct IdleIo {
    stream: TcpStream,
    tick: Pin<Box<Sleep>>,
    has_moved: bool,
}

/// Serves `router` until `shutdown` resolves, then lets open requests finish for a grace period.
pub(crate) async fn serve(
    listener: TcpListener,
    router: Router,
    max_connections: usize,
    shutdown: impl Future<Output = ()>,
) {
    let mut acceptor = Acceptor::new(router, max_connections);
    let mut shutdown = std::pin::pin!(shutdown);

    loop {
        let accepted = tokio::select! {
            accepted = listener.accept() => accepted,
            Some(_) = acceptor.connections.join_next() => continue,
            () = &mut shutdown => break,
        };

        match accepted {
            Ok((stream, peer)) => acceptor.accept(stream, peer),
            Err(error) => {
                tracing::warn!(%error, "cannot accept a connection");
                tokio::time::sleep(ACCEPT_BACKOFF).await;
            }
        }
    }

    drop(listener);
    acceptor.drain().await;
}

impl Acceptor {
    fn new(router: Router, max_connections: usize) -> Self {
        let mut builder = auto::Builder::new(TokioExecutor::new());
        builder.http1().timer(TokioTimer::new()).header_read_timeout(HEADER_READ_TIMEOUT);
        builder.http2().timer(TokioTimer::new());

        Self {
            builder,
            router,
            slots: Arc::new(Semaphore::new(max_connections)),
            graceful: GracefulShutdown::new(),
            connections: JoinSet::new(),
        }
    }

    fn accept(&mut self, stream: TcpStream, peer: SocketAddr) {
        if let Err(error) = stream.set_nodelay(true) {
            tracing::debug!(%error, "cannot set TCP_NODELAY");
        }

        let Ok(slot) = Arc::clone(&self.slots).try_acquire_owned() else {
            reject(&stream);
            return;
        };

        let router = self.router.clone();
        let service = hyper::service::service_fn(move |mut request: Request<Incoming>| {
            request.extensions_mut().insert(PeerAddr(peer));
            router.clone().call(request)
        });

        let connection = self.builder.serve_connection(TokioIo::new(IdleIo::new(stream)), service).into_owned();
        let connection = self.graceful.watch(connection);

        self.connections.spawn(async move {
            if let Err(error) = connection.await {
                tracing::debug!(%error, "connection closed with an error");
            }

            drop(slot);
        });
    }

    async fn drain(mut self) {
        let finished = tokio::time::timeout(SHUTDOWN_GRACE, self.graceful.shutdown()).await;

        if finished.is_err() {
            tracing::warn!("connections still open after the shutdown grace period are dropped");
        }

        self.connections.shutdown().await;
    }
}

impl IdleIo {
    fn new(stream: TcpStream) -> Self {
        let tick = Box::pin(tokio::time::sleep(IDLE_TIMEOUT));

        Self { stream, tick, has_moved: false }
    }

    /// Checked only while a read is pending, which hyper keeps on every open connection.
    fn poll_idle(&mut self, context: &mut Context<'_>) -> Poll<io::Result<()>> {
        while self.tick.as_mut().poll(context).is_ready() {
            if !self.has_moved {
                return Poll::Ready(Err(io::Error::from(io::ErrorKind::TimedOut)));
            }

            self.has_moved = false;
            self.tick.as_mut().reset(Instant::now() + IDLE_TIMEOUT);
        }

        Poll::Pending
    }
}

impl AsyncRead for IdleIo {
    fn poll_read(self: Pin<&mut Self>, context: &mut Context<'_>, buf: &mut ReadBuf<'_>) -> Poll<io::Result<()>> {
        let this = self.get_mut();
        let filled_before = buf.filled().len();
        let polled = Pin::new(&mut this.stream).poll_read(context, buf);

        if polled.is_pending() {
            return this.poll_idle(context);
        }

        if buf.filled().len() > filled_before {
            this.has_moved = true;
        }

        polled
    }
}

impl AsyncWrite for IdleIo {
    fn poll_write(self: Pin<&mut Self>, context: &mut Context<'_>, buf: &[u8]) -> Poll<io::Result<usize>> {
        let this = self.get_mut();
        let polled = Pin::new(&mut this.stream).poll_write(context, buf);
        this.has_moved |= matches!(polled, Poll::Ready(Ok(written)) if written > 0);

        polled
    }

    fn poll_write_vectored(
        self: Pin<&mut Self>,
        context: &mut Context<'_>,
        bufs: &[io::IoSlice<'_>],
    ) -> Poll<io::Result<usize>> {
        let this = self.get_mut();
        let polled = Pin::new(&mut this.stream).poll_write_vectored(context, bufs);
        this.has_moved |= matches!(polled, Poll::Ready(Ok(written)) if written > 0);

        polled
    }

    fn is_write_vectored(&self) -> bool {
        self.stream.is_write_vectored()
    }

    fn poll_flush(self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.get_mut().stream).poll_flush(context)
    }

    fn poll_shutdown(self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.get_mut().stream).poll_shutdown(context)
    }
}

/// Answers over the connection limit with a 503 and closes, without a task: a fresh socket has room in its send
/// buffer, so the write either goes through at once or is skipped.
fn reject(stream: &TcpStream) {
    if let Err(error) = stream.try_write(OVERLOADED_RESPONSE.as_bytes()) {
        tracing::debug!(%error, "cannot answer a connection over the limit");
    }
}

/// Resolves on Ctrl-C, or on SIGTERM on Unix.
pub(crate) async fn shutdown_signal() {
    let interrupt = async {
        if let Err(error) = tokio::signal::ctrl_c().await {
            tracing::warn!(%error, "cannot listen for Ctrl-C");
            std::future::pending::<()>().await;
        }
    };

    tokio::select! {
        () = interrupt => {}
        () = terminate() => {}
    }

    tracing::info!("shutting down");
}

#[cfg(unix)]
async fn terminate() {
    let mut terminate = match tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()) {
        Ok(signal) => signal,
        Err(error) => {
            tracing::warn!(%error, "cannot listen for SIGTERM");
            return std::future::pending().await;
        }
    };

    terminate.recv().await;
}

#[cfg(not(unix))]
async fn terminate() {
    std::future::pending::<()>().await;
}
