import { readFileSync } from 'node:fs';
import { Script, createContext } from 'node:vm';
import test from 'node:test';
import assert from 'node:assert/strict';

const source = readFileSync(new URL('../app/src/main/assets/instagram-producer-end.js', import.meta.url), 'utf8');
function fixture({ labels = ['Canlı videoyu bitir'], heading = '', dialog = false, covered = [], disabled = [], auth = false,
  origin = 'https://www.instagram.com', pathname = '/', child = false, passwords = [] } = {}) {
  const clicks = [];
  const headings = heading ? [{ textContent: heading, getAttribute: () => null, getClientRects: () => [1],
    scrollIntoView() {}, getBoundingClientRect: () => ({ left: 0, top: 0, width: 100, height: 30 }), contains: () => false }] : [];
  const modal = { getClientRects: () => [1], querySelectorAll: () => headings };
  const buttons = labels.map((text, index) => ({ textContent: text,
    getAttribute: () => null, getClientRects: () => [1], disabled: disabled.includes(index),
    closest: selector => selector.includes('inert') ? null : dialog ? modal : null,
    scrollIntoView() {}, getBoundingClientRect: () => ({ left: index * 150, top: 50, width: 100, height: 30 }),
    contains: () => false, click() { clicks.push(text); },
  }));
  const window = {}; window.top = child ? {} : window;
  const context = createContext({ window, location: { origin, pathname },
    document: {
      querySelector: () => auth ? {} : null,
      querySelectorAll: selector => selector === 'input[type="password"]' ? passwords : selector.startsWith('button') ? buttons : selector.startsWith('[role="dialog"]') ? dialog ? [modal] : [] : headings,
      elementFromPoint: (x, y) => y < 40 ? headings[0] : covered.includes(Math.floor(x / 150)) ? {} : buttons[Math.floor(x / 150)],
    }, getComputedStyle: () => ({ visibility: 'visible', display: 'block' }),
  });
  return { clicks, run: (action = '', token = 'request', deadline = Date.now() + 10_000) => new Script(source
    .replace("/*ACTION*/''", JSON.stringify(action)).replace("/*TOKEN*/''", JSON.stringify(token)).replace('/*DEADLINE*/0', String(deadline))).runInContext(context) };
}

test('probe is read-only; End dispatch is once even if a callback is lost', () => {
  const page = fixture(); assert.equal(page.run(), 'end'); assert.deepEqual(page.clicks, []);
  assert.equal(page.run('end'), 'clicked'); assert.equal(page.run('end'), 'dispatched');
  assert.deepEqual(page.clicks, ['Canlı videoyu bitir']);
});
test('confirmation requires a matching heading, cancel control and an unambiguous end control', () => {
  const page = fixture({ labels: ['İptal', 'Bitir'], heading: 'Canlı videoyu bitir?', dialog: true });
  assert.equal(page.run(), 'confirm'); assert.equal(page.run('end'), 'waiting');
  assert.deepEqual(page.clicks, []); assert.equal(page.run('confirm'), 'clicked');
  assert.deepEqual(page.clicks, ['Bitir']);
  const other = fixture({ labels: ['İptal', 'Bitir'], heading: 'Delete replay?', dialog: true });
  assert.equal(other.run('confirm'), 'waiting'); assert.deepEqual(other.clicks, []);
});
test('explicit ended heading confirms completion; missing controls alone never do', () => {
  for (const heading of ['Canlı videon sona erdi', 'Live video ended']) {
    assert.equal(fixture({ labels: [], heading }).run(), 'ended');
  }
  assert.equal(fixture({ labels: [] }).run(), 'waiting');
  assert.equal(fixture({ labels: [], heading: 'Canlı video bağlantısı kesildi' }).run(), 'waiting');
});
test('a draft can be stopped without ever clicking Go live, share, delete or cancel', () => {
  const page = fixture({ labels: ['Go live', 'Delete video', 'Share', 'Cancel'], disabled: [0] });
  assert.equal(page.run('end'), 'idle'); assert.deepEqual(page.clicks, []);
});
test('ambiguous and covered end controls fail closed; disabled copies are ignored', () => {
  assert.equal(fixture({ labels: ['End live video', 'End live video'] }).run('end'), 'waiting');
  assert.equal(fixture({ covered: [0] }).run('end'), 'waiting');
  const page = fixture({ labels: ['End live video', 'End live video'], disabled: [0] });
  assert.equal(page.run('end'), 'clicked'); assert.equal(page.clicks.length, 1);
});
test('login frames, other origins and unarmed requests cannot end a broadcast', () => {
  for (const config of [{ auth: true }, { pathname: '/accounts/login/' }, { origin: 'https://evil.example' }, { child: true }]) {
    const page = fixture(config); assert.equal(page.run('end'), 'blocked'); assert.deepEqual(page.clicks, []);
  }
  const page = fixture(); assert.equal(page.run('end', ''), 'blocked'); assert.deepEqual(page.clicks, []);
});

test('a delayed renderer commit expires rather than ending a later broadcast', () => {
  const page = fixture(); assert.equal(page.run('end', 'expired-request', 0), 'expired');
  assert.deepEqual(page.clicks, []);
});

test('full Turkish confirmation question is recognized without matching deletion questions', () => {
  const page = fixture({ labels: ['İptal', 'Bitir'], heading: 'Canlı videoyu bitirmek istediğinden emin misin?', dialog: true });
  assert.equal(page.run(), 'confirm');
  const deletion = fixture({ labels: ['İptal', 'Bitir'], heading: 'Canlı videoyu silmek istediğinden emin misin?', dialog: true });
  assert.equal(deletion.run('confirm'), 'waiting'); assert.deepEqual(deletion.clicks, []);
});

function producerKey({ label = 'Stream key', readonly = true, nested = false } = {}) {
  const field = { readOnly: readonly, labels: nested ? [] : [{ textContent: label }],
    getAttribute: () => null, get value() { throw Error('Never read the stream key or password'); } };
  if (nested) field.parentElement = { textContent: '', querySelectorAll: () => [field], parentElement: {
    textContent: label, querySelectorAll: () => [field],
  } };
  return field;
}

test('the same readonly Producer key does not block End probing; unknown passwords remain blocked', () => {
  const page = fixture({ passwords: [producerKey({ nested: true })] });
  assert.equal(page.run(), 'end'); assert.deepEqual(page.clicks, []);
  assert.equal(fixture({ passwords: [producerKey({ label: 'Password' })] }).run(), 'blocked');
});
