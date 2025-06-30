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
    let body_text = if body.is_empty() { "..." } else { body };
    let chars_len = body_text.chars().count();

    let top_border = "_".repeat(chars_len + 2);
    let bottom_border = "-".repeat(chars_len + 2);

    let cowsay_art = format!(
        " {}
< {} >
 {}
        \\   ^__^
         \\  (👀 )\\_______
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
    if data == 0 {
        nur_log("Request end forced, mu... 🐄🥛");
        nur_end();
    }

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

    let buffer: &Vec<u8>;
    unsafe {
        buffer = &*&raw const REQUEST_BUFFER;
    }

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

            if req.method == Some("GET") {
                // Handle GET requests
                let response = create_cowsay_response("muu! intenta con POST!");
                nur_send(&response);
                nur_end();
                return;
            }

            // Request headers are sent completed, but we have no guarantees over the body,
            // so let's manuallly chech for the content length
            let body_start = parsed.unwrap();

            let content_length = req
                .headers
                .iter()
                .find(|h| h.name.to_lowercase() == "content-length");

            if content_length.is_none() {
                let response = create_cowsay_response("muu! no content-length?");
                nur_send(&response);
                nur_end();
                return;
            }

            let content_length = match str::from_utf8(content_length.unwrap().value) {
                Ok(length) => match length.trim().parse::<usize>() {
                    Ok(len) => len,
                    Err(_) => {
                        let response = create_cowsay_response("muu! invalid content-length?");
                        nur_send(&response);
                        nur_end();
                        return;
                    }
                },
                Err(_) => {
                    let response = create_cowsay_response("muu! invalid content-length?");
                    nur_send(&response);
                    nur_end();
                    return;
                }
            };

            nur_log(&format!(
                "🐄 Received complete request with total len: {}!",
                buffer.len()
            ));

            let mut body = &buffer[body_start..];
            if body.len() > content_length {
                nur_log(&format!(
                    "🐄 Body is too long, content-length={}, body={}. Trimming to content-length...",
                    content_length,
                    body.len()
                ));
                body = &buffer[body_start..body_start + content_length];
            }

            if body.len() != content_length {
                nur_log(&format!(
                    "🐄 Body is not there yet, content-length={}, body={}. Waiting for more data...",
                    content_length,
                    body.len()
                ));
                return;
            }

            // We now have a complete and valid body!

            let body_str = String::from_utf8_lossy(&buffer[body_start..])
                .trim()
                .to_string();

            nur_log(&format!("🗨️🐄 cow message: {body_str}\n"));

            let response = create_cowsay_response(&body_str);
            nur_send(&response);
            nur_log("200 OK\n");
            nur_end();
        }
        Err(e) => {
            nur_log(&format!("Got invalid request, digo muu 🐄: {e}"));
            nur_log(String::from_utf8_lossy(buffer).to_string().as_str());
            nur_end();
        }
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn alloc(len: usize) -> usize {
    let layout = std::alloc::Layout::array::<u8>(len).unwrap();
    unsafe { std::alloc::alloc(layout) as usize }
}
