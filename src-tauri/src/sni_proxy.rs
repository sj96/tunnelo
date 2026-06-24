//! TCP SNI passthrough router on 127.0.0.1:443.

use crate::sni_tls::await_routing_hostname;
use anyhow::{Context, Result};
use std::sync::Arc;
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::watch;
use tokio::task::JoinHandle;

pub type ResolveFn = Arc<dyn Fn(u16, &str) -> Option<u16> + Send + Sync>;

pub struct SniProxy {
    shutdown: watch::Sender<bool>,
    task: Option<JoinHandle<()>>,
}

impl SniProxy {
    pub fn start(resolve: ResolveFn) -> Result<Self> {
        let (tx, rx) = watch::channel(false);
        let task = tokio::spawn(run_listener(443, resolve, rx));
        Ok(Self {
            shutdown: tx,
            task: Some(task),
        })
    }

    pub fn stop(&mut self) {
        let _ = self.shutdown.send(true);
        if let Some(task) = self.task.take() {
            task.abort();
        }
    }
}

impl Drop for SniProxy {
    fn drop(&mut self) {
        self.stop();
    }
}

async fn run_listener(port: u16, resolve: ResolveFn, mut shutdown: watch::Receiver<bool>) {
    let listener = match TcpListener::bind(("127.0.0.1", port)).await {
        Ok(l) => l,
        Err(e) => {
            tracing::error!("SNI router failed to bind 127.0.0.1:{port}: {e:#}");
            return;
        }
    };
    tracing::info!("SNI router listening on 127.0.0.1:{port}");

    loop {
        tokio::select! {
            _ = shutdown.changed() => {
                if *shutdown.borrow() { break; }
            }
            accept = listener.accept() => {
                let Ok((mut client, _)) = accept else { break };
                let resolve = resolve.clone();
                tokio::spawn(async move {
                    let Some(hostname) = await_routing_hostname(&mut client).await else {
                        tracing::warn!("SNI: could not determine hostname on port {port}");
                        return;
                    };
                    let Some(backend_port) = resolve(port, &hostname) else {
                        tracing::warn!("SNI: no route for {hostname}:{port}");
                        return;
                    };
                    let Ok(mut backend) = TcpStream::connect(("127.0.0.1", backend_port)).await else {
                        tracing::warn!("SNI: backend 127.0.0.1:{backend_port} unreachable");
                        return;
                    };
                    if let Err(e) = proxy_bidirectional(&mut client, &mut backend).await {
                        tracing::debug!("SNI proxy closed: {e}");
                    }
                });
            }
        }
    }
}

async fn proxy_bidirectional(a: &mut TcpStream, b: &mut TcpStream) -> Result<()> {
    let (mut ar, mut aw) = a.split();
    let (mut br, mut bw) = b.split();
    let c1 = tokio::io::copy(&mut ar, &mut bw);
    let c2 = tokio::io::copy(&mut br, &mut aw);
    tokio::select! {
        r = c1 => { r.context("client→backend")?; }
        r = c2 => { r.context("backend→client")?; }
    }
    Ok(())
}
