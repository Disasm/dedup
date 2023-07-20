# `dedup`
> A command line tool for removing duplicate files.

## Installation
`dedup` is written in Rust and can be built with `cargo`:

```sh
cargo build --release
```

## Usage
```
dedup [OPTIONS] <REFERENCE> <TARGET>
Arguments:
  <REFERENCE>  Path to a reference directory
  <TARGET>     Path to a target directory to be deduplicated

Options:
  -n, --dry-run  Perform a trial run with no changes made
  -h, --help     Print help
```
