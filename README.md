# LZ👌-rs [![Build Status]][actions] [![Latest Version]][crates.io] [![Api Rustdoc]][rustdoc] ![Rust Version]

[Build Status]: https://github.com/encounter/lzokay-rs/workflows/build/badge.svg
[actions]: https://github.com/encounter/lzokay-rs/actions
[Latest Version]: https://img.shields.io/crates/v/lzokay.svg
[crates.io]: https://crates.io/crates/lzokay
[Api Rustdoc]: https://img.shields.io/badge/api-rustdoc-blue.svg
[rustdoc]: https://docs.rs/lzokay
[Rust Version]: https://img.shields.io/badge/rust-1.81+-blue.svg?maxAge=3600

Pure-Rust port of [LZ👌](https://github.com/jackoalan/lzokay), a minimal, MIT-licensed implementation of the
[LZO compression format](http://www.oberhumer.com/opensource/lzo/).

See the original [README](https://github.com/jackoalan/lzokay/blob/master/README.md) for more information.

### Features

- MIT-licensed
- Simple compression and decompression routines
- `#![no_std]` compatible

### Usage

See the [compress](https://docs.rs/lzokay/latest/lzokay/compress)
or [decompress](https://docs.rs/lzokay/latest/lzokay/decompress)
documentation for reference.

In `Cargo.toml`:

```toml
[dependencies]
lzokay = "2.0.0"
```

Or, to only enable certain features:

```toml
[dependencies.lzokay]
version = "2.0.0"
default-features = false
features = ["decompress", "compress"]
```

- `decompress`: Enables decompression functions.
- `compress`: Enables compression functions.
- `alloc`: Enables optional compression functions that perform heap allocation.
   Without `std`, this uses `extern crate alloc`.
- `std`: Enables use of `std`. Implies `alloc`.

All features are enabled by default.

### License

LZ👌 and LZ👌-rs are available under the MIT License and have no external dependencies.
