# Change Log

## 0.3.4

- Add `Adapter::discover_devices`
- Add reconnect example

## 0.3.3

- Reduce dependencies

## 0.3.2

- Add CI
- Add scraped examples to docs
- Fix a doctest error

## 0.3.1

- Change `AttError` from an enum to a newtype wrapper around a `u8` with defined
  constants for the known error codes

## 0.3.0

- Add Linux implementation
- Add connected example
- Add `Adapter::connected_devices`
- Add `Adapter::connected_devices_with_services`
- Add re-export of the `Uuid` struct
- Change `Characteristic::properties` to return a `CharacteristicProperties`
  struct
- Change `SmallVec` to `Vec` in all APIs
- Split discovery APIs into separate `discover_x()` and `discover_x_with_uuid()`
  methods.
- Remove `smallvec` and `uuid` re-exports

## 0.2.0

Initial release
