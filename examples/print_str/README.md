# print_str

| Features   |     |
| ---------- | --- |
| imports    | no  |
| parameters | no  |
| no_std     | no  |
| wasi       | no  |
| C bindings | no  |

## Compiling

Add the rustup target

```sh
rustup target add wasm32-unknown-unknown

cargo build --target wasm32-unknown-unknown --release
```

Inspect with

```sh
wasm2wat target/wasm32-unknown-unknown/release/hello_wasm.wasm
```

## Acknowledgements

Rust to WebAsembly the hard way: <https://surma.dev/things/rust-to-webassembly/>
