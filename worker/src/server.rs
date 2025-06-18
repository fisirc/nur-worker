use std::{io, net::SocketAddr};
use tokio::{io::{AsyncReadExt, AsyncWriteExt}, net::ToSocketAddrs};

use wasmer::{imports, AsStoreMut, AsStoreRef, FunctionEnv, FunctionEnvMut, Instance, Module, Store};

static WASM: &'static [u8] = include_bytes!("../test.wasm");

const EXPORTED_HANDLER_SYMBOL_NAME: &str = "handle_request";

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
        // TODO handshake
        // TODO fetch from S3
        // TODO unzpip

        let mut store = Store::default();
        let module = match Module::new(&store, WASM) {
            Ok(module) => module,
            Err(e) => {
                log::error!("Failed to compile WebAssembly module: {e}");
                // TODO: send error back
                return;
            },
        };

        struct FuncEnv {
            memory: Option<wasmer::Memory>,
        }

        let env = FunctionEnv::new(&mut store, FuncEnv {
            memory: None,
        });

        fn print_str(env: FunctionEnvMut<FuncEnv>, ptr: u32, len: i32) {
            let data = env.data();
            let store = env.as_store_ref();

            let memory = data.memory.as_ref().unwrap();
            let bytes = memory.view(&store);
            let slice = &bytes.copy_range_to_vec(ptr as u64..(ptr + len as u32) as u64).unwrap();
            let str_data = String::from_utf8_lossy(slice);
            log::info!("wa: {}", str_data);
        }

        let print_str_typed = wasmer::Function::new_typed_with_env(&mut store, &env, print_str);

        let import_object = imports! {
            "env" => {
                "print_str" => print_str_typed,
            },
        };

        let instance = match Instance::new(&mut store, &module, &import_object) {
            Ok(instance) => instance,
            Err(e) => {
                // TODO: send error back
                log::error!("Failed to instantiate WebAssembly module: {e}");
                return;
            },
        };

        env.as_mut(&mut store).memory = Some(instance.exports.get_memory("memory").unwrap().clone());

        let handle_request = match instance.exports.get_function(EXPORTED_HANDLER_SYMBOL_NAME) {
            Ok(func) => func,
            Err(e) => {
                log::error!("Failed to get exported function '{}': {}", EXPORTED_HANDLER_SYMBOL_NAME, e);
                return;
            },
        };

        let mut buf = vec![0; 1024];
        loop {
            match socket.read(&mut buf).await {
                Ok(0) => {
                    log::info!("Connection closed by {addr}");
                    break;
                },
                Ok(n) => {
                    handle_request.call(&mut store, &[]).unwrap();
                }
                Err(e) => {
                    log::error!("Error reading from socket: {}", e);
                    break;
                }
            }
        }
    }
}
