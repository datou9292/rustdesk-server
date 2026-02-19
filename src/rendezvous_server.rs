use hbb_common::{
    anyhow::Context,
    config::RENDEZVOUS_PORT,
    crypto::Crypto,
    futures::{self, stream::FuturesUnordered, StreamExt},
    log,
    net::{self, AsyncAccept, TcpListener, TcpStream},
    ResultType,
};
use std::net::SocketAddr;
use std::sync::Arc;

pub struct RendezvousServer;

impl RendezvousServer {
    pub async fn start(
        bind_addrs: &[SocketAddr],
        serial: i32,
        key: &str,
        rmem: usize,
    ) -> ResultType<()> {
        let key = Arc::new(Crypto::new_server(key)?);
        let mut listeners = FuturesUnordered::new();

        // 遍历所有绑定地址，启动监听（默认双栈）
        for addr in bind_addrs {
            let listener = TcpListener::bind(addr).await
                .with_context(|| format!("Failed to bind to {}", addr))?;
            log::info!("hbbs listening on {}", addr);
            listeners.push(listener.incoming());
        }

        // 处理客户端连接
        let key_clone = key.clone();
        tokio::spawn(async move {
            while let Some(stream) = listeners.next().await {
                match stream {
                    Ok(stream) => {
                        let key = key_clone.clone();
                        tokio::spawn(async move {
                            if let Err(e) = Self::handle_stream(stream, key).await {
                                log::error!("Handle stream error: {}", e);
                            }
                        });
                    }
                    Err(e) => {
                        log::error!("Accept error: {}", e);
                    }
                }
            }
        });

        // 保留原有 UDP 监听逻辑（无需修改）
        if rmem > 0 {
            let _ = net::set_udp_recv_buffer_size(rmem);
        }
        let udp_listener = net::UdpSocket::bind(("0.0.0.0", RENDEZVOUS_PORT)).await?;
        let udp_listener6 = net::UdpSocket::bind(("[::]", RENDEZVOUS_PORT)).await.ok();
        let udp_key = key.clone();
        tokio::spawn(async move {
            let mut buf = vec![0; 65536];
            loop {
                let (n, addr) = match udp_listener.recv_from(&mut buf).await {
                    Ok(v) => v,
                    Err(e) => {
                        log::error!("UDP recv error: {}", e);
                        continue;
                    }
                };
                let data = &buf[..n];
                let _ = crate::udp::handle(udp_key.clone(), data, addr, None).await;
            }
        });
        if let Some(udp_listener6) = udp_listener6 {
            let udp_key = key.clone();
            tokio::spawn(async move {
                let mut buf = vec![0; 65536];
                loop {
                    let (n, addr) = match udp_listener6.recv_from(&mut buf).await {
                        Ok(v) => v,
                        Err(e) => {
                            log::error!("UDP6 recv error: {}", e);
                            continue;
                        }
                    };
                    let data = &buf[..n];
                    let _ = crate::udp::handle(udp_key.clone(), data, addr, Some(6)).await;
                }
            });
        }

        crate::config::start(serial).await?;
        Ok(())
    }

    // 保留原有连接处理逻辑
    async fn handle_stream(stream: TcpStream, key: Arc<Crypto>) -> ResultType<()> {
        let peer = stream.peer_addr()?;
        log::info!("New connection from {}", peer);
        let mut stream = stream;
        stream.set_nodelay(true)?;
        let mut buf = vec![0; 1024];
        let n = stream.read(&mut buf).await?;
        if n == 0 {
            return Ok(());
        }
        let req = String::from_utf8_lossy(&buf[..n]);
        if req.starts_with("GET /") {
            let _ = crate::http::handle(stream, &req, key).await;
        } else {
            let _ = crate::tcp::handle(stream, key).await;
        }
        Ok(())
    }
}
