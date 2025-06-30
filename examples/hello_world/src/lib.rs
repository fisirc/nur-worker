// Define a function that is imported into the module.
// By default, the "env" namespace is used.

mod import {
    #[link(wasm_import_module = "nur")]
    unsafe extern "C" {
        pub fn nur_log(ptr: *const u8, len: usize);
        pub fn nur_send(ptr: *const u8, len: usize);
        pub fn nur_end();
    }
}

fn nur_log(msg: &str) {
    unsafe {
        import::nur_log(msg.as_ptr(), msg.len());
    }
}

fn nur_send(msg: &str) {
    unsafe {
        import::nur_send(msg.as_ptr(), msg.len());
    }
}

// end doesn't really do something special, it just sends the signal to the host to end
// its lifecycle
fn nur_end() {
    unsafe {
        import::nur_end();
    }
}

// Global buffer to accumulate HTTP request data across multiple poll_stream calls
// Safe to use static mut in WASM since it's single-threaded
static mut REQUEST_BUFFER: Vec<u8> = Vec::new();

// String accessible within the wasm linear memory
static RESPONSE: &str = "HTTP/1.1 200 OK\r\n\
Content-Type: application/json\r\n\
Content-Length: 29\r\n\
\r\n\
{\"msg\": \"Hello world, wasm!\"}";

// This will be called by wasmer
#[unsafe(no_mangle)]
pub extern "C" fn poll_stream(data: usize, len: usize) {
    let data: *const u8 = data as *const u8;
    let slice = unsafe { std::slice::from_raw_parts(data, len) };

    unsafe {
        let buffer = &mut *&raw mut REQUEST_BUFFER;
        buffer.extend_from_slice(slice);
    }

    let mut headers = [httparse::EMPTY_HEADER; 64];
    let mut req = httparse::Request::new(&mut headers);

    match req.parse(slice) {
        Ok(parsed) => {
            if parsed.is_partial() {
                nur_log("🌎 Accumulating request data of len={len}");
                return;
            }
        }
        Err(e) => {
            nur_log(&format!("Got invalid request, digo waa: {e}"));
            nur_log(String::from_utf8_lossy(slice).to_string().as_str());
            nur_end();
            return;
        }
    }

    nur_log("Look, there is a request!\n");
    nur_log("👋 Let's send them a hello\n");

    nur_send(RESPONSE);
    nur_end();
}

#[unsafe(no_mangle)]
pub extern "C" fn alloc(len: usize) -> usize {
    let layout = std::alloc::Layout::array::<u8>(len).unwrap();
    unsafe { std::alloc::alloc(layout) as usize }
}
