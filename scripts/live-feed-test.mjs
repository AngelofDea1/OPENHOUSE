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
    if (msg.type() === 'error') errors.push(`console.error: ${msg.text()}`);
  });

  // (a) Feed on #fair
  await page.goto(baseUrl + '/#fair', { waitUntil: 'domcontentloaded' });
  await page.waitForFunction(() => !!document.querySelector('#fairFeedList a[href*="/tx/"]'), { timeout: 20000 });
  const rowInfo = await page.evaluate(() => {
    const list = document.querySelector('#fairFeedList');
    const a = list.querySelector('a[href*="/tx/"]');
    const m = a.href.match(/\/tx\/([A-Za-z0-9]+)/);
    return { text: list.textContent.trim().slice(0, 140), txSig: m ? m[1] : '' };
  });
  console.log('(a) Feed row:', rowInfo.text);
  console.log('Real tx signature:', rowInfo.txSig);
  if (!rowInfo.txSig) throw new Error('No real tx signature found in feed');

  // (b) Leaderboard
  await page.goto(baseUrl + '/#leaderboard', { waitUntil: 'domcontentloaded' });
  await page.waitForFunction(() => {
    const el = document.querySelector('#lbWagered');
    return el && el.textContent.includes('SOL') && el.textContent.includes('#1');
  }, { timeout: 20000 });
  const lbText = await page.evaluate(() => document.querySelector('#lbWagered').textContent.trim().slice(0, 120));
  console.log('(b) Leaderboard #1:', lbText);

  // (c) #me page (check /api/bets for a wallet that has bets)
  const betsRes = await page.evaluate(async () => {
    const r = await fetch('/api/bets?wallet=89t4acyUQy5YBqr11jHUav7odtu3oKkX12qZNaH2WPhv');
    return await r.json();
  });
  console.log('(c) /api/bets count for 89t4...WPhv:', betsRes.bets?.length || 0);
  if (!betsRes.ok || !betsRes.bets || betsRes.bets.length === 0) {
    throw new Error('Expected non-zero bets for 89t4acyUQy5YBqr11jHUav7odtu3oKkX12qZNaH2WPhv');
  }

  // (d) Verify any bet on #fair
  await page.goto(baseUrl + '/#fair', { waitUntil: 'domcontentloaded' });
  await page.type('#vTxIn', rowInfo.txSig);
  await page.click('#vTxGo');
  await page.waitForFunction(() => {
    const out = document.querySelector('#vTxOut');
    return out && out.textContent.includes('Seed hash match:');
  }, { timeout: 20000 });
  const verifyText = await page.evaluate(() => document.querySelector('#vTxOut').textContent);
  console.log('(d) Verify any bet output:', verifyText);
  if (!verifyText.includes('Seed hash match: PASS') || !verifyText.includes('Server Seed:') || !verifyText.includes('Client Seed:')) {
    throw new Error('Verification output missing expected fields: ' + verifyText);
  }

  if (errors.length) {
    console.error('Console/Page Errors:', errors);
    process.exitCode = 1;
  } else {
    console.log('ALL LIVE FEED, LEADERBOARD, PROFILE & VERIFY CHECKS PASSED');
  }
} finally {
  await browser.close();
}
