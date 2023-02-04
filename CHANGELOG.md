# Changelog

## [2.4.0] - 2023-02-04

### Additions

* `/prune` has a new optional role argument
* Expose TLS backends through features, now also defaulting to native-tls
* On shutdown, process remaining events after sending the close frame

### Internal changes

* Bump clap -> 4.0
* Bump twilight 0.12 -> 0.15.0-rc.1
* Instrument interactions with only their ID
* Lower log level by 1 for most instances

## [2.3.0] - 2022-07-28

### Additions

* `/is_monitored` and others use "whether" instead of "returns `true` if" to describe boolean operations
* `/is_monitored` and `/list` no longer have `default_member_permissions` set

### Internal changes

* Bump twilight 0.11 -> 0.12
* Don't abort on panic in release mode
* Improve token retrieval logging
* Inline the `event` module into `main` and coalesce `Search`, and `auto_prune` into a new `prune` module
* Relicense from the Unlicense to 0BSD
* Use static `BOT` variable instead of passing around a static `Bot` pointer

## [2.2.1] - 2022-06-30

### Fixes

* `/list` now correctly filters non voice channels

### Internal changes

* Use single threaded tokio runtime

## [2.2.0] - 2022-06-09

### Additions

* Commands are ephemeral
* Command caller permission is configured at guild level, by default requiring `MOVE_MEMBERS`
* `/list` channel names are clickable to connect

### Internal changes

* Commands are registered as guild only
* Command logic is greatly simplified by `expect()` impossible states
* Improved documentation

## [2.1.1] - 2022-06-04

### Internal changes

* Bump twilight 0.9 -> 0.11
* `channel` and `role` event skipping improvements
* Faster startup via concurrency
* Gateway now managed by a single shard instead of a cluster
* Improved logging
* MSRV is now 1.60
* Reworked command registration, per guild is no longer supported
* Simplified systemd credential loader

## [2.1.0] - 2022-01-24

### Breaking changes

`/monitored` and `/unmonitored` are replaced by `/list` and `is-monitored` for greater usability.

## [2.0.0] - 2022-01-23

### Breaking changes

`/list` is replaced by `monitored` and `unmonitored`

### Internal changes

* Bump clap -> 3.0
* Bump twilight 0.8 -> 0.9
* MSRV is now 1.57
* No longer depends on `async-trait`

## [1.1.1] - 2021-12-05

### Internal changes

* Use 2021 edition
* Use the native TLS certificate store
* Bump tracing-subscriber 0.2 -> 0.3
* Bump twilight 0.6 -> 0.8

## [1.1.0] - 2021-10-09

### Additions

* `/prune` Return the number of pruned users
* Only allow voice channels to be selected

## [1.0.0] - 2021-08-29

Initial release.

[2.4.0]: https://github.com/vilgotf/voice-pruner/releases/tag/2.4.0
[2.3.0]: https://github.com/vilgotf/voice-pruner/releases/tag/2.3.0
[2.2.1]: https://github.com/vilgotf/voice-pruner/releases/tag/2.2.1
[2.2.0]: https://github.com/vilgotf/voice-pruner/releases/tag/2.2.0
[2.1.1]: https://github.com/vilgotf/voice-pruner/releases/tag/2.1.1
[2.1.0]: https://github.com/vilgotf/voice-pruner/releases/tag/2.1.0
[2.0.0]: https://github.com/vilgotf/voice-pruner/releases/tag/2.0.0
[1.1.1]: https://github.com/vilgotf/voice-pruner/releases/tag/1.1.1
[1.1.0]: https://github.com/vilgotf/voice-pruner/releases/tag/1.1.0
[1.0.0]: https://github.com/vilgotf/voice-pruner/releases/tag/1.0.0
