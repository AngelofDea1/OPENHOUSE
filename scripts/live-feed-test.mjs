import fs from 'node:fs';
import puppeteer from 'puppeteer-core';

const executablePath = process.env.CHROME_PATH || '/Applications/Google Chrome.app/Contents/MacOS/Google Chrome';
const baseUrl = process.env.SMOKE_URL || 'https://openhouse-ten.vercel.app';
const browser = await puppeteer.launch({ executablePath, headless: true, args: ['--no-sandbox'] });
const errors = [];

try {
  const page = await browser.newPage();
  await page.setViewport({ width: 1280, height: 800 });
  page.on('pageerror', (e) => errors.push(`pageerror: ${e.message}`));
  page.on('console', (msg) => {
    if (msg.type() === 'error') {
      const txt = msg.text();
      if (!/429|Too Many Requests/i.test(txt)) errors.push(`console.error: ${txt}`);
    }
  });

  for (const r of ['/#fair', '/#leaderboard', '/#me', '/#home']) {
    await page.goto(baseUrl + r, { waitUntil: 'domcontentloaded' });
    await new Promise((res) => setTimeout(res, 1500));
    console.log('Loaded', r);
  }

  await page.goto(baseUrl + '/#fair', { waitUntil: 'domcontentloaded' });
  await page.waitForFunction(() => {
    const el = document.querySelector('#fairFeedList a[href*="/tx/"]');
    return !!el;
  }, { timeout: 20000 });

  const rowInfo = await page.evaluate(() => {
    const list = document.querySelector('#fairFeedList');
    const a = list.querySelector('a[href*="/tx/"]');
    const m = a.href.match(/\/tx\/([A-Za-z0-9]+)/);
    return { text: list.textContent.slice(0, 160), txSig: m ? m[1] : '' };
  });
  console.log('Real feed row:', rowInfo.text.trim());
  console.log('Real tx signature:', rowInfo.txSig);
  if (!rowInfo.txSig) throw new Error('No real tx signature found in feed');

  await page.type('#vTxIn', rowInfo.txSig);
  await page.click('#vTxGo');
  await page.waitForFunction(() => {
    const out = document.querySelector('#vTxOut');
    return out && out.textContent.includes('Seed hash match:');
  }, { timeout: 15000 });

  const verifyText = await page.evaluate(() => document.querySelector('#vTxOut').textContent);
  console.log('Verify any bet output:', verifyText);
  if (!verifyText.includes('Seed hash match: PASS') || !verifyText.includes('Server Seed:') || !verifyText.includes('Client Seed:')) {
    throw new Error('Verification output missing expected fields: ' + verifyText);
  }

  if (errors.length) {
    console.error('Console/Page Errors:', errors);
    process.exitCode = 1;
  } else {
    console.log('ALL LIVE FEED & VERIFY CHECKS PASSED');
  }
} finally {
  await browser.close();
}
