# Changelog

## [2.1.0] - 2020-01-24

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

[2.0.0]: https://github.com/vilgotf/voice-pruner/releases/tag/2.0.0 
[1.1.1]: https://github.com/vilgotf/voice-pruner/releases/tag/1.1.1
[1.1.0]: https://github.com/vilgotf/voice-pruner/releases/tag/1.1.0
[1.0.0]: https://github.com/vilgotf/voice-pruner/releases/tag/1.0.0
