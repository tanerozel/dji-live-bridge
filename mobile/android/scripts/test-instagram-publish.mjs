import { readFileSync } from 'node:fs';
import { Script, createContext } from 'node:vm';
import test from 'node:test';
import assert from 'node:assert/strict';

const source = readFileSync(new URL('../app/src/main/assets/instagram-producer-publish.js', import.meta.url), 'utf8');
function fixture({ labels = ['Go live'], disabled = false, covered = false, auth = false,
  origin = 'https://www.instagram.com', pathname = '/', child = false, passwords = [] } = {}) {
  let clicks = 0;
  const buttons = labels.map((text, index) => ({
    textContent: text, disabled: Array.isArray(disabled) ? disabled.includes(index) : disabled,
    getAttribute: () => null, getClientRects: () => [1], closest: () => null,
    scrollIntoView() {}, getBoundingClientRect: () => ({ left: 10 + index * 150, top: 10, width: 100, height: 40 }),
    contains: () => false, click() { clicks++; },
  }));
  const window = {}; window.top = child ? {} : window;
  const context = createContext({ window, location: { origin, pathname, href: origin + pathname },
    document: { querySelector: () => auth ? {} : null, querySelectorAll: selector => selector === 'input[type="password"]' ? passwords : buttons,
      elementFromPoint: x => {
        const index = Math.floor((x - 10) / 150);
        return (Array.isArray(covered) ? covered.includes(index) : covered) ? {} : buttons[index];
      } },
    getComputedStyle: () => ({ display: 'block', visibility: 'visible' }),
  });
  return { clicks: () => clicks, run: (click = false, token = 'request-1', deadline = Date.now() + 10_000, expected = origin + pathname) =>
    new Script(source.replace('/*CLICK*/false', String(click)).replace("/*TOKEN*/''", JSON.stringify(token)).replace('/*DEADLINE*/0', String(deadline)).replace("/*EXPECTED_HREF*/''", JSON.stringify(expected))).runInContext(context) };
}
test('probing readiness never publishes; explicit commit clicks once per request', () => {
  const page = fixture();
  assert.equal(page.run(), 'ready'); assert.equal(page.clicks(), 0);
  assert.equal(page.run(true), 'clicked'); assert.equal(page.clicks(), 1);
  assert.equal(page.run(true), 'dispatched'); assert.equal(page.clicks(), 1);
});
test('disabled covered ambiguous and missing controls never publish', () => {
  for (const config of [{ disabled: true }, { covered: true }, { labels: [] }, { labels: ['Go live', 'Go live'] },
    { labels: ['Next', 'Public', 'Practice'] }, { labels: ['End live video'], disabled: true }]) {
    const page = fixture(config); assert.equal(page.run(true), 'waiting'); assert.equal(page.clicks(), 0);
  }
});
test('no authorization token and authentication documents cannot publish', () => {
  const noToken = fixture(); assert.equal(noToken.run(true, ''), 'blocked'); assert.equal(noToken.clicks(), 0);
  for (const config of [{ auth: true }, { pathname: '/accounts/login/' }, { pathname: '/challenge/' },
    { origin: 'https://evil.example' }, { child: true }]) {
    const page = fixture(config); assert.equal(page.run(true), 'blocked'); assert.equal(page.clicks(), 0);
  }
});
test('live requires an end control without a start control, never clicks End or audience', () => {
  for (const label of ['End live video', 'Canlı videoyu bitir', 'Canlı yayını bitir']) {
    const page = fixture({ labels: [label, 'Practice'] });
    assert.equal(page.run(true), 'live'); assert.equal(page.clicks(), 0);
  }
  const ambiguous = fixture({ labels: ['Go live', 'End live video'] });
  assert.equal(ambiguous.run(true), 'waiting'); assert.equal(ambiguous.clicks(), 0);
});
test('Turkish publish control is recognized and does not change audience controls', () => {
  const page = fixture({ labels: ['Canlı yayına geç', 'Alıştırma'] });
  assert.equal(page.run(true), 'clicked'); assert.equal(page.clicks(), 1);
});

test('disabled or covered duplicate controls from old dialog layers do not block the active button', () => {
  for (const config of [{ disabled: [0] }, { covered: [0] }]) {
    const page = fixture({ labels: ['Go live', 'Go live'], ...config });
    assert.equal(page.run(true), 'clicked'); assert.equal(page.clicks(), 1);
  }
});

function producerKey({ label = 'Stream key', readonly = true, nested = false } = {}) {
  const field = { readOnly: readonly, labels: nested ? [] : [{ textContent: label }],
    getAttribute: () => null, get value() { throw Error('Never read the stream key or password'); } };
  if (nested) field.parentElement = { textContent: '', querySelectorAll: () => [field], parentElement: {
    textContent: label, querySelectorAll: () => [field],
  } };
  return field;
}

test('readonly Producer stream-key password fields do not block the explicit Go live request', () => {
  for (const field of [producerKey(), producerKey({ label: 'Yayın anahtarı', nested: true })]) {
    const page = fixture({ passwords: [field] });
    assert.equal(page.run(), 'ready'); assert.equal(page.clicks(), 0);
    assert.equal(page.run(true), 'clicked'); assert.equal(page.clicks(), 1);
  }
});
test('unknown or editable password fields still block publication without reading them', () => {
  for (const field of [producerKey({ label: 'Password' }), producerKey({ readonly: false })]) {
    const page = fixture({ passwords: [field] });
    assert.equal(page.run(true), 'blocked'); assert.equal(page.clicks(), 0);
  }
});
test('readonly key on a live Producer page permits read-only confirmation of a manually started broadcast', () => {
  const page = fixture({ labels: ['Canlı videonu sonlandır'], passwords: [producerKey()] });
  assert.equal(page.run(), 'live'); assert.equal(page.clicks(), 0);
});

test('a delayed commit cannot click after its deadline or on a different page', () => {
  const expired = fixture(); assert.equal(expired.run(true, 'request', 0), 'expired'); assert.equal(expired.clicks(), 0);
  const moved = fixture(); assert.equal(moved.run(true, 'request', Date.now() + 10_000, 'https://www.instagram.com/other/'), 'blocked'); assert.equal(moved.clicks(), 0);
});
