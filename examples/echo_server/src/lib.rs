// Define a function that is imported into the module.
// By default, the "env" namespace is used.

mod import {
    #[link(wasm_import_module = "nur")]
    unsafe extern "C" {
        pub fn nur_send(ptr: *const u8, len: usize);
        pub fn nur_log(ptr: *const u8, len: usize);
        pub fn nur_end();
    }
}

fn nur_log(msg: &str) {
    unsafe {
        import::nur_log(msg.as_ptr(), msg.len());
    }
}

fn nur_send(msg: &[u8]) {
    unsafe {
        import::nur_send(msg.as_ptr(), msg.len());
    }
}

// Abort doesn't really do something special, it just sends the signal to the host to end
// its lifecycle
fn nur_end() {
    unsafe {
        import::nur_end();
    }
}

// This will be called by wasmer
#[unsafe(no_mangle)]
pub extern "C" fn poll_stream(data: usize, len: usize) {
    let data: *const u8 = data as *const u8;
    let slice = unsafe { std::slice::from_raw_parts(data, len) };

    let content = match str::from_utf8(slice) {
        Ok(content) => content,
        Err(e) => {
            nur_log(e.to_string().as_str());
            return nur_end();
        }
    };
    let content_len = content.len();

    nur_log("Hola UX! El usuario nos ha enviado una petición!!");

    nur_send(
        format!(
            "HTTP/1.1 200 OK\r
content-length: {content_len}\r
\r
{content}",
        )
        .as_bytes(),
    );
    nur_end();
}

#[unsafe(no_mangle)]
pub extern "C" fn alloc(len: usize) -> usize {
    let layout = std::alloc::Layout::array::<u8>(len).unwrap();
    unsafe { std::alloc::alloc(layout) as usize }
}
