# voidctl

**Lightweight Windows manager for Corsair VOID headsets — no iCUE required — with a unified control panel for Huawei FreeBuds (via [OpenFreebuds](https://github.com/melianmiko/OpenFreebuds)).**

A tiny, native Rust replacement for the bloated vendor software: check the battery, set sidetone, toggle the LED, play notification beeps, configure auto-shutdown — from the CLI, a compact dark-themed tray app, or a local HTTP API. The GUI also embeds a **Huawei FreeBuds section** that talks to a running OpenFreebuds instance over its local RPC, so both headsets live in one window.

> UI and code comments are in Turkish. The app was built by reverse-engineering the Void USB HID protocol with Wireshark/USBPcap captures (see `PLAN.md` for the roadmap and research notes).

## Features

### Corsair VOID (USB dongle)
- Battery level with 5-sample median (the dongle's battery byte jitters ±10) + charging/mic status
- Sidetone 0–128, LED on/off, notification beeps
- Auto-shutdown (inactive timer) — written to headset memory, works without voidctl running
- Firmware version readout (dongle + headset)
- Battery history log + estimated runtime (`%LOCALAPPDATA%\voidctl\history.jsonl`)
- Per-app automatic sidetone via `profiles.ini` (e.g. Discord → 100, Chrome → 80)
- Tray app with live battery icon, low-battery beep/notifications, LED auto-off below 30%

### Huawei FreeBuds (via OpenFreebuds)
- Status line: device name, connection state, battery (L/R/case, charging indicator)
- ANC mode switching, EQ preset selection, low-latency toggle, connect/disconnect
- Buttons adapt to what the device actually reports (capability discovery through the property store)
- Degrades gracefully when OpenFreebuds isn't running (one-click launch / install hint)

### Local HTTP API (`voidctl serve`)
- `GET /health`, `GET /state` (cached battery snapshot — instant)
- `POST /sidetone {"value":0-128}`, `POST /led {"on":bool}`, `POST /beep {"id":0|1}`, `POST /otokapat {"on":bool}`
- Binds to `127.0.0.1` only; HID access serialized behind a mutex

## CLI

```
voidctl battery            battery + mic status (5-sample median)
voidctl sidetone <0-128>   sidetone level (0 = off)
voidctl led on|off         LED
voidctl beep [0|1]         notification sound
voidctl otokapat on|off    inactive auto-shutdown (writes headset memory)
voidctl watch [secs]       battery watch (default 60s; beep below 20%)
voidctl fw                 dongle + headset firmware versions
voidctl fb                 Huawei FreeBuds status (via OpenFreebuds RPC)
voidctl info               list Corsair HID interfaces
voidctl serve [port]       local HTTP API (127.0.0.1, default 59312)
```

## Build

Windows, Rust with the `x86_64-pc-windows-gnu` target:

```
rustup toolchain install stable-x86_64-pc-windows-gnu
cargo +stable-x86_64-pc-windows-gnu build --release
```

Binaries: `target/release/voidctl.exe` (CLI) and `target/release/voidgui.exe` (tray + unified panel).

## FreeBuds integration notes

voidctl talks to OpenFreebuds' built-in RPC server (`http://127.0.0.1:19823`) — a separate process, plain HTTP, no shared code. Only an allowlist of read/write endpoints is used (`get_state`, `get_property`, `set_property`, `get_device_tags` + known shortcuts). If you enable OpenFreebuds' authorization, put the secret in `%LOCALAPPDATA%\voidctl\panel.json` as `openfreebuds_secret`.

## Credits & disclaimer

- Void HID protocol knowledge from [Sapd/HeadsetControl](https://github.com/Sapd/HeadsetControl), [Ravenslofty/corsair-re](https://github.com/Ravenslofty/corsair-re) and the Linux `hid-corsair-void` driver.
- FreeBuds control by [melianmiko/OpenFreebuds](https://github.com/melianmiko/OpenFreebuds) (GPL-3.0, used over HTTP — this project contains none of its code).
- Unofficial community software. Not affiliated with CORSAIR or HUAWEI. Firmware updates are intentionally out of scope.

## License

MIT — see [LICENSE](LICENSE).
