import http from 'node:http';
import fs from 'node:fs';
import path from 'node:path';
import puppeteer from 'puppeteer-core';

const ROOT = process.cwd();
const MIME = { '.html': 'text/html', '.js': 'application/javascript', '.css': 'text/css', '.json': 'application/json', '.svg': 'image/svg+xml', '.png': 'image/png' };

const server = http.createServer((req, res) => {
  const urlPath = (req.url || '/').split('?')[0];
  let filePath = path.join(ROOT, urlPath === '/' ? 'index.html' : urlPath);
  if (!fs.existsSync(filePath) || fs.statSync(filePath).isDirectory()) filePath = path.join(ROOT, 'index.html');
  const ext = path.extname(filePath);
  res.writeHead(200, { 'Content-Type': MIME[ext] || 'application/octet-stream' });
  fs.createReadStream(filePath).pipe(res);
});

const executablePath = process.env.CHROME_PATH || '/Applications/Google Chrome.app/Contents/MacOS/Google Chrome';
if (!fs.existsSync(executablePath)) {
  console.error('No Chrome executable found at ' + executablePath);
  process.exit(1);
}

const targetUrl = process.env.SMOKE_URL || '';
if (!targetUrl) {
  await new Promise((resolve) => server.listen(0, '127.0.0.1', resolve));
}
const baseUrl = targetUrl || `http://127.0.0.1:${server.address().port}/`;

const browser = await puppeteer.launch({ executablePath, headless: true, args: ['--no-sandbox'] });
const routes = ['#home', '#games', '#vault-account', '#fair', '#responsible', '#bets', '#limits'];
const viewports = [
  { name: 'desktop', width: 1280, height: 800 },
  { name: 'phone', width: 390, height: 844 },
];

const errors = [];
try {
  for (const vp of viewports) {
    const page = await browser.newPage();
    await page.setViewport({ width: vp.width, height: vp.height });
    page.on('pageerror', (e) => errors.push(`${vp.name} pageerror: ${e.message}`));
    page.on('console', (msg) => {
      if (msg.type() === 'error') {
        const txt = msg.text();
        if (!/ERR_NAME_NOT_RESOLVED|Failed to fetch|net::ERR_|NetworkError|CORS policy/i.test(txt)) {
          errors.push(`${vp.name} console.error: ${txt}`);
        }
      }
    });
    for (const r of routes) {
      await page.goto(baseUrl + r, { waitUntil: 'domcontentloaded' });
      await new Promise((res) => setTimeout(res, 150));
      const check = await page.evaluate(() => {
        const overflow = document.documentElement.scrollWidth > window.innerWidth + 1;
        const badText = [];
        const walk = document.createTreeWalker(document.body, NodeFilter.SHOW_TEXT);
        let n;
        while ((n = walk.nextNode())) {
          const tag = n.parentElement && n.parentElement.tagName;
          if (tag === 'SCRIPT' || tag === 'STYLE' || tag === 'NOSCRIPT') continue;
          const t = n.nodeValue || '';
          if (/\bundefined\b|\bNaN\b|\[object /.test(t)) badText.push(t.trim().slice(0, 60));
        }
        return { overflow, badText };
      });
      if (check.overflow) errors.push(`${vp.name} ${r}: horizontal overflow`);
      if (check.badText.length) errors.push(`${vp.name} ${r}: bad text: ${check.badText.join(', ')}`);
      console.log(`PASS ${vp.name} ${r}`);
    }
    await page.close();
  }
} finally {
  await browser.close();
  server.close();
}

if (errors.length) {
  console.error('SMOKE FAILURES:\n' + errors.join('\n'));
  process.exit(1);
}
console.log('ALL SMOKE CHECKS PASSED');
