const { chromium } = require('@playwright/test');
const fs = require('node:fs');
const http = require('node:http');
const path = require('node:path');
const { URL } = require('node:url');

const viewport = { width: 1700, height: 850 };
const buildDir = path.join(__dirname, '..', 'site', 'build');
const basePath = '/blipcoard/';

function roundBox(box) {
  return {
    x: Math.round(box.x),
    y: Math.round(box.y),
    width: Math.round(box.width),
    height: Math.round(box.height),
  };
}

function assert(condition, message) {
  if (!condition) {
    throw new Error(message);
  }
}

function contentType(filePath) {
  switch (path.extname(filePath)) {
    case '.css':
      return 'text/css';
    case '.html':
      return 'text/html';
    case '.js':
      return 'text/javascript';
    case '.json':
      return 'application/json';
    case '.png':
      return 'image/png';
    case '.svg':
      return 'image/svg+xml';
    case '.woff2':
      return 'font/woff2';
    default:
      return 'application/octet-stream';
  }
}

function resolveBuildPath(requestUrl) {
  const url = new URL(requestUrl, 'http://127.0.0.1');
  let pathname = decodeURIComponent(url.pathname);
  if (!pathname.startsWith(basePath)) {
    return null;
  }

  pathname = pathname.slice(basePath.length);
  if (!pathname || pathname.endsWith('/')) {
    pathname = `${pathname}index.html`;
  }

  const candidate = path.normalize(path.join(buildDir, pathname));
  if (!candidate.startsWith(buildDir)) {
    return null;
  }
  if (fs.existsSync(candidate) && fs.statSync(candidate).isDirectory()) {
    return path.join(candidate, 'index.html');
  }
  if (!fs.existsSync(candidate) && fs.existsSync(`${candidate}.html`)) {
    return `${candidate}.html`;
  }
  return candidate;
}

function startStaticServer() {
  return new Promise((resolve, reject) => {
    const server = http.createServer((request, response) => {
      const filePath = resolveBuildPath(request.url || basePath);
      if (!filePath || !fs.existsSync(filePath)) {
        response.writeHead(404);
        response.end('Not found');
        return;
      }

      response.writeHead(200, { 'Content-Type': contentType(filePath) });
      fs.createReadStream(filePath).pipe(response);
    });

    server.on('error', reject);
    server.listen(0, '127.0.0.1', () => {
      const address = server.address();
      resolve({ server, url: `http://127.0.0.1:${address.port}${basePath}` });
    });
  });
}

(async () => {
  const hosted = process.env.DOCS_BASE_URL
    ? { server: null, url: process.env.DOCS_BASE_URL }
    : await startStaticServer();
  const browser = await chromium.launch();
  try {
    const page = await browser.newPage({ viewport, colorScheme: 'dark', deviceScaleFactor: 1 });
    await page.goto(hosted.url, { waitUntil: 'networkidle' });

    const idle = await page.locator('.navbar__search-input').boundingBox();
    assert(idle, 'Docs search input was not found');

    const idleBox = roundBox(idle);
    assert(idleBox.y >= 8 && idleBox.y <= 14, `Idle search input is vertically off: ${JSON.stringify(idleBox)}`);
    assert(idleBox.height === 38, `Idle search input height changed: ${JSON.stringify(idleBox)}`);

    await page.locator('.navbar__search-input').click();
    await page.locator('.navbar__search-input').fill('docs');
    await page.locator('[class*=dropdownMenu]').waitFor({ state: 'visible', timeout: 5000 });

    const input = await page.locator('.navbar__search-input').boundingBox();
    const dropdown = await page.locator('[class*=dropdownMenu]').boundingBox();
    assert(input, 'Focused docs search input was not found');
    assert(dropdown, 'Docs search dropdown was not found');

    const inputBox = roundBox(input);
    const dropdownBox = roundBox(dropdown);
    assert(inputBox.x === dropdownBox.x, `Search input and results x mismatch: ${JSON.stringify({ inputBox, dropdownBox })}`);
    assert(
      inputBox.width === dropdownBox.width,
      `Search input and results width mismatch: ${JSON.stringify({ inputBox, dropdownBox })}`,
    );

    console.log(`docs search layout ok: ${JSON.stringify({ idleBox, inputBox, dropdownBox })}`);
  } finally {
    await browser.close();
    if (hosted.server) {
      hosted.server.close();
    }
  }
})().catch((error) => {
  console.error(error.message);
  process.exit(1);
});
