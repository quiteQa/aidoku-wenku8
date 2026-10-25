// Exercise the release WASM through Aidoku's ABI, following aidoku-pixiv's host-test approach.
// HTTP, clock, defaults and HTML are deterministic mocks. The small fixture DOM
// below tests request lifecycles, not SwiftSoup's HTML/CSS parsing. No network or
// real account credentials are used.
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';

const wasm = new WebAssembly.Module(readFileSync(new URL('../target/wasm32-unknown-unknown/release/wenku8.wasm', import.meta.url)));
const uint = value => { const out = []; do { const b = value % 128; value = Math.floor(value / 128); out.push(b | (value ? 128 : 0)); } while (value); return out; };
const string = value => [...uint(Buffer.byteLength(value)), ...Buffer.from(value)];
const array = (items, encode) => [...uint(items.length), ...items.flatMap(encode)];

class Reader {
  constructor(data) { this.data = Buffer.from(data); this.pos = 0; }
  byte() { assert.ok(this.pos < this.data.length, 'truncated postcard result'); return this.data[this.pos++]; }
  uint() { let value = 0, scale = 1, b; do { b = this.byte(); value += (b & 127) * scale; scale *= 128; } while (b & 128); return value; }
  string() { const len = this.uint(); assert.ok(this.pos + len <= this.data.length); const s = this.data.subarray(this.pos, this.pos + len).toString(); this.pos += len; return s; }
  option(read) { return this.byte() ? read.call(this) : null; }
  array(read) { return Array.from({ length: this.uint() }, () => read.call(this)); }
  float() { const n = this.data.readFloatLE(this.pos); this.pos += 4; return n; }
  chapter() { return { key: this.string(), title: this.option(this.string), number: this.option(this.float), volume: this.option(this.float), date: this.option(this.uint), groups: this.option(() => this.array(this.string)), url: this.option(this.string), lang: this.option(this.string), thumbnail: this.option(this.string), locked: this.byte() }; }
  manga() { return { key: this.string(), title: this.string(), cover: this.option(this.string), artists: this.option(() => this.array(this.string)), authors: this.option(() => this.array(this.string)), description: this.option(this.string), url: this.option(this.string), tags: this.option(() => this.array(this.string)), status: this.uint(), rating: this.uint(), viewer: this.uint(), strategy: this.uint(), nextUpdate: this.option(this.uint), chapters: this.option(() => this.array(this.chapter)) }; }
  list() { return { entries: this.array(this.manga), next: !!this.byte() }; }
  pages() { return this.array(function () { const kind = this.uint(), value = this.string(); if (kind === 0) this.option(this.string); else assert.equal(kind, 1); this.option(this.string); this.byte(); this.option(this.string); return { kind, value }; }); }
}
const emptyManga = key => [...string(key), ...string(''), ...Array(12).fill(0)];
const chapterURL = 'https://www.wenku8.net/modules/article/reader.php?aid=3057&cid=176892';
const body = '<html><title>测试小说</title><body><div id="content">测试正文</div></body></html>';

async function host(initial = {}, respond = () => ({ body })) {
  const prefs = new Map(Object.entries(initial)), objects = new Map(), requests = [], sleeps = [], rates = [], logs = [];
  let api, nextId = 1, now = 1_792_800_000;
  const save = value => { const id = nextId++; objects.set(id, value); return id; };
  const memory = () => new Uint8Array(api.memory.buffer);
  const read = (p, n) => Buffer.from(memory().slice(p, p + n)).toString();
  const copy = (p, data) => { memory().set(data, p); return 0; };
  const textID = value => value == null ? -5 : save(Buffer.from(value));
  // Fixture DOM: only the successful detail/chapter selectors used by these tests.
  const select = (id, query) => {
    const node = objects.get(id);
    if (['html', 'body', '#content', '#acontent', 'h1', '#content h1', "td[align='center'][valign='middle'] b"].includes(query)) return save(node);
    if (query === 'title') return save({ ...node, source: '测试小说' });
    return -5;
  };
  const imports = {
    env: { print: (p, n) => logs.push(read(p, n)), abort: () => { throw Error('WASM aborted'); } },
    std: {
      destroy: id => objects.delete(id), current_date: () => now,
      buffer_len: id => objects.get(id)?.length ?? -1,
      read_buffer: (id, p) => copy(p, objects.get(id)),
      sleep: seconds => { assert.ok(seconds > 0 && seconds <= 30); sleeps.push(seconds); now += seconds; },
      parse_date: (p, n) => { const value = Date.parse(read(p, n)); return Number.isNaN(value) ? -1 : value / 1000; },
    },
    defaults: {
      get: (p, n) => { const key = read(p, n); if (!prefs.has(key)) return -1; const value = prefs.get(key); return save(Buffer.from(typeof value === 'string' ? string(value) : Array.isArray(value) ? array(value, string) : [Number(value)])); },
      set: (p, n, kind, ptr) => {
        const key = read(p, n); if (kind === 6) { prefs.delete(key); return 0; }
        const len = new DataView(api.memory.buffer).getInt32(ptr, true), r = new Reader(memory().slice(ptr + 8, ptr + len));
        prefs.set(key, kind === 4 ? r.string() : kind === 5 ? r.array(r.string) : !!r.byte()); return 0;
      },
    },
    net: {
      init: method => save({ method, headers: {} }),
      set_url: (id, p, n) => { objects.get(id).url = read(p, n); return 0; },
      set_header: (id, kp, kn, vp, vn) => { objects.get(id).headers[read(kp, kn).toLowerCase()] = read(vp, vn); return 0; },
      set_body: (id, p, n) => { objects.get(id).body = read(p, n); return 0; },
      set_timeout: (id, value) => { objects.get(id).timeout = value; return 0; },
      set_rate_limit: (permits, period, unit) => rates.push({ permits, period, unit }),
      send: id => {
        const request = objects.get(id); requests.push({ ...structuredClone(request), at: now });
        const response = respond(request, requests, prefs);
        if (response.error) return response.error;
        Object.assign(request, { status: response.status ?? 200, responseHeaders: response.headers ?? {}, finalURL: response.url ?? request.url, data: Buffer.from(response.body ?? body) });
        return 0;
      },
      get_status_code: id => objects.get(id).status,
      get_header: (id, p, n) => textID(objects.get(id).responseHeaders[read(p, n).toLowerCase()]),
      get_url: id => textID(objects.get(id).finalURL),
      data_len: id => objects.get(id).data.length,
      read_data: (id, p) => copy(p, objects.get(id).data),
    },
    html: {
      parse: (p, n, bp, bn) => save({ source: read(p, n), base: read(bp, bn) }),
      parse_fragment: (p, n) => save({ source: read(p, n), base: '' }),
      select_first: (id, p, n) => select(id, read(p, n)),
      select: () => -5,
      base_uri: id => textID(objects.get(id).base),
      text: id => textID(objects.get(id).source.replace(/<[^>]*>/g, '')),
      untrimmed_text: id => textID(objects.get(id).source.replace(/<[^>]*>/g, '')),
      html: id => textID(objects.get(id).source),
      attr: () => -5,
    },
  };
  // Any unexpectedly used host import fails loudly, rather than silently succeeding.
  for (const imp of WebAssembly.Module.imports(wasm)) {
    imports[imp.module] ??= {};
    imports[imp.module][imp.name] ??= () => { throw Error('Unmocked host import: ' + imp.module + '.' + imp.name); };
  }
  api = (await WebAssembly.instantiate(wasm, imports)).exports;
  const input = data => save(Buffer.from(data));
  const result = ptr => {
    assert.ok(ptr > 0, 'WASM error code: ' + ptr);
    const view = new DataView(api.memory.buffer), len = view.getInt32(ptr, true);
    if (len === -1) { const n = view.getInt32(ptr + 8, true), message = read(ptr + 12, n - 12); api.free_result(ptr); throw Error(message); }
    const data = Buffer.from(memory().slice(ptr + 8, ptr + len)); api.free_result(ptr); return new Reader(data);
  };
  api.start();
  return {
    prefs, requests, sleeps, rates, logs, now: () => now, tick: n => { now += n; },
    restart: () => api.start(),
    notify: key => api.handle_notification(input(string(key))),
    images: url => objects.get(result(api.get_image_request(input(string(url)), -1)).uint() / 2),
    pages: (url = chapterURL) => result(api.get_page_list(input(emptyManga('3057')), input([...string(url), ...Array(9).fill(0)]))).pages(),
    search: (query, page = 1) => result(api.get_search_manga_list(input(Buffer.from(query)), page, input([0]))).list(),
    deepLink: url => { const r = result(api.handle_deep_link(input(string(url)))); return r.byte() ? { book: r.option(r.string), chapter: r.option(r.string), listing: r.byte() } : null; },
    login: (key, cookies) => result(api.handle_web_login(input(string(key)), input(array(Object.keys(cookies), string)), input(array(Object.values(cookies), string)))).byte(),
  };
}
const auth = site => ({ wenku8_site: site, ['wenku8_login_' + (site.endsWith('cc') ? 'cc' : 'net') + '.keys']: ['jieqiUserInfo', 'PHPSESSID', 'cf_clearance', 'bad;cookie'], ['wenku8_login_' + (site.endsWith('cc') ? 'cc' : 'net') + '.values']: ['test_user', 'test_session', 'stale', 'bad'] });
let passed = 0;
async function test(name, run) { await run(); passed++; console.log('PASS ' + name); }

await test('default, disabled and invalid rate settings reach the actual host API', async () => {
  const h = await host(); assert.deepEqual(h.rates.at(-1), { permits: 5, period: 1, unit: 0 });
  h.prefs.set('wenku8_download_limit_enabled', false); h.notify('wenku8_download_limit_changed'); assert.equal(h.rates.at(-1).permits, 0);
  h.prefs.set('wenku8_download_limit_enabled', true); h.prefs.set('wenku8_download_requests_per_second', 'bad'); h.notify('wenku8_download_limit_changed'); assert.equal(h.rates.at(-1).permits, 5);
});
await test('Tencent image Referer and Cookie isolation survive WASM serialization', async () => {
  const h = await host(auth('wenku8.net'));
  for (const [url, expected] of [['https://docimg10.docs.qq.com/a.png', 'https://docs.qq.com/'], ['https://img.wenku8.com/a.jpg', 'https://www.wenku8.net'], ['https://example.test/a.png', undefined], ['https://docimg10.docs.qq.com.evil.test/a.png', undefined]]) {
    const r = h.images(url); assert.equal(r.headers.referer, expected); assert.equal(r.headers.cookie, undefined); assert.equal(r.headers['user-agent'], undefined);
  }
});
await test('429 chapter retry waits and rebuilds selected-site headers and cookies', async () => {
  const h = await host(auth('wenku8.cc'), (_, reqs) => reqs.length === 1 ? { status: 429, headers: { 'retry-after': '10' } } : { body });
  assert.ok(h.pages().length); assert.deepEqual(h.sleeps, [11]); assert.equal(h.requests.length, 2);
  assert.equal(h.requests[1].at - h.requests[0].at, 11);
  for (const r of h.requests) { assert.ok(r.url.startsWith('https://www.wenku8.cc/')); assert.equal(r.headers.referer, 'https://www.wenku8.cc'); assert.ok(r.headers.cookie.includes('jieqiUserInfo=test_user')); assert.ok(r.headers.cookie.includes('PHPSESSID=test_session')); assert.ok(!r.headers.cookie.includes('stale')); assert.ok(!r.headers.cookie.includes('bad;cookie')); }
});
await test('repeated 429 has a finite retry budget and preserves the remaining cooldown', async () => {
  const h = await host({}, () => ({ status: 429, headers: { 'retry-after': '2' } }));
  assert.throws(() => h.pages(), /429/); assert.equal(h.requests.length, 3); assert.deepEqual(h.sleeps, [3, 3]);
  assert.ok(Number(h.prefs.get('wenku8_server_cooldown_until')) > h.now());
});
await test('long cooldown survives restart and disabling the manual limiter', async () => {
  const h = await host({}, () => ({ status: 429, headers: { 'retry-after': '90' } }));
  assert.throws(() => h.pages(), /冷却/); assert.equal(h.requests.length, 1); assert.equal(h.sleeps.length, 0);
  h.prefs.set('wenku8_download_limit_enabled', false); h.restart(); assert.throws(() => h.pages(), /冷却/); assert.equal(h.requests.length, 1);
});
await test('HTTP-date Retry-After uses the host parser and one-second clock margin', async () => {
  const h = await host({}, (_, reqs) => reqs.length === 1 ? { status: 429, headers: { 'retry-after': new Date((h.now() + 7) * 1000).toUTCString() } } : { body });
  h.pages(); assert.deepEqual(h.sleeps, [8]);
});
await test('search POST is never replayed after 429', async () => {
  const h = await host({}, () => ({ status: 429, headers: { 'retry-after': '10' } }));
  assert.throws(() => h.search('测试'), /429/); assert.equal(h.requests.length, 1); assert.equal(h.requests[0].method, 1);
  assert.match(h.requests[0].body, /searchkey=%B2%E2%CA%D4/);
});
await test('403, login redirects and CF challenges never trigger automatic chapter retries', async () => {
  for (const response of [{ status: 403 }, { status: 429, headers: { 'cf-mitigated': 'challenge' } }, { url: 'https://www.wenku8.net/login.php' }]) {
    const h = await host({}, () => response); assert.throws(() => h.pages(), /403|验证|登录/); assert.equal(h.requests.length, 1); assert.deepEqual(h.sleeps, []);
  }
});
await test('invalid saved chapter URLs fail before making an HTTP request', async () => {
  const h = await host(); assert.throws(() => h.pages('https://example.test/reader.php?aid=3057&cid=1'), /章节链接/); assert.equal(h.requests.length, 0);
});
await test('ID and link lookup bypass search cooldown and never request the search endpoint', async () => {
  const h = await host({ wenku8_last_search_at: '1792800000' });
  for (const query of ['3057', 'https://www.wenku8.cc/book/3057.htm']) {
    assert.equal(h.search(query).entries[0].key, '3057'); assert.equal(h.requests.at(-1).url, 'https://www.wenku8.net/book/3057.htm');
  }
  const count = h.requests.length; assert.equal(h.search('3057', 2).entries.length, 0); assert.equal(h.requests.length, count);
  assert.throws(() => h.search('测试'), /搜索冷却/);
});
await test('deep links preserve chapter identity while selecting the active site', async () => {
  const h = await host({ wenku8_site: 'wenku8.cc' });
  assert.deepEqual(h.deepLink(chapterURL), { book: '3057', chapter: chapterURL.replace('wenku8.net', 'wenku8.cc'), listing: 0 });
  assert.deepEqual(h.deepLink('https://www.wenku8.net/book/3057.htm'), { book: '3057', chapter: null, listing: 0 });
  assert.equal(h.deepLink('https://evil.test/book/3057.htm'), null);
});
await test('web login persists cookies and logout clears the callback snapshot', async () => {
  const h = await host(); assert.equal(h.login('wenku8_login_net', { jieqiUserInfo: 'test_user', PHPSESSID: 'test_session' }), 1);
  h.pages(); assert.ok(h.requests[0].headers.cookie.includes('test_session'));
  assert.equal(h.login('wenku8_login_net', {}), 0); h.pages(); assert.equal(h.requests.at(-1).headers.cookie, undefined);
});
console.log(passed + ' WASM network integration tests passed');
