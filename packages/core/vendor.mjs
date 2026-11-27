// Copies dist/openhouse-core.js to ../../vendor/openhouse-core.js, computes sha384, and updates ../../index.html SRI integrity.
import { createHash } from 'node:crypto';
import { readFileSync, writeFileSync, copyFileSync } from 'node:fs';
import { resolve } from 'node:path';

const distFile = resolve('dist/openhouse-core.js');
const vendorFile = resolve('../../vendor/openhouse-core.js');
const indexHtmlFile = resolve('../../index.html');

copyFileSync(distFile, vendorFile);
const buf = readFileSync(vendorFile);
const sha384 = createHash('sha384').update(buf).digest('base64');
const integrity = `sha384-${sha384}`;

let html = readFileSync(indexHtmlFile, 'utf8');
const re = /(<script\s+src="\/vendor\/openhouse-core\.js"\s+integrity=")sha384-[^"]+(")/;
if (!re.test(html)) {
  throw new Error('Could not find openhouse-core.js integrity attribute in index.html');
}
html = html.replace(re, `$1${integrity}$2`);
writeFileSync(indexHtmlFile, html, 'utf8');
console.log(`Updated vendor/openhouse-core.js (${buf.length} bytes) and index.html integrity=${integrity}`);
