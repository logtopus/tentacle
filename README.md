[![CircleCI](https://circleci.com/gh/logtopus/tentacle/tree/master.svg?style=svg)](https://circleci.com/gh/logtopus/tentacle/tree/master)

# tentacle
The daemon which provides data to the logtopus server

## Building from source

1. Install the Rust toolchain (stable) using `rustup` see [Installation](https://doc.rust-lang.org/book/second-edition/ch01-01-installation.html)
    * build has been tested with at least Rust 1.39

2. Run `cargo build` or `cargo build --release`

3. Optionally build an RPM or DEB package
    * for RPM, install cargo-rpm: `cargo install cargo-rpm` and run `cargo rpm init`, followed by `cargo rpm build`
    * for DEB, install cargo-deb: `cargo install cargo-deb` and `...to be continued`
