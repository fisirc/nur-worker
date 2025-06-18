use std::{io, net::SocketAddr};
use tokio::{io::{AsyncReadExt, AsyncWriteExt}, net::ToSocketAddrs};

pub struct Server {
    listener: tokio::net::TcpListener,
}

impl Server {
    pub async fn new<A: ToSocketAddrs>(addr: A) -> io::Result<Self> {
        Ok(Server {
            listener: tokio::net::TcpListener::bind(&addr).await?,
        })
    }

    pub async fn listen_forever_and_ever_amen(self) -> io::Result<()> {
        loop {
            let (socket, addr) = self.listener.accept().await?;
            log::info!("Accepted connection from {addr}\n");
            tokio::spawn(Server::handle_conn(socket, addr));
        }
    }

    async fn handle_conn(mut socket: tokio::net::TcpStream, addr: SocketAddr) {
        socket.write(":3\n".as_bytes()).await.unwrap();
        let mut buf = vec![0; 1024];
        loop {
            match socket.read(&mut buf).await {
                Ok(0) => {
                    log::info!("Connection closed by {addr}");
                    break;
                },
                Ok(n) => {
                    log::info!("Received: {}", String::from_utf8_lossy(&buf[..n]));
                    if socket.write_all(&buf[..n]).await.is_err() {
                        break;
                    }
                }
                Err(e) => {
                    log::error!("Error reading from socket: {}", e);
                    break;
                }
            }
        }
    }
}
