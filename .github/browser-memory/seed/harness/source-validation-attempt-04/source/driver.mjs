// Unprivileged Playwright driver + loopback server, outside measured cgroup.
import { readFile, realpath } from 'node:fs/promises';
import { createServer } from 'node:http';
import { createHash } from 'node:crypto';
import { dirname, join, extname } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { createInterface } from 'node:readline';
const here = dirname(fileURLToPath(import.meta.url));
const { job, bindings: b, packageFiles, wrapper } = JSON.parse(await readFile(process.argv[2]));
const sha = bytes => createHash('sha256').update(bytes).digest('hex');
const require = (value, message) => { if (!value) throw Error(message); };
const profiles = JSON.parse(await readFile(join(here, 'PROFILES.json')));
const artifact = job.artifact ? b.artifacts[job.artifact] : null;
const fixtureName = job.mode === 'lifecycle' ? 'replacement' : job.profile;
const fixture = fixtureName ? b.fixtures[fixtureName] : null;
const requests = [], errors = [], workers = [];
let waiting = null, browserServer, browser;
const stdin = createInterface({ input: process.stdin });
stdin.on('line', line => {
  if (line !== 'ack' || !waiting) throw Error('unexpected supervisor acknowledgment');
  const resolve = waiting; waiting = null; resolve();
});
const phase = row => new Promise(resolve => {
  require(waiting === null, 'overlapping phases'); waiting = resolve;
  process.stdout.write(JSON.stringify({ type: 'phase', row }) + '\n');
});
const server = createServer(async (req, res) => {
  try {
    let bytes, expected;
    if (req.url === '/') bytes = Buffer.from('<!doctype html><title>Linux group-memory qualification</title>');
    else if (req.url === '/worker.mjs') bytes = await readFile(join(here, 'worker.mjs'));
    else if (req.url === '/fixture' && fixture) { bytes = await readFile(fixture.path); expected = fixture.sha256; }
    else if (req.url.startsWith('/sdk/') && artifact) {
      const relative = req.url.slice(5);
      const registered = packageFiles[job.artifact].find(f => f.path === relative);
      require(registered, 'unregistered asset');
      bytes = await readFile(join(artifact.packageRoot, relative)); expected = registered.sha256;
    } else { res.writeHead(404); res.end(); return; }
    const digest = sha(bytes); require(!expected || digest === expected, 'served asset bytes changed');
    requests.push({ url: req.url, bytes: bytes.length, sha256: digest });
    res.writeHead(200, { 'content-type': req.url === '/' ? 'text/html' : extname(req.url) === '.wasm' ? 'application/wasm' : 'text/javascript', 'cache-control': 'no-store' });
    res.end(bytes);
  } catch (error) { errors.push(String(error)); res.writeHead(500); res.end('qualification asset error'); }
});
let result = { type: 'result', status: 'failed' };
try {
  const { [job.browser]: type } = await import(pathToFileURL(join(b.playwright.root, 'index.mjs')));
  await new Promise((resolve, reject) => { server.once('error', reject); server.listen(0, '127.0.0.1', resolve); });
  const origin = 'http://127.0.0.1:' + server.address().port;
  browserServer = await type.launchServer({ executablePath: wrapper, headless: true, env: process.env, timeout: 30000 });
  browser = await type.connect(browserServer.wsEndpoint(), { timeout: 30000 });
  require(browser.version() === b.browsers[job.browser].version, 'browser runtime version differs');
  const context = await browser.newContext();
  await context.route('**/*', route => route.request().url().startsWith(origin + '/') ? route.continue() : route.abort());
  const page = await context.newPage();
  page.on('pageerror', error => errors.push(String(error)));
  page.on('worker', worker => workers.push(worker.url()));
  await page.exposeFunction('recordKernelPhase', phase);
  await page.goto(origin, { timeout: 15000 });
  const worker = await page.evaluate(job => new Promise((resolve, reject) => {
    const worker = new Worker('/worker.mjs', { type: 'module', name: job.id });
    const watchdog = setTimeout(() => { worker.terminate(); reject(Error('external150secondWorkerTimeout')); }, 150000);
    worker.onerror = event => { clearTimeout(watchdog); worker.terminate(); reject(Error(event.message)); };
    worker.onmessage = async ({ data }) => {
      try {
        if (data.type === 'phase') { await window.recordKernelPhase(data.row); worker.postMessage({ ack: data.id }); }
        else if (data.type === 'result') { clearTimeout(watchdog); worker.terminate(); resolve(data.report); }
        else throw Error('unknown Worker message');
      } catch (error) { clearTimeout(watchdog); worker.terminate(); reject(error); }
    };
    worker.postMessage({ run: job });
  }), { ...job, wasmSha256: artifact?.wasmSha256, fixtureSha256: fixture?.sha256, facts: profiles[job.profile]?.expectedFacts });
  require(workers.length === 1 && workers[0] === origin + '/worker.mjs', 'exactly one actual Worker required');
  require(errors.length === 0 && worker.status === 'passed', 'Worker or asset failure');
  require(job.mode !== 'control' || (worker.sdkLoaded === false && !requests.some(r => r.url.startsWith('/sdk/'))), 'SDK-free control loaded SDK');
  result = { ...result, status: 'passed', worker, version: browser.version(),
    launch: { pid: browserServer.process().pid, spawnfile: await realpath(browserServer.process().spawnfile), args: browserServer.process().spawnargs } };
} catch (error) { result.error = { message: error.message, stack: error.stack }; process.exitCode = 1; }
finally {
  try { await browser?.close(); await browserServer?.close(); } catch (error) { errors.push(String(error)); result.status = 'failed'; process.exitCode = 1; }
  if (server.listening) await new Promise(resolve => server.close(resolve));
  stdin.close();
  process.stdout.write(JSON.stringify({ ...result, requests, errors, workers }) + '\n');
}
