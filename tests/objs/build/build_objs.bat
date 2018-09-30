@echo off

rustup run nightly-x86_64-pc-windows-msvc rustc add.rs --crate-type lib -O --emit obj -Z force-overflow-checks=off --out-dir ..\x64
rustup run nightly-x86_64-pc-windows-msvc rustc main.rs --crate-type lib -O --emit obj -Z force-overflow-checks=off --out-dir ..\x64

rustup run nightly-i686-pc-windows-msvc rustc add.rs --crate-type lib -O --emit obj -Z force-overflow-checks=off --out-dir ..\x86
rustup run nightly-i686-pc-windows-msvc rustc main.rs --crate-type lib -O --emit obj -Z force-overflow-checks=off --out-dir ..\x86
