import { readFileSync } from 'node:fs';
import { Script, createContext } from 'node:vm';
import test from 'node:test';
import assert from 'node:assert/strict';

const source = readFileSync(new URL('../app/src/main/assets/instagram-producer-open.js', import.meta.url), 'utf8');
function page({ labels = [], spanLabels = [], dialogs = false, covered = false, auth = false, origin = 'https://www.instagram.com', pathname = '/', child = false, icon = null } = {}) {
  const clicked = [];
  const hit = {};
  const buttons = labels.map(text => ({ textContent: text, getAttribute: () => null,
    getClientRects: () => [1], querySelectorAll: () => [], click: () => { clicked.push(text); } }));
  if (icon) buttons.push({ textContent: '', getAttribute: () => null, getClientRects: () => [1],
    querySelectorAll: () => [{ getAttribute: () => icon }], click: () => { clicked.push(icon); } });
  const spans = spanLabels.flatMap(text => {
    const inner = { textContent: text, getAttribute: () => null, getClientRects: () => [1], click: () => { clicked.push(text); } };
    const outer = { ...inner, child: inner };
    return [outer, inner];
  });
  for (const button of [...buttons, ...spans]) {
    button.scrollIntoView = () => {};
    button.getBoundingClientRect = () => ({ left: 10, right: 100, top: 10, bottom: 40 });
    button.contains = node => node === button.child || (node === hit && !covered);
  }
  const window = { innerWidth: 1100, innerHeight: 700 }; window.top = child ? {} : window;
  const document = {
    querySelector: () => auth ? {} : null,
    elementFromPoint: () => hit,
    querySelectorAll: selector => selector === 'span' ? spans : selector.startsWith('[role="dialog"]') ? (dialogs ? [{ getClientRects: () => [1] }] : []) : buttons,
  };
  const context = createContext({ window, document, location: { origin, pathname },
    getComputedStyle: () => ({ display: 'block', visibility: 'visible' }) });
  return { clicked, run: (allowCreate = true) => new Script(source.replace('/*ALLOW_CREATE*/true', String(allowCreate))).runInContext(context) };
}
test('opens Create followed by Live video without pressing Go live', () => {
  for (const [create, live] of [['Create', 'Live video'], ['Oluştur', 'Canlı video']]) {
    const home = page({ labels: [create] });
    assert.equal(home.run(), 'menu'); assert.deepEqual(home.clicked, [create]);
    const menu = page({ labels: [live, 'Go live', 'End live video'], dialogs: true });
    assert.equal(menu.run(false), 'opened'); assert.deepEqual(menu.clicked, [live]);
  }
});
test('collapsed sidebar uses the labelled Create icon and never toggles it twice', () => {
  const fixture = page({ icon: 'New post' });
  assert.equal(fixture.run(), 'menu'); assert.deepEqual(fixture.clicked, ['New post']);
  assert.equal(fixture.run(false), 'unavailable'); assert.equal(fixture.clicked.length, 1);
});
test('desktop Live video submenu supports nested plain spans with delegated click handlers', () => {
  const fixture = page({ spanLabels: ['Canlı video', 'Gönderi'], labels: ['Oluştur'] });
  assert.equal(fixture.run(false), 'opened');
  assert.deepEqual(fixture.clicked, ['Canlı video']);
});
test('existing Producer dialogs and missing or ambiguous controls are left untouched', () => {
  for (const config of [{ labels: ['Create'], covered: true }, { labels: ['Create', 'Go live', 'Practice', 'Public', 'Next'] },
    { labels: ['Create', 'Create'] }, { labels: ['Live video', 'Live video'] }]) {
    const fixture = page(config); fixture.run(); assert.deepEqual(fixture.clicked, []);
  }
});
test('desktop shell role=dialog does not block an uncovered Create control', () => {
  const fixture = page({ labels: ['Create'], dialogs: true });
  assert.equal(fixture.run(), 'menu'); assert.deepEqual(fixture.clicked, ['Create']);
});
test('authentication, other origins and child frames never click controls', () => {
  for (const config of [{ auth: true }, { origin: 'https://evil.example' }, { pathname: '/accounts/login/' },
    { pathname: '/checkpoint/' }, { child: true }]) {
    const fixture = page({ ...config, labels: ['Create', 'Live video'] });
    assert.equal(fixture.run(), 'blocked'); assert.deepEqual(fixture.clicked, []);
  }
});
