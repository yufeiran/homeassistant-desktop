# Home Assistant Desktop

A small tray client for [Home Assistant](https://www.home-assistant.io/) that uses each operating system's maintained WebView instead of bundling a frozen copy of Chromium.

![Home Assistant Desktop](https://raw.githubusercontent.com/iprodanovbg/homeassistant-desktop/master/media/screenshot.png)

This branch is a Tauri 2 migration of Ivan Prodanov's Electron client, which was itself forked from [mrvnklm/homeassistant-desktop](https://github.com/mrvnklm/homeassistant-desktop).

## System WebViews

| Platform | WebView | Runtime handling |
| --- | --- | --- |
| Windows | Microsoft Edge WebView2 | The NSIS installer checks for the Evergreen Runtime and offers Microsoft's bootstrapper if it is missing. |
| macOS | WKWebView | Included in macOS and updated with the operating system. |
| Linux | WebKitGTK 4.1 | Installed and updated by the distribution package manager; DEB packages declare the runtime dependency. |

The setup screen displays the detected runtime. The Home Assistant server is not modified by this application.

## Release architectures

Each GitHub Release provides native 64-bit packages for the following systems:

| Platform | Architectures | Package |
| --- | --- | --- |
| Windows | x64, ARM64 | NSIS installer (`.exe`) |
| macOS | Intel x64, Apple Silicon ARM64 | Disk image (`.dmg`) |
| Linux | x64, ARM64 | Debian package (`.deb`) and AppImage |

The packages are built on native GitHub-hosted runners for their target architecture. Legacy 32-bit x86 packages are not produced.

## Features

- Hover or click the tray icon to show Home Assistant
- Multiple instances, mDNS discovery, health checks, and automatic switching
- Detached window, always-on-top, full-screen, and saved window layout
- Optional start-at-login and global show/hide shortcut (`Cmd/Ctrl + Alt + X`)
- Full-screen shortcut (`Cmd/Ctrl + Alt + Enter`)
- Existing Electron settings are read from the same `homeassistant-desktop/config.json` path

Remote Home Assistant pages do not receive Tauri IPC permission. New-window links open in the default browser.

## Install

Download the installer for your platform from the GitHub Actions artifacts or build it locally.

On Windows, run the NSIS installer. It installs per user and checks for the Microsoft Edge WebView2 Evergreen Runtime. If WebView2 is missing, the installer offers Microsoft's official bootstrapper. The locally built packages are not code-signed, so Windows may display an unknown-publisher warning.

Existing 1.x settings are preserved. Version 1.8.0 reads the same configuration file, migrates the start-at-login command to the new executable, and removes the obsolete Electron installation during the normal upgrade.

The client keeps Home Assistant's background connection timers active on Windows. Its independent native monitor tolerates short network interruptions and automatically reconnects after a confirmed outage, so the unavailable page does not require a manual reconnect when the server returns.

## System WebView migration

Version 1.8.0 replaces Electron 21 and its bundled Chromium 106 engine with Tauri 2 and the maintained system WebView:

- Windows now uses the installed Microsoft Edge WebView2 Evergreen Runtime.
- macOS now uses the operating system's WKWebView.
- Linux now uses WebKitGTK 4.1 and declares the required Debian package dependencies.
- The tray workflow, multiple Home Assistant instances, mDNS discovery, health checks, automatic switching, window modes, shortcuts, and start-at-login behavior remain available.
- Remote Home Assistant content is isolated from Tauri IPC capabilities.
- GitHub Actions now tests and builds Windows, macOS, and Linux packages.
- The legacy Electron updater is disabled until signed Tauri update artifacts are configured.

See [CHANGELOG.md](CHANGELOG.md) for the complete release notes.

## Build

Prerequisites are Node.js 20+, Rust, and the [Tauri platform prerequisites](https://v2.tauri.app/start/prerequisites/).

```sh
npm install
npm test
cargo test --manifest-path src-tauri/Cargo.toml
npm run build
```

On Debian/Ubuntu, install the build dependencies first:

```sh
sudo apt-get install libwebkit2gtk-4.1-dev libayatana-appindicator3-dev librsvg2-dev
```

The generated installers are under `src-tauri/target/release/bundle/`. Automatic updating is intentionally disabled until the fork owner configures and signs a Tauri update feed; the archived Electron update server is not reused.

## License and authors

- Copyright 2022 Ivan Prodanov
- Copyright 2020-2021 Marvin Kelm

Licensed under the Apache License, Version 2.0. See [LICENSE.md](LICENSE.md).
