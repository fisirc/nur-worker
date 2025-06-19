use std::ops::Range;
use wasmer::{AsStoreRef, FunctionEnvMut};

pub struct NurFunctionEnv {
    pub memory: Option<wasmer::Memory>,
    pub channel_tx: flume::Sender<NurWasmMessage>,
    // TODO: log service reference
}

pub enum NurWasmMessage {
    Abort,
    SendData { data: Vec<u8> },
}

pub fn nur_log(env: FunctionEnvMut<NurFunctionEnv>, ptr: i32, len: i32) {
    log::trace!("nur_nur_log({ptr}, {len})");
    let data = env.data();
    let store = env.as_store_ref();
    let memory = data.memory.as_ref().unwrap();
    let memory_view = memory.view(&store);

    let memory_slice = memory_view
        .copy_range_to_vec(Range {
            start: ptr as u64,
            end: (ptr + len) as u64,
        })
        .unwrap();

    let msg = String::from_utf8_lossy(memory_slice.as_slice());
    // TODO: send to log service
    println!("{msg}");
}

pub fn nur_send(env: FunctionEnvMut<NurFunctionEnv>, ptr: i32, len: i32) {
    log::trace!("nur_send({ptr}, {len})");
    let data = env.data();
    let store = env.as_store_ref();
    let memory = data.memory.as_ref().unwrap();
    let memory_view = memory.view(&store);

    let memory_slice = memory_view
        .copy_range_to_vec(Range {
            start: ptr as u64,
            end: (ptr + len) as u64,
        })
        .unwrap();

    data.channel_tx
        .send(NurWasmMessage::SendData {
            data: memory_slice.to_owned(),
        })
        .unwrap();
}

/// Aborts with the given message described by a fat ointer in memory.
pub fn nur_end(mut env: FunctionEnvMut<NurFunctionEnv>) {
    log::trace!("nur_end()");
    let data = env.data_mut();
    data.channel_tx.send(NurWasmMessage::Abort).unwrap();
}
