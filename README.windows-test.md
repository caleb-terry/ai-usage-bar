# Windows 11 test VM

Spin up a throwaway Windows 11 desktop in a container to install and smoke-test
the Windows build of AI Usage Bar (`.msi` / `.exe`) without owning a Windows
machine.

> **Just need to know if the Windows target compiles?** Don't boot this VM —
> run [`scripts/win-check.sh`](scripts/win-check.sh) instead. It cross-compiles
> the Rust backend for Windows in a lightweight Linux container (no KVM, no
> Windows host), catching `#[cfg(windows)]` errors that CI never checks. This VM
> is for the heavier job: installing and clicking through the built installer.

It runs a **real Windows 11 VM** ([`dockurr/windows`](https://github.com/dockur/windows))
via KVM/QEMU — a genuine GUI desktop, not Windows Server Core. That matters
here: the app is a WebView2-backed GUI that lives in the system tray, so there's
nothing to verify on a headless image.

## Requirements

- **A Linux host with KVM** (`/dev/kvm` present, nested virtualization enabled).
  This compose file mounts `/dev/kvm` for hardware acceleration.
- ~8 GB free RAM and ~70 GB disk for the VM.

> **Heads up — Docker Desktop for macOS / Windows cannot pass through `/dev/kvm`.**
> Your dev machine is macOS, so this won't run hardware-accelerated locally.
> Options:
> - Run it on a **Linux box / cloud VM** (bare metal or a nested-virt-capable
>   instance such as GCP `nested-virtualization` or an EC2 `*.metal`).
> - For a quick local-only check on macOS, drop the `devices:`/`cap_add:` KVM
>   lines and add `environment: KVM: "N"` — it works but runs under slow
>   software emulation (TCG), fine for a one-off install sanity check.

## Usage

```bash
# 1. Build the Windows installer (on a Windows host or in CI) and copy the
#    .msi/.exe into ./win-share/  (see Tauri's src-tauri/target/release/bundle/).

# 2. Boot the VM. First run downloads + auto-installs Windows 11 unattended
#    (~10-20 min). No ISO or product key needed.
docker compose -f docker-compose.windows.yml up -d

# 3. Watch the install / use the desktop in your browser:
open http://localhost:8006        # noVNC web viewer

# 4. Once installed, connect over RDP for a smoother desktop:
#    host: localhost:3389   user: docker   password: (blank)
```

Inside the VM, your `./win-share` folder is mounted as a network drive at
`\\host.lan\Data`. Open it, run the installer, then confirm the tray icon
appears and the usage panel opens.

## Lifecycle

```bash
# Stop, keeping the installed Windows disk (fast restart next time):
docker compose -f docker-compose.windows.yml down

# Nuke everything including the installed Windows disk:
docker compose -f docker-compose.windows.yml down -v
```

The installed OS lives on the `aub-windows-storage` named volume, so `down`
without `-v` lets you re-`up` straight back to the desktop.

## Notes

- Adjust `RAM_SIZE` / `CPU_CORES` / `DISK_SIZE` and the account in
  `docker-compose.windows.yml`.
- `dockurr/windows` downloads Windows from Microsoft's official servers on first
  boot — review its [licensing notes](https://github.com/dockur/windows#fineprint)
  before use.
