# ionguard

> **NEMESIS ENGINE LAN Monitor** — Real-time passive & active network device detection

A Wails-based desktop application that monitors your local network for device arrivals, departures, and MAC address changes. The Rust core handles raw socket monitoring while the Go/Wails frontend provides a sleek dark-themed interface.

## Features

- **Passive Monitoring** — Reads `ip neigh` and `/proc/net/arp` for real-time neighbor table changes
- **Active Sweep** — Optional `fping` sweep to discover silent hosts
- **Event Log** — Tracks device joins, leaves, and MAC changes with timestamps
- **Ignore List** — Filter out known/gateway IPs to reduce noise
- **Dark UI** — NEMESIS ENGINE branded interface

## Architecture

| Layer | Technology | Role |
|-------|-----------|------|
| Frontend | Vanilla JS + Vite | Dark-themed dashboard |
| Backend | Go + Wails v2 | GUI runtime, process management |
| Core | Rust + Tokio | ARP/neighbor monitoring, JSON IPC |

IPC between Go and Rust uses newline-delimited JSON over stdin/stdout.

## Building

### Prerequisites

- Go 1.21+
- Node.js 18+
- Rust 1.75+
- Wails CLI v2

### Build

```bash
# Build Rust core
cd core/ionguard-core
cargo build --release
cd ../..

# Build Wails app
wails build
```

The binary `ionguard` and its companion `ionguard-core` will be in `build/bin/`.

## Usage

```bash
ionguard
```

Click **Start** to begin monitoring. Use **Sweep** to actively scan the local subnet.

## License

MIT — iON Data Management System / VanHoney-ltd
