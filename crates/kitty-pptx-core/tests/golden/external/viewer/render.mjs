import fs from 'node:fs';
import path from 'node:path';
import { spawn } from 'node:child_process';
import { parseArgs } from 'node:util';
import { fileURLToPath } from 'node:url';
import { chromium } from 'playwright';

const root = path.dirname(fileURLToPath(import.meta.url));
const { values } = parseArgs({
  options: {
    manifest: { type: 'string' },
    fixtures: { type: 'string' },
    out: { type: 'string' },
    width: { type: 'string' },
  },
});

const manifestPath = path.resolve(values.manifest || path.join(root, '..', 'fixtures.json'));
const manifest = JSON.parse(fs.readFileSync(manifestPath, 'utf8'));
const fixturesDir = values.fixtures && path.resolve(values.fixtures);
const outRoot = values.out && path.resolve(values.out);
const width = Number(values.width || manifest.render_width || 1600);

if (!fixturesDir || !outRoot) {
  console.error('usage: node render.mjs --fixtures DIR --out DIR [--manifest FILE] [--width PX]');
  process.exit(2);
}

const server = spawn(process.execPath, [path.join(root, 'serve.mjs')], {
  cwd: root,
  env: { ...process.env, FIXTURES_DIR: fixturesDir },
  stdio: ['ignore', 'pipe', 'inherit'],
});

const port = await new Promise((resolve, reject) => {
  const timer = setTimeout(() => reject(new Error('server did not start')), 10000);
  server.stdout.once('data', (data) => {
    clearTimeout(timer);
    resolve(Number(data.toString().trim()));
  });
  server.once('error', reject);
});

const browser = await chromium.launch({ headless: true });
try {
  fs.mkdirSync(outRoot, { recursive: true });
  for (const fixture of manifest.files) {
    const page = await browser.newPage({
      viewport: { width: 1920, height: 1080 },
      deviceScaleFactor: 1,
    });
    try {
      const src = `/fixtures/${fixture.name}.pptx`;
      const url =
        `http://127.0.0.1:${port}/index.html` +
        `?src=${encodeURIComponent(src)}&width=${width}`;
      if (process.env.DEBUG) {
        page.on('console', (message) => console.error(`${fixture.name}: ${message.text()}`));
      }
      page.on('pageerror', (error) => console.error(`${fixture.name}: ${error.message}`));
      await page.goto(url, { waitUntil: 'load', timeout: 60000 });
      await page.waitForFunction(() => window.__result || window.__error, null, {
        timeout: 180000,
      });
      const error = await page.evaluate(() => window.__error);
      if (error) throw new Error(error);
      const slides = await page.evaluate(() => window.__result);
      if (slides.length !== fixture.slides) {
        throw new Error(
          `expected ${fixture.slides} slide(s), renderer produced ${slides.length}`,
        );
      }

      const fixtureOut = path.join(outRoot, fixture.name);
      fs.rmSync(fixtureOut, { recursive: true, force: true });
      fs.mkdirSync(fixtureOut, { recursive: true });
      for (const [index, slide] of slides.entries()) {
        const name = `slide_${String(index + 1).padStart(3, '0')}.png`;
        const payload = slide.data.split(',')[1];
        fs.writeFileSync(path.join(fixtureOut, name), Buffer.from(payload, 'base64'));
      }
      console.log(`rendered ${fixture.name}: ${slides.length} slide(s)`);
    } finally {
      await page.close();
    }
  }
} finally {
  await browser.close();
  server.kill();
}
