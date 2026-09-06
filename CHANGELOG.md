# Changelog

All notable changes to this fork are documented in this file.

## Unreleased

### Fixed

- Restored native macOS window controls in detached mode and fixed Retina window
  size and position persistence.
- Switched the macOS menu bar icon to the transparent monochrome template asset.
- Made `Control+Alt+X` the macOS show/hide shortcut, retained
  `Command+Alt+X` compatibility, and fixed normalized shortcut matching.

## 1.8.1 - 2026-09-06

### Fixed

- Kept Home Assistant's WebSocket timers active while the Windows tray window is hidden.
- Reused one native keep-alive HTTP client for availability checks instead of opening a new connection for every probe.
- Ignored brief network errors and waited for six consecutive failed checks before showing the unavailable page.
- Kept availability monitoring active on the error page and returned to Home Assistant automatically when the instance recovered.

## 1.8.0 - 2026-08-31

### Changed

- Replaced Electron 21 and bundled Chromium 106 with Tauri 2.
- Switched Windows rendering to the Microsoft Edge WebView2 Evergreen Runtime.
- Switched macOS rendering to WKWebView and Linux rendering to WebKitGTK 4.1.
- Preserved the existing `homeassistant-desktop/config.json` settings format and location.
- Preserved tray controls, multiple instances, mDNS discovery, health checks, automatic instance switching, detached mode, always-on-top mode, full-screen mode, global shortcuts, and start-at-login support.
- Migrated the Windows start-at-login command from the Electron executable to the Tauri executable.
- Added a WebView runtime status display to the setup screen and tray menu.
- Replaced the archived Electron auto-updater with an explicit disabled state until a signed Tauri update feed is configured.

### Security

- Prevented remote Home Assistant pages from receiving Tauri IPC capabilities.
- Opened new-window requests in the user's default browser instead of creating privileged embedded windows.
- Rejected Home Assistant URLs containing embedded usernames or passwords.

### Packaging and testing

- Added a per-user Windows NSIS installer that downloads Microsoft's WebView2 bootstrapper only when the runtime is missing.
- Added native Windows x64 and Windows ARM64 NSIS installers.
- Added separate macOS Intel x64 and Apple Silicon ARM64 disk images.
- Added native Linux x64 and Linux ARM64 Debian packages and AppImages.
- Added Debian runtime dependencies for WebKitGTK 4.1 and Ayatana AppIndicator.
- Added GitHub Actions builds and automatic tagged Release publishing for all six supported operating system and architecture combinations.
- Added a manual workflow trigger for reproducible package builds.
- Declared the platform icon set explicitly so AppImage packaging can select a square PNG on both x64 and ARM64 Linux runners.
- Restricted downloadable artifacts and Release assets to finished installer and package files.
- Documented the supported release architectures and the intentional exclusion of legacy 32-bit x86 packages.
- Added Node.js migration tests and Rust unit tests.

### Upgrade notes

- Existing Home Assistant instances and window preferences are retained automatically.
- The Windows package has been tested with WebView2 `151.0.4129.107` against Home Assistant.
- Locally generated installers are unsigned. Configure platform code signing before publishing a production release.
- macOS and Linux packages are covered by CI configuration but still require installation testing on their native operating systems.
