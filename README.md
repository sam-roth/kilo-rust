# kilo-rust
A text editor in [Rust](https://www.rust-lang.org/) based on [Antirez's Kilo editor](https://github.com/antirez/kilo).

## Usage
`kilo_rust FILENAME`

## Keys

Key|Effect
-----|----
`C-s`|Save
`C-q`|Quit
`C-f`|Find string in file (navigate with arrow keys, `Esc` to cancel, `Enter` to accept)

## Missing Features

* Syntax highlighting isn't implemented yet

## Building

Requires Rust nightly. Build using `cargo build`.
