# Bluest — Cross-platform Bluetooth LE crate for Rust

[![crates.io][crates-badge]][crates-url] [![docs.rs][docs-badge]][docs-url]
[![Build Status][actions-badge]][actions-url]

[crates-badge]: https://img.shields.io/crates/v/bluest
[crates-url]: https://crates.io/crates/bluest
[docs-badge]: https://docs.rs/bluest/badge.svg
[docs-url]: https://docs.rs/bluest
[actions-badge]: https://github.com/alexmoon/bluest/workflows/CI/badge.svg
[actions-url]: https://github.com/alexmoon/bluest/actions?query=workflow%3ACI+branch%3Amain

**Bluest** is a cross-platform [Bluetooth] Low Energy (BLE) crate for [Rust]. It
supports the GAP Central and GATT Client roles, allowing you to access BLE
peripheral devices and the GATT services they provide.

The primary functions provided by **Bluest** are:

- Device discovery:
  - Scanning for devices and receiving advertisements
  - Finding connected devices
  - Re-opening previously found devices
  - Connecting to discovered devices
- Accessing remote GATT services:
  - Discovering devices by the services they advertise
  - Discovering device services
  - Discovering service characteristics
  - Discovering characteristic descriptors
  - Read, write (including write with response), and notify/indicate operations
    on remote characteristics
  - Read and write operations on characteristic descriptors

[Rust]: https://www.rust-lang.org/
[Bluetooth]: https://www.bluetooth.com/specifications/specs/

## Platform support

- Windows
- MacOS
- iOS
- Linux

Android support is planned.

## Features

The `serde` feature is available to enable serializing/deserializing device
identifiers.

## Examples

Examples demonstrating basic usage are available in the [examples folder]. Refer
to the [API documentation] for more details.

[API documentation]: https://docs.rs/bluest
[examples folder]: https://github.com/alexmoon/bluest/tree/master/bluest/examples
