// Define a function that is imported into the module.
// By default, the "env" namespace is used.

#[link(wasm_import_module = "nur")]
unsafe extern "C" {
    fn nur_log(ptr: *const u8, len: usize);
    fn nur_send(ptr: *const u8, len: usize);
    // TODO: random
    fn nur_end();
}

fn log(msg: &str) {
    unsafe {
        nur_log(msg.as_ptr(), msg.len());
    }
}

fn send(msg: &str) {
    unsafe {
        nur_send(msg.as_ptr(), msg.len());
    }
}

// end doesn't really do something special, it just sends the signal to the host to end
// its lifecycle
fn end() {
    unsafe {
        nur_end();
    }
}

// String accessible within the wasm linear memory
static RESPONSE: &str = "HTTP/1.1 200 OK\r\n\
Content-Type: application/json\r\n\
Content-Length: 27\r\n\
\r\n\
{\"msg\": \"Hello world, wa!\"}";

// This will be called by wasmer
#[unsafe(no_mangle)]
pub extern "C" fn poll_stream(data: usize, len: usize) {
    let data: *const u8 = data as *const u8;
    let mut headers = [httparse::EMPTY_HEADER; 64];
    let mut req = httparse::Request::new(&mut headers);

    let slice = unsafe { std::slice::from_raw_parts(data, len) };

    match req.parse(slice) {
        Ok(parsed) => {
            if parsed.is_partial() {
                return;
            }
        }
        Err(e) => {
            log(&format!("Got invalid request, digo waa: {e}"));
            log(String::from_utf8_lossy(slice).to_string().as_str());
            end();
            return;
        }
    }

    log("Look, there is a request!\n");
    log("Let's send them some love\n");

    send(RESPONSE);
    end();
}

#[unsafe(no_mangle)]
pub extern "C" fn alloc(len: usize) -> usize {
    let layout = std::alloc::Layout::array::<u8>(len).unwrap();
    unsafe { std::alloc::alloc(layout) as usize }
}
