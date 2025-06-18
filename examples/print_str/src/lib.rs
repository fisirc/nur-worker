// Define a function that is imported into the module.
// By default, the "env" namespace is used.
unsafe extern "C" {
    fn print_str(ptr: *const u8, len: usize);
}

// String accessible within the wasm linear memory
static HELLO: &'static str = "Hello, World!";

// This will be called by wasmer
#[unsafe(no_mangle)]
pub fn hello_wasm() {
    unsafe {
      print_str(HELLO.as_ptr(), HELLO.len());
    }
}
