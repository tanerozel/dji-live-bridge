import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { test } from 'node:test';
import vm from 'node:vm';

const script = readFileSync(new URL('../app/src/main/assets/instagram-producer-capture.js', import.meta.url), 'utf8');
function field(value, label, { type = 'text', readonly = true, visible = true, forbidRead = false } = {}) {
  const node = {
    get value() { if (forbidRead) throw new Error('Login data must never be read'); return value; },
    readOnly: readonly, labels: [{ textContent: label }],
    getAttribute(name) { return ({ type, 'aria-label': label })[name] ?? null; },
    getClientRects() { return visible ? [{}] : []; },
  };
  node.parentElement = { textContent: label, querySelectorAll: () => [node] };
  return node;
}
const server = () => field('rtmps://live-upload.instagram.com:443/rtmp/', 'Stream URL');
const key = () => field('IG_fixture-key-123?s_vt=ig', 'Stream key', { type: 'password' });
function capture(fields, { origin = 'https://www.instagram.com', path = '/', login = false, child = false, titleRecord = null, titleRevision = '', audienceRecord = null, audience = 'PRACTICE' } = {}) {
  const window = {}; window.top = child ? {} : window;
  if (titleRecord) window[Symbol.for('streammydrone.instagram.titleRecord')] = titleRecord;
  if (audienceRecord) window[Symbol.for('streammydrone.instagram.audienceRecord')] = audienceRecord;
  return JSON.parse(vm.runInNewContext(script.replaceAll("/*TITLE_REVISION*/''", JSON.stringify(titleRevision)).replace("/*AUDIENCE*/'PRACTICE'", JSON.stringify(audience)), {
    window, location: { origin, pathname: path }, URL,
    getComputedStyle: () => ({ visibility: 'visible', display: 'block' }),
    document: { querySelector: () => login ? {} : null, querySelectorAll: selector => selector === 'input[type="password"]' ? fields.filter(field => field.getAttribute('type') === 'password') : fields },
  }));
}

test('captures a labelled stream key with signed query parameters', () => {
  const result = capture([server(), key()]);
  assert.equal(result.status, 'ready');
  assert.equal(result.streamKey, 'IG_fixture-key-123?s_vt=ig');
});
test('never reads login, 2FA, other-origin or child-frame inputs', () => {
  const secret = field('', 'Password', { forbidRead: true });
  for (const options of [{ path: '/accounts/login/' }, { path: '/challenge/123' }, { login: true }, { origin: 'https://evil.example' }, { child: true }]) {
    assert.equal(capture([secret], options).status, 'waiting');
  }
});
test('an unnamed login-password overlay blocks every value read even on a producer page', () => {
  assert.equal(capture([server(), key(), field('', 'Password', { type: 'password', forbidRead: true })]).status, 'waiting');
});
test('requires a visible trusted server without credentials embedded in its path', () => {
  for (const url of ['rtmp://live-upload.instagram.com/rtmp', 'rtmps://instagram.com.evil.example/rtmp', 'rtmps://live-upload.instagram.com/rtmp/secret']) {
    assert.equal(capture([field(url, 'Stream URL'), key()]).status, 'waiting');
  }
  assert.equal(capture([field('rtmps://live-upload.instagram.com/rtmp', 'Stream URL', { visible: false }), key()]).status, 'waiting');
});
test('never imports masked placeholders or ambiguous key fields', () => {
  assert.equal(capture([server(), field('****abcd', 'Stream key')]).status, 'waiting');
  assert.equal(capture([server(), key(), field('IG_other-fixture-key', 'Stream key')]).status, 'waiting');
});
test('supports Turkish labels and a regional fbcdn address', () => {
  const result = capture([field('rtmps://edgetee-upload-ist1-2.xx.fbcdn.net:443/rtmp', 'Yayın URL’si'), field('IG_test-key-1234', 'Yayın anahtarı')]);
  assert.equal(result.status, 'ready');
});

test('does not substitute another readonly profile field when the key is masked', () => {
  assert.equal(capture([server(), field('****abcd', 'Stream key'), field('my-channel-name', 'Channel')]).status, 'waiting');
});

test('captures real Producer layout with a caption above the wrapping LABEL', () => {
  const address = server();
  const streamKey = field('IG_nested-fixture-key', '');
  const shared = { textContent: 'Yayın URL’si Yayın anahtarı', querySelectorAll: () => [address, streamKey] };
  streamKey.parentElement = { textContent: '', querySelectorAll: () => [streamKey], parentElement: {
    textContent: 'Yayın anahtarı', querySelectorAll: () => [streamKey], parentElement: shared,
  } };
  address.parentElement = { textContent: '', querySelectorAll: () => [address], parentElement: {
    textContent: 'Yayın URL’si', querySelectorAll: () => [address], parentElement: shared,
  } };
  assert.equal(capture([address, streamKey]).status, 'ready');
});

test('a shared container caption cannot relabel another field as a key', () => {
  const address = server();
  const hiddenKey = field('****abcd', 'Stream key');
  const unrelated = field('my-channel-name', 'Channel');
  const shared = { textContent: 'Stream key Channel', querySelectorAll: () => [hiddenKey, unrelated] };
  unrelated.parentElement = { textContent: 'Channel', querySelectorAll: () => [unrelated], parentElement: shared };
  hiddenKey.parentElement = { textContent: 'Stream key', querySelectorAll: () => [hiddenKey], parentElement: shared };
  assert.equal(capture([address, hiddenKey, unrelated]).status, 'waiting');
});

test('capture proves that the chosen title belongs to this request without returning its text', () => {
  const titleRecord = { revision: 'start-1', desired: 'Shared private title', actual: 'Shared private title' };
  const result = capture([server(), key()], { titleRecord, titleRevision: 'start-1' });
  assert.equal(result.titleMatches, true);
  assert.equal(JSON.stringify(result).includes('Shared private title'), false);
  assert.equal(capture([server(), key()], { titleRecord, titleRevision: 'start-2' }).titleMatches, false);
  titleRecord.actual = 'Changed on Instagram';
  assert.equal(capture([server(), key()], { titleRecord, titleRevision: 'start-1' }).titleMatches, false);
});

test('native capture requires audience proof for the same setup revision and choice', () => {
  const audienceRecord = { revision: 'start-1', audience: 'PRACTICE' };
  assert.equal(capture([server(), key()], { titleRevision: 'start-1', audienceRecord }).audienceMatches, true);
  assert.equal(capture([server(), key()], { titleRevision: 'start-2', audienceRecord }).audienceMatches, false);
  assert.equal(capture([server(), key()], { titleRevision: 'start-1', audienceRecord, audience: 'PUBLIC' }).audienceMatches, false);
});
