// Define a function that is imported into the module.
// By default, the "env" namespace is used.

// Global buffer to accumulate HTTP request data across multiple poll_stream calls
// Safe to use static mut in WASM since it's single-threaded
static mut REQUEST_BUFFER: Vec<u8> = Vec::new();

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

fn create_cowsay_response(body: &str) -> String {
    let body_text = if body.is_empty() { "<empty>" } else { body };
    let chars_len = body_text.chars().count();

    let top_border = "_".repeat(chars_len);
    let bottom_border = "-".repeat(chars_len);

    let cowsay_art = format!(
        " {}
< {} >
 {}
        \\   ^__^
         \\  (oo)\\_______
            (__)\\       )\\/\\
                ||----w |
                ||     ||",
        top_border, body_text, bottom_border
    );

    let cowsay_len = cowsay_art.len();

    format!(
        "HTTP/1.1 200 OK\r
Content-Type: text/plain\r
Content-Length: {cowsay_len}\r
\r
{cowsay_art}"
    )
}

// This will be called by wasmer
#[unsafe(no_mangle)]
pub extern "C" fn poll_stream(data: usize, len: usize) {
    let data: *const u8 = data as *const u8;
    let slice = unsafe { std::slice::from_raw_parts(data, len) };

    // Append new data to the global buffer
    unsafe {
        let buffer = &mut *&raw mut REQUEST_BUFFER;
        buffer.extend_from_slice(slice);
    }

    // Try to parse the accumulated data
    let mut headers = [httparse::EMPTY_HEADER; 64];
    let mut req = httparse::Request::new(&mut headers);

    let parsed_result = unsafe {
        let buffer = &*&raw const REQUEST_BUFFER;

        match req.parse(buffer) {
            Ok(parsed) => {
                if parsed.is_partial() {
                    // Still waiting for more data
                    nur_log(&format!(
                        "🐄 Received partial request chunk with len: {len}, total buffered: {}",
                        buffer.len()
                    ));
                    return;
                }
                parsed
            }
            Err(e) => {
                nur_log(&format!("Got invalid request, digo waa: {e}"));
                nur_log(String::from_utf8_lossy(buffer).to_string().as_str());
                nur_end();
                return;
            }
        }
    };

    unsafe {
        let buffer = &*std::ptr::addr_of!(REQUEST_BUFFER);
        nur_log(&format!(
            "🐄 Received complete request with total len: {}!",
            buffer.len()
        ));

        // Extract the body from the request
        let body_start = parsed_result.unwrap();
        let body = if body_start < buffer.len() {
            String::from_utf8_lossy(&buffer[body_start..])
                .trim()
                .to_string()
        } else {
            String::new()
        };

        nur_log(&format!("🗨️ Received cow message: {}\n", body));

        let response = create_cowsay_response(&body);
        nur_send(&response);

        nur_log("200 OK\n");

        // Clear the buffer for the next request
        let buffer_mut = &mut *std::ptr::addr_of_mut!(REQUEST_BUFFER);
        buffer_mut.clear();
    }

    nur_end();
}

#[unsafe(no_mangle)]
pub extern "C" fn alloc(len: usize) -> usize {
    let layout = std::alloc::Layout::array::<u8>(len).unwrap();
    unsafe { std::alloc::alloc(layout) as usize }
}
