// Define a function that is imported into the module.
// By default, the "env" namespace is used.

#[link(wasm_import_module = "nur")]
unsafe extern "C" {
    fn nur_send(ptr: *const u8, len: usize);
    fn nur_log(ptr: *const u8, len: usize);
    fn nur_end();
}

fn log(msg: &str) {
    unsafe {
        nur_log(msg.as_ptr(), msg.len());
    }
}

fn send(msg: &[u8]) {
    unsafe {
        nur_send(msg.as_ptr(), msg.len());
    }
}

// Abort doesn't really do something special, it just sends the signal to the host to end
// its lifecycle
fn end() {
    unsafe {
        nur_end();
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
            log(e.to_string().as_str());
            return end();
        }
    };
    let content_len = content.len();

    send(
        format!(
            "HTTP/1.1 200 OK\r
content-length: {content_len}\r
\r
{content}",
        )
        .as_bytes(),
    );
    end();
}

#[unsafe(no_mangle)]
pub extern "C" fn alloc(len: usize) -> usize {
    let layout = std::alloc::Layout::array::<u8>(len).unwrap();
    unsafe { std::alloc::alloc(layout) as usize }
}
