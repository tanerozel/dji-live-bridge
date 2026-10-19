import { readFileSync } from 'node:fs';
import { Script, createContext } from 'node:vm';
import test from 'node:test';
import assert from 'node:assert/strict';

const source = readFileSync(new URL('../app/src/main/assets/instagram-producer-title.js', import.meta.url), 'utf8');
function fixture({ values = [''], auth = false, heading = 'Canlı video', origin = 'https://www.instagram.com', pathname = '/', child = false, maxLength = -1 } = {}) {
  class Input {
    constructor(value) { this.raw = value; this.events = []; this.listeners = {}; this.reads = 0; this.writes = 0; this.tagName = 'INPUT'; this.maxLength = maxLength; }
    get value() { this.reads++; return this.raw; }
    set value(next) { this.writes++; this.raw = next; }
    getAttribute(name) { return name === 'type' ? 'text' : null; }
    getClientRects() { return [1]; }
    addEventListener(type, handler) { (this.listeners[type] ||= []).push(handler); }
    dispatchEvent(event) { this.events.push(event.type); for (const handler of this.listeners[event.type] || []) handler(); }
  }
  const fields = values.map(value => new Input(value));
  // A framework-owned setter must not intercept the native setter/event path.
  for (const field of fields) Object.defineProperty(field, 'value', {
    get() { return Object.getOwnPropertyDescriptor(Input.prototype, 'value').get.call(this); },
    set() { throw Error('Use the input native prototype setter'); },
  });
  const dialog = { getClientRects: () => [1], querySelectorAll: selector =>
    selector.startsWith('[role="heading"]') ? [{ textContent: heading }] : fields };
  const window = {}; window.top = child ? {} : window;
  const context = createContext({ window, location: { origin, pathname },
    document: { querySelector: () => auth ? {} : null, querySelectorAll: () => [dialog] },
    getComputedStyle: () => ({ display: 'block', visibility: 'visible' }),
    HTMLInputElement: Input, HTMLTextAreaElement: Input, Event: class { constructor(type) { this.type = type; } },
  });
  return { fields, record: () => window[Symbol.for('streammydrone.instagram.titleRecord')], run: (title = 'Stream My Drone', revision = 'default', force = false) => new Script(source.replace("/*TITLE*/'Stream My Drone'", JSON.stringify(title)).replace("/*REVISION*/'default'", JSON.stringify(revision)).replace('/*FORCE*/false', String(force))).runInContext(context) };
}
test('fills only the empty Live video title and dispatches input/change without submitting', () => {
  const page = fixture();
  assert.equal(page.run(), 'filled');
  assert.equal(page.fields[0].raw, 'Stream My Drone');
  assert.deepEqual(page.fields[0].events, ['input', 'change']);
  assert.equal(page.fields[0].writes, 1);
});
test('existing user title and deliberately cleared autofill remain untouched on repeated polls', () => {
  const existing = fixture({ values: ['My chosen title'] });
  assert.equal(existing.run(), 'unchanged'); assert.equal(existing.fields[0].writes, 0);
  const page = fixture(); page.run();
  page.fields[0].raw = '';
  assert.equal(page.run(), 'unchanged'); assert.equal(page.fields[0].raw, '');
  assert.equal(page.fields[0].writes, 1);
});
test('login challenges other origins and child frames do not read or write a field', () => {
  for (const config of [{ auth: true }, { pathname: '/accounts/login/' }, { pathname: '/challenge/' },
    { origin: 'https://evil.example' }, { child: true }]) {
    const page = fixture(config); assert.equal(page.run(), 'blocked');
    assert.equal(page.fields[0].reads, 0); assert.equal(page.fields[0].writes, 0);
  }
});
test('unrelated dialog and ambiguous unnamed fields are never modified', () => {
  for (const config of [{ heading: 'Edit profile' }, { values: ['', ''] }]) {
    const page = fixture(config); page.run();
    assert.ok(page.fields.every(field => field.reads === 0 && field.writes === 0));
  }
});

test('a confirmed common title replaces an old setup value once and a new request can replace it again', () => {
  const page = fixture({ values: ['Previous title'] });
  assert.equal(page.run('New shared title', 'start-1', true), 'filled');
  assert.equal(page.fields[0].raw, 'New shared title');
  page.fields[0].raw = 'User page edit';
  assert.equal(page.run('New shared title', 'start-1', true), 'unchanged');
  assert.equal(page.fields[0].raw, 'User page edit');
  assert.equal(page.run('Next flight', 'start-2', true), 'filled');
  assert.equal(page.fields[0].raw, 'Next flight');
});
test('platform maxlength is enforced without truncating or keeping an incorrect old title', () => {
  const page = fixture({ values: ['old'], maxLength: 8 });
  assert.equal(page.run('Long new title', 'start-1', true), 'too-long');
  assert.equal(page.fields[0].raw, 'old'); assert.equal(page.fields[0].writes, 0);
  assert.equal(page.run('🚁'.repeat(5), 'start-2', true), 'too-long');
});
test('quotes and backslashes in the chosen title remain plain field text', () => {
  const page = fixture(); const title = `İstanbul 'flight' \\ 2026`;
  assert.equal(page.run(title, 'start-1', true), 'filled');
  assert.equal(page.fields[0].raw, title);
});

test('title proof keeps user edits and survives React reusing the input as a readonly credential field', () => {
  const page = fixture(); page.run('Shared title', 'start-1', true);
  page.fields[0].raw = 'Edited title'; page.fields[0].dispatchEvent({ type: 'input' });
  assert.equal(page.record().actual, 'Edited title');
  page.fields[0].readOnly = true; page.fields[0].raw = 'not-a-title-anymore';
  page.fields[0].dispatchEvent({ type: 'change' });
  assert.equal(page.record().actual, 'Edited title');
});
