# Installation

Schema Bridge is a [Dioxus](https://dioxuslabs.com) 0.7 application written
in Rust. It is built to WebAssembly and runs in your browser at
`http://localhost:8080`; a small local development server provided by the
Dioxus tool serves it. Nothing is installed system-wide apart from the Rust
toolchain and the Dioxus tool, and nothing connects to the internet once the
dependencies are downloaded.

> The steps were run on macOS. The Windows and Linux steps follow the
> official Rust and Dioxus instructions; if anything differs on your
> machine, please report it (see [troubleshooting](troubleshooting.md)).

## What you need

| Tool | Version | Why |
| --- | --- | --- |
| Rust (via rustup) | 1.85 or newer | Compiles Schema Bridge |
| WebAssembly target `wasm32-unknown-unknown` | — | The browser build |
| Dioxus CLI (`dx`) | 0.7.x (tested with 0.7.10) | Builds and serves the app |
| A current browser | Chrome, Edge, Firefox or Safari | Runs the app |
| Git | any | Gets the source |

About 3 GB of disk space is used by the Rust toolchain and the compiled
dependencies. The first build takes a few minutes; later builds take seconds.

## macOS

```sh
# 1. Rust (skip if `rustc --version` already prints 1.85 or newer)
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source "$HOME/.cargo/env"
rustup update stable

# 2. The WebAssembly target
rustup target add wasm32-unknown-unknown

# 3. The Dioxus CLI, as a ready-made binary
brew install cargo-binstall          # or: cargo install cargo-binstall
cargo binstall dioxus-cli@0.7.10 --force
```

If you do not use Homebrew, `cargo install dioxus-cli --version 0.7.10 --locked`
compiles the tool instead (about 10 minutes). Xcode's command line tools are
needed for Rust; macOS offers to install them the first time, or run
`xcode-select --install`.

## Windows

1. Install the **Visual Studio Build Tools** with the workload
   "Desktop development with C++" (Rust needs its linker):
   <https://visualstudio.microsoft.com/visual-cpp-build-tools/>.
2. Install Rust with **rustup-init.exe** from <https://rustup.rs> and accept
   the defaults.
3. Open a new **PowerShell** window and run:

```powershell
rustup update stable
rustup target add wasm32-unknown-unknown
cargo install cargo-binstall
cargo binstall dioxus-cli@0.7.10 --force
```

Use the same commands as on macOS for everything below. If PowerShell
reports that `dx` is unknown, close and reopen it so `%USERPROFILE%\.cargo\bin`
is on the `PATH`.

## Linux

Install a C toolchain and a few libraries first (names for Debian/Ubuntu;
Fedora uses `gcc pkg-config openssl-devel`):

```sh
sudo apt update
sudo apt install build-essential pkg-config libssl-dev curl git
```

Then:

```sh
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source "$HOME/.cargo/env"
rustup target add wasm32-unknown-unknown
cargo install cargo-binstall
cargo binstall dioxus-cli@0.7.10 --force
```

## Check the tools

```sh
rustc --version                    # 1.85 or newer
rustup target list --installed     # includes wasm32-unknown-unknown
dx --version                       # starts with 0.7
```

## Get the source and run the application

```sh
git clone https://github.com/IqraRaja/schema-bridge.git
cd schema-bridge
dx serve --package schema-bridge-app
```

When the terminal shows "Serving your app", open **http://localhost:8080**.
Changes to the code reload automatically. Stop the server with `Ctrl+C`.

Useful options:

- `dx serve --package schema-bridge-app --open false`: do not open a browser tab.
- `dx serve --package schema-bridge-app --port 8081`: use another port when 8080 is taken.

In VS Code, open the `schema-bridge` folder and use **Terminal → New
Terminal**; the [rust-analyzer](https://marketplace.visualstudio.com/items?itemName=rust-lang.rust-analyzer)
extension is recommended.

## Build a release version

```sh
dx build --package schema-bridge-app --release
```

The finished site is written below `target/dx/schema-bridge-app/release/web/public`.
It is a static folder (HTML, JavaScript, WebAssembly, CSS); any static web
server on your own machine can serve it, for example
`python3 -m http.server --directory target/dx/schema-bridge-app/release/web/public 8080`.
Do not put a copy on a public server if the people using it would load
confidential schemas there: although the files are processed in the
browser, keep it on machines you trust.

## Run the tests

```sh
cargo test                                                          # library tests
cargo check -p schema-bridge-app --target wasm32-unknown-unknown    # the app compiles
```

More in [CONTRIBUTING.md](../CONTRIBUTING.md).

## Update

```sh
git pull
dx serve --package schema-bridge-app
```

Saved comparison files keep working across versions of the same format;
a file from a newer version is refused with a clear message.

## Uninstall

Delete the `schema-bridge` folder. To remove the tools as well:
`cargo uninstall dioxus-cli` and `rustup self uninstall`.
