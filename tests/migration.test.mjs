import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import test from 'node:test';

const read = path => readFile(new URL(`../${path}`, import.meta.url), 'utf8');

test('the desktop client no longer ships Electron', async () => {
  const pkg = JSON.parse(await read('package.json'));
  assert.equal(pkg.devDependencies.electron, undefined);
  assert.equal(pkg.dependencies?.electron, undefined);
  assert.match(pkg.scripts.build, /tauri build/);
});

test('local pages use Tauri commands rather than Node integration', async () => {
  const pages = `${await read('web/index.html')}\n${await read('web/error.html')}`;
  assert.doesNotMatch(pages, /require\(['"]electron['"]\)/);
  assert.match(pages, /__TAURI__\.core\.invoke/);
});

test('bundles declare each operating system WebView policy', async () => {
  const config = JSON.parse(await read('src-tauri/tauri.conf.json'));
  assert.equal(config.bundle.windows.webviewInstallMode.type, 'downloadBootstrapper');
  assert.deepEqual(config.bundle.linux.deb.depends, [
    'libwebkit2gtk-4.1-0',
    'libayatana-appindicator3-1',
  ]);
  assert.equal(config.bundle.macOS.minimumSystemVersion, '11.0');
});

test('the native monitor keeps Home Assistant sessions alive and reconnects automatically', async () => {
  const source = await read('src-tauri/src/lib.rs');
  const errorPage = await read('web/error.html');

  assert.match(source, /disable-background-timer-throttling/);
  assert.match(source, /FAILURES_BEFORE_ERROR_PAGE: u8 = 6/);
  assert.match(source, /instance_available\(&client, &current\)/);
  assert.match(source, /url\.path\(\)\.ends_with\("error\.html"\)/);
  assert.match(errorPage, /Automatic reconnect is active/);
});
