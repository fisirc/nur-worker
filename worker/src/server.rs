use aws_config::BehaviorVersion;
use std::{io, net::SocketAddr};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::ToSocketAddrs;
use tokio::select;

use crate::intrinsics;
use wasmer::{FunctionEnv, Instance, Module, Store, imports};

static WASM: &'static [u8] = include_bytes!("../test.wasm");

const EXPORTED_POLL_HANDLER_SYMBOL_NAME: &str = "poll_stream";
const EXPORTED_ALLOC_SYMBOL_NAME: &str = "alloc";

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

    async fn handle_conn(socket: tokio::net::TcpStream, addr: SocketAddr) {
        // TODO handshake
        // TODO fetch from S3
        // TODO unzpip

        let config = aws_config::load_defaults(BehaviorVersion::v2025_01_17()).await;
        let client = aws_sdk_s3::Client::new(&config);

        client
            .get_object()
            .bucket("nur-storage")
            .key("builds/nur-worker.zip")
            .send()
            .await;

        let (mut socket_read_half, mut socket_write_half) = socket.into_split();

        log::warn!("Instatiating wasm module...");
        let mut store = Store::default();
        let module = match Module::new(&store, WASM) {
            Ok(module) => module,
            Err(e) => {
                log::error!("Failed to compile WebAssembly module: {e}");
                // TODO: send error back
                return;
            }
        };

        let (tx, rx) = flume::unbounded::<intrinsics::NurWasmMessage>();

        let func_env = FunctionEnv::new(
            &mut store,
            intrinsics::NurFunctionEnv {
                memory: None,
                channel_tx: tx,
            },
        );

        let import_object = imports! {
            "nur" => {
                "nur_log" => wasmer::Function::new_typed_with_env(&mut store, &func_env, intrinsics::nur_log),
                "nur_send" => wasmer::Function::new_typed_with_env(&mut store, &func_env, intrinsics::nur_send),
                "nur_end" => wasmer::Function::new_typed_with_env(&mut store, &func_env, intrinsics::nur_end),
            },
        };

        let instance = match Instance::new(&mut store, &module, &import_object) {
            Ok(instance) => instance,
            Err(e) => {
                // TODO: send error back
                log::error!("Failed to instantiate WebAssembly module: {e}");
                return;
            }
        };

        let instance_memory = instance.exports.get_memory("memory").unwrap().clone();

        func_env.as_mut(&mut store).memory = Some(instance_memory.clone());

        let wasm_poll_stream = match instance
            .exports
            .get_function(EXPORTED_POLL_HANDLER_SYMBOL_NAME)
        {
            Ok(func) => func.clone(),
            Err(e) => {
                log::error!(
                    "Failed to get exported function '{}': {}",
                    EXPORTED_POLL_HANDLER_SYMBOL_NAME,
                    e
                );
                return;
            }
        };

        let wasm_alloc = match instance.exports.get_function(EXPORTED_ALLOC_SYMBOL_NAME) {
            Ok(func) => func.clone(),
            Err(e) => {
                log::error!(
                    "Failed to get exported function '{}': {}",
                    EXPORTED_ALLOC_SYMBOL_NAME,
                    e
                );
                return;
            }
        };
        log::warn!("Wasm module instantiated!");

        let mut buf = vec![0; 1024];

        let listen_wasm_messages_task = tokio::spawn(async move {
            loop {
                let msg = rx.recv_async().await;
                match msg {
                    Ok(intrinsics::NurWasmMessage::Abort) => {
                        log::info!("Aborting connection with {addr}");
                        socket_write_half.shutdown().await.unwrap();
                        break;
                    }
                    Ok(intrinsics::NurWasmMessage::SendData { data }) => {
                        if let Err(e) = socket_write_half.write(&data).await {
                            log::error!("Failed to send data to {addr}: {e}");
                            break;
                        }
                    }
                    Err(flume::RecvError::Disconnected) => {
                        log::info!("Channel closed, aborting connection with {addr}");
                        break;
                    }
                }
            }
        });

        let read_connection = tokio::spawn(async move {
            loop {
                let read_n = { socket_read_half.read(&mut buf).await };

                match read_n {
                    Ok(0) => {
                        log::info!("Connection closed by {addr}");
                        break;
                    }
                    Ok(n) => {
                        // TODO: unwraps should abort the execution safely
                        let ptr = wasm_alloc
                            .call(&mut store, &[wasmer::Value::I32(n as i32)])
                            .unwrap();

                        let ptr = &ptr[0];

                        let ptr_num = *match ptr {
                            wasmer::Value::I32(num) => num,
                            _ => {
                                log::error!(
                                    "Expected I32 return value from alloc function. Aborting."
                                );
                                return;
                            }
                        };

                        instance_memory
                            .view(&store)
                            .write(ptr_num as u64, &buf[..n])
                            .unwrap();

                        wasm_poll_stream
                            .call(
                                &mut store,
                                &[wasmer::Value::I32(ptr_num), wasmer::Value::I32(n as i32)],
                            )
                            .unwrap();
                    }
                    Err(e) => {
                        log::error!("Error reading from socket: {}", e);
                        break;
                    }
                };
            }
        });

        select! {
            _ = listen_wasm_messages_task => {
                log::info!("WASM message listener task completed for {addr}");
            },
            _ = read_connection => {
                log::info!("Read connection task completed for {addr}");
            }
        }
    }
}
