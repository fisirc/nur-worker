use std::io;
use tokio::{io::AsyncWriteExt, net::ToSocketAddrs};

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
            tokio::spawn(Server::handle_conn(socket));
        }
    }

    async fn handle_conn(mut socket: tokio::net::TcpStream) {
        socket.write(":3\n".as_bytes()).await.unwrap();
    }
}
