use std::{io, net::SocketAddr};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::ToSocketAddrs;
use tokio::select;

use crate::fetcher::FunctionFetcher;
use crate::{fetcher, intrinsics};
use wasmer::{FunctionEnv, Instance, Module, Store, imports};

// static WASM: &'static [u8] = include_bytes!("../test.wasm");

const EXPORTED_POLL_HANDLER_SYMBOL_NAME: &str = "poll_stream";
const EXPORTED_ALLOC_SYMBOL_NAME: &str = "alloc";

pub struct Server {
    listener: tokio::net::TcpListener,
    function_fetcher: FunctionFetcher,
}

impl Server {
    pub async fn new<A: ToSocketAddrs>(
        addr: A,
        function_fetcher: FunctionFetcher,
    ) -> io::Result<Self> {
        Ok(Server {
            listener: tokio::net::TcpListener::bind(&addr).await?,
            function_fetcher,
        })
    }

    pub async fn listen_forever_and_ever_amen(self) -> io::Result<()> {
        loop {
            let (socket, addr) = self.listener.accept().await?;
            log::info!("💌 Gateway request started {addr}\n");
            tokio::spawn(Server::handle_conn(
                socket,
                addr,
                self.function_fetcher.clone(),
            ));
        }
    }

    async fn handle_conn(
        socket: tokio::net::TcpStream,
        addr: SocketAddr,
        function_fetcher: fetcher::FunctionFetcher,
    ) {
        let func_name = "echo_server";
        // TODO handshake

        /*
        function_id:
        function_deployment_id:
        */

        log::debug!("start:function_fetcher.fetch");
        let wasm_bytes = match function_fetcher.fetch(func_name).await {
            Ok(bytes) => bytes,
            Err(e) => {
                // TODO: send error back
                // Probably worth implementing a default handler for this kind of cases
                log::error!("Could not fetch function \"{func_name}\": {e:?}");
                return;
            }
        };
        log::debug!("finish:function_fetcher.fetch");

        let (mut socket_read_half, mut socket_write_half) = socket.into_split();

        log::debug!("start:wasm_module_instantiating");
        let mut store = Store::default();
        let module = match Module::new(&store, wasm_bytes) {
            Ok(module) => module,
            Err(e) => {
                log::error!("Failed to compile WebAssembly module: {e}");
                // TODO: send error back
                // Probably worth implementing a default handler for this kind of cases
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

        let instance_memory = match instance.exports.get_memory("memory") {
            Ok(mem) => mem.clone(),
            Err(e) => {
                log::error!("Unable to get WASM memory. Aborting: {e}");
                return;
            }
        };

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
        log::debug!("end:wasm_module_instantiating");

        let mut buf = vec![0; 1024];

        let listen_wasm_messages_task = tokio::spawn(async move {
            loop {
                let msg = rx.recv_async().await;
                match msg {
                    Ok(intrinsics::NurWasmMessage::Abort) => {
                        let _ = socket_write_half.shutdown().await;
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

        let read_socket_task = tokio::spawn(async move {
            loop {
                let read_n = { socket_read_half.read(&mut buf).await };

                match read_n {
                    Ok(0) => {
                        break;
                    }
                    Ok(n) => {
                        let ptr = match wasm_alloc.call(&mut store, &[wasmer::Value::I32(n as i32)])
                        {
                            Ok(ptr) => ptr,
                            Err(e) => {
                                log::error!("Call error: alloc({n}): {e}");
                                return;
                            }
                        };

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

                        match instance_memory
                            .view(&store)
                            .write(ptr_num as u64, &buf[..n])
                        {
                            Ok(_) => {}
                            Err(e) => {
                                log::error!("Failed to write to WASM memory at &{ptr_num}: {}", e);
                                return;
                            }
                        };

                        match wasm_poll_stream.call(
                            &mut store,
                            &[wasmer::Value::I32(ptr_num), wasmer::Value::I32(n as i32)],
                        ) {
                            Ok(_) => {}
                            Err(e) => {
                                log::error!("Call error: poll_stream({ptr_num}, {n}): {e}",);
                                return;
                            }
                        };
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
                log::debug!("listen_wasm_messages_task done for {addr}");
            },
            _ = read_socket_task => {
                log::debug!("read_socket_task done for {addr}");
            }
        }
    }
}
