import { readFileSync } from 'node:fs';
import { Script, createContext } from 'node:vm';
import test from 'node:test';
import assert from 'node:assert/strict';

const script = new Script(readFileSync(new URL('../app/src/main/assets/instagram-producer-controls.js', import.meta.url), 'utf8'));
function page(labels, origin = 'https://www.instagram.com', pathname = '/') {
  let scrolled = 0;
  const buttons = labels.map(text => ({
    textContent: text, getAttribute: () => null, getClientRects: () => [1],
    scrollIntoView: () => { scrolled++; },
    click: () => { throw new Error('Publishing must never be clicked'); },
  }));
  const window = {}; window.top = window;
  return { run: () => script.runInContext(createContext({ window, location: { origin, pathname },
    document: { querySelectorAll: () => buttons }, getComputedStyle: () => ({ display: 'block', visibility: 'visible' }) })),
  scrolled: () => scrolled };
}
test('scrolls to one visible Turkish or English publishing button without clicking', () => {
  for (const label of ['Go live', 'Canlı yayına geç', 'Canlı yayını başlat']) {
    const fixture = page(['Back', label]);
    assert.equal(fixture.run(), true);
    assert.equal(fixture.scrolled(), 1);
  }
});
test('ambiguous or absent publishing controls do not move the page', () => {
  for (const labels of [[], ['Go live', 'Go live'], ['End live video']]) {
    const fixture = page(labels);
    assert.equal(fixture.run(), false);
    assert.equal(fixture.scrolled(), 0);
  }
});
test('other origins and authentication pages never navigate controls', () => {
  for (const [origin, path] of [['https://www.facebook.com', '/'], ['https://www.instagram.com', '/accounts/login/'],
    ['https://www.instagram.com', '/challenge/']]) {
    const fixture = page(['Go live'], origin, path);
    assert.equal(fixture.run(), false);
    assert.equal(fixture.scrolled(), 0);
  }
});
