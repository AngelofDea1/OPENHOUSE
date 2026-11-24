// Bundles the shared client for the browser. @solana/web3.js comes from the page (window.solanaWeb3), not from the bundle.
import { build } from 'esbuild';
import { writeFileSync, mkdirSync } from 'node:fs';
mkdirSync('dist', { recursive: true });
writeFileSync('dist/web3-shim.js', 'const w = window.solanaWeb3;\nexport const PublicKey = w.PublicKey, SystemProgram = w.SystemProgram, TransactionInstruction = w.TransactionInstruction;\n');
await build({
  entryPoints: ['src/index.ts'], bundle: true, format: 'iife', globalName: 'OHProgram', minify: true, target: 'es2020',
  outfile: 'dist/openhouse-core.js', alias: { '@solana/web3.js': './dist/web3-shim.js' },
});
console.log('ok');
