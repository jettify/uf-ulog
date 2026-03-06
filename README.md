# uf-ulog

[![CI](https://github.com/jettify/uf-ulog/actions/workflows/CI.yml/badge.svg)](https://github.com/jettify/uf-ulog/actions/workflows/CI.yml)
[![codecov](https://codecov.io/gh/jettify/uf-ulog/graph/badge.svg?token=2N16CN1OZX)](https://codecov.io/gh/jettify/uf-ulog)
[![crates.io](https://img.shields.io/crates/v/uf-ulog)](https://crates.io/crates/uf-ulog)
[![docs.rs](https://img.shields.io/docsrs/uf_ulog)](https://docs.rs/uf-ulog/latest/uf_ulog/)

`uf_ulog` is an allocator-free ULog serializer designed for embedded targets.

## Features

* `no_std`, allocator-free core.
* MCU and transport agnostic (you provide writer/IO).
* Supports both derive-based and manual trait implementations.
* Split into two crates:
  * `uf_ulog` (core serializer)
  * `uf_ulog_macro` (derive macros)

## Supported messages (current scope)

The initial implementation focuses on:
* Data messages (topic payloads) (`D` type)
* String log messages (plain and tagged) (`L` and `C`a types)
* Parameter messages, (`P` type)
* Required format messages needed to describe logged data/strings (`B`, `F`)

## Cargo feature flags

* `derive` (default): enables `#[derive(ULogData)]` and `#[derive(ULogRegistry)]`.
* `async`: enables async exporter support via `embedded-io-async`.

## Installation

Add `uf-ulog` to your `Cargo.toml`:

```toml
[dependencies]
uf-ulog = "*" # replace * by the latest version of the crate.
```

Or use the command line:

```bash
cargo add uf-ulog
```

## Examples

Based on [`uf_ulog/examples/minimal.rs`](uf_ulog/examples/minimal.rs):

```rust
use std::convert::Infallible;

use uf_ulog::LogLevel;
use uf_ulog::ULogCoreExporter;
use uf_ulog::ULogData;
use uf_ulog::ULogProducer;
use uf_ulog::ULogRegistry;

#[derive(ULogData, Debug)]
struct Acc {
    timestamp: u64,
    x: f32,
    y: f32,
    z: f32,
}

#[derive(ULogData, Debug)]
struct Gyro {
    timestamp: u64,
    x: f32,
    y: f32,
    z: f32,
}

#[derive(ULogRegistry)]
pub enum UlogDataMessages {
    Gyro,
    Acc,
}

#[derive(Default)]
struct PrintWriter;

impl embedded_io::ErrorType for PrintWriter {
    type Error = Infallible;
}

impl embedded_io::Write for PrintWriter {
    fn write(&mut self, buf: &[u8]) -> Result<usize, Self::Error> {
        println!("{:?}", buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> Result<(), Self::Error> {
        Ok(())
    }
}

fn main() {
    let timestamp = 1772079727637;
    let producer = ULogProducer::<UlogDataMessages>::new();
    let mut exporter = ULogCoreExporter::<_, UlogDataMessages>::new(PrintWriter)
        .start(timestamp)
        .unwrap();

    let g = Gyro {
        timestamp,
        x: 1.0,
        y: 2.0,
        z: 3.0,
    };
    let a = Acc {
        timestamp,
        x: 1.0,
        y: 2.0,
        z: 9.0,
    };

    let record_data_gyro = producer.data::<Gyro>(&g).unwrap();
    let record_data_acc_instance = producer.data_instance::<Acc>(&a, 1).unwrap();
    let record_log_info = producer.log(LogLevel::Info, 43, "info log");
    let record_log_info_tagged = producer.log_tagged(LogLevel::Info, 1, 43, "info log");
    // Usually records produced by one task in non blocking fashion
    // to make sure that control loops are fast as possible. Records
    // then sent over channel to IO task, that may block or
    // take longer to offload data over compatible bus/net.
    exporter.accept(record_data_gyro).unwrap();
    exporter.accept(record_data_acc_instance).unwrap();
    exporter.accept(record_log_info).unwrap();
    exporter.accept(record_log_info_tagged).unwrap();
}
```

Run it:

```bash
cargo run -p uf_ulog --example minimal --features std
```

### Write ULog to file and parse with `pyulog`

For an end-to-end demo (write a `.ulg` file, then parse it with Python), use:

```bash
just demo
```

This runs the example in [`examples/simple-file`](examples/simple-file) and then executes
[`examples/simple-file/read_ulg.py`](examples/simple-file/read_ulg.py) via `uv`.


## References
* [PX4 ULog File Format Specification](https://docs.px4.io/main/en/dev_log/ulog_file_format) - official binary format reference.
* [PX4/pyulog](https://github.com/PX4/pyulog) - canonical Python tooling for reading and analyzing `.ulg` files.
* [annoybot/yule_log](https://github.com/annoybot/yule_log) - Rust ULog parser implementation.
