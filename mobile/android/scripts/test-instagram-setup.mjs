import { readFileSync } from 'node:fs';
import { Script, createContext } from 'node:vm';
import test from 'node:test';
import assert from 'node:assert/strict';

const source = readFileSync(new URL('../app/src/main/assets/instagram-producer-setup.js', import.meta.url), 'utf8');
function fixture({ selected = 'Public', target = 'PRACTICE', titleMatches = true, auth = false, origin = 'https://www.instagram.com', pathname = '/', covered = false, duplicate = false, nativeSelect = false, unmarkedChoices = false } = {}) {
  const clicked = []; let menu = false; let nextPage = false;
  const window = { innerWidth: 1100, innerHeight: 700 }; window.top = window;
  const proofKey = Symbol.for('streammydrone.instagram.audienceRecord');
  window[Symbol.for('streammydrone.instagram.titleRecord')] = { revision: 'start-1', desired: 'Flight', actual: titleMatches ? 'Flight' : 'Other' };
  const hit = {};
  function node(text, role = 'button') {
    const item = {
      textContent: text, tagName: 'DIV',
      getAttribute: name => ({ role, ...(role === 'combobox' ? { 'aria-expanded': String(menu) } : {}),
        ...(role === 'option' ? { 'aria-selected': String(selected === text) } : {}) })[name] ?? null,
      getClientRects: () => [1], scrollIntoView: () => {},
      getBoundingClientRect: () => ({ left: 10, right: 100, top: 10, bottom: 40 }),
      contains: other => other === hit && !covered,
      click: () => {
        clicked.push(text);
        if (text === 'Next') { nextPage = true; return; }
        if (role === 'combobox') menu = true;
        else if (role === 'option') { selected = text; menu = false; }
      },
    };
    return item;
  }
  class Select {
    constructor() { this.tagName = 'SELECT'; this.options = ['Public', 'Practice'].map(text => ({ textContent: text, value: text, getAttribute: () => null })); this.events = []; }
    get value() { return selected; } set value(value) { selected = value; }
    getAttribute() { return null; } getClientRects() { return [1]; }
    scrollIntoView() {} getBoundingClientRect() { return { left: 10, right: 100, top: 10, bottom: 40 }; }
    contains(other) { return other === hit && !covered; }
    dispatchEvent(event) { this.events.push(event.type); }
  }
  const select = new Select();
  const dialog = {
    getClientRects: () => [1],
    querySelectorAll: selector => selector === 'span' ? [] : selector.startsWith('[role="heading"]') ? [{ textContent: 'Live video', getAttribute: () => null }] :
      nextPage ? [] : [...(unmarkedChoices ? [node('Public'), node('Practice')] : [nativeSelect ? select : node(selected, 'combobox')]), node('Next'), ...(duplicate ? [node('Next')] : []), node('Go live'), node('End live video')],
  };
  const document = {
    querySelector: () => auth ? {} : null,
    elementFromPoint: () => hit,
    querySelectorAll: selector => selector === '[role="dialog"]' ? [dialog] : ['input[type="password"]', 'span'].includes(selector) ? [] :
      menu ? [node('Public', 'option'), node('Practice', 'option')] : [],
  };
  const context = createContext({ window, document, location: { origin, pathname }, HTMLSelectElement: Select,
    Event: class { constructor(type) { this.type = type; } }, getComputedStyle: () => ({ display: 'block', visibility: 'visible' }) });
  const code = source.replace("/*REVISION*/''", JSON.stringify('start-1')).replace("/*AUDIENCE*/'PRACTICE'", JSON.stringify(target));
  return { clicked, select, proof: () => window[proofKey], run: () => new Script(code).runInContext(context),
    corruptProof: () => { window[Symbol.for('streammydrone.instagram.titleRecord')].revision = 'old'; } };
}

test('prepares the confirmed Practice audience and presses Next exactly once without publishing', () => {
  const page = fixture();
  assert.equal(page.run(), 'audience-opened');
  assert.equal(page.run(), 'selected');
  assert.equal(page.run(), 'next');
  assert.deepEqual(page.clicked, ['Public', 'Practice', 'Next']);
  assert.equal(page.proof().audience, 'PRACTICE');
  assert.equal(page.proof().revision, 'start-1');
  page.run(); page.run();
  assert.deepEqual(page.clicked, ['Public', 'Practice', 'Next']);
});
test('an explicit Public selection is applied instead of silently keeping Practice', () => {
  const page = fixture({ selected: 'Practice', target: 'PUBLIC' });
  page.run(); page.run(); assert.equal(page.run(), 'next');
  assert.deepEqual(page.clicked, ['Practice', 'Public', 'Next']);
  assert.equal(page.proof().audience, 'PUBLIC');
});
test('matching selected audience goes directly to Next', () => {
  const page = fixture({ selected: 'Practice' });
  assert.equal(page.run(), 'next'); assert.deepEqual(page.clicked, ['Next']);
});
test('native selects use their setter and change events then verify the selected value', () => {
  const page = fixture({ nativeSelect: true });
  assert.equal(page.run(), 'selected'); assert.deepEqual(page.select.events, ['input', 'change']);
  assert.equal(page.run(), 'next'); assert.deepEqual(page.clicked, ['Next']);
});
test('auth challenges and origins never click or disclose page content', () => {
  for (const config of [{ auth: true }, { pathname: '/accounts/login/' }, { pathname: '/challenge/' }, { origin: 'https://evil.example' }]) {
    const page = fixture(config); assert.ok(['blocked', 'auth'].includes(page.run()));
    assert.deepEqual(page.clicked, []); assert.equal(page.proof(), undefined);
  }
});
test('title mismatch or stale revision prevents preparation', () => {
  const mismatch = fixture({ titleMatches: false }); assert.equal(mismatch.run(), 'waiting'); assert.deepEqual(mismatch.clicked, []);
  const stale = fixture(); stale.corruptProof(); assert.equal(stale.run(), 'waiting'); assert.deepEqual(stale.clicked, []);
});
test('covered or ambiguous controls never advance or claim audience proof', () => {
  for (const config of [{ covered: true }, { covered: true, nativeSelect: true }, { duplicate: true, selected: 'Practice' }]) {
    const page = fixture(config); assert.ok(['covered', 'ambiguous'].includes(page.run()));
    assert.deepEqual(page.clicked, []); assert.equal(page.proof(), undefined);
  }
});
test('invalid native audience cannot create a public draft', () => {
  const page = fixture({ target: 'UNKNOWN' }); assert.equal(page.run(), 'blocked');
  assert.deepEqual(page.clicked, []); assert.equal(page.proof(), undefined);
});

test('two unmarked audience buttons cannot be mistaken for a verified selected audience', () => {
  const page = fixture({ unmarkedChoices: true });
  assert.equal(page.run(), 'ambiguous'); assert.deepEqual(page.clicked, []);
  assert.equal(page.proof(), undefined);
});

function plainPicker({ covered = false, disabledNext = false, ignoresSelection = false, nextLabel = 'Next', initialAudience = 'Public', target = 'PRACTICE', portalButtons = false } = {}) {
  let audience = initialAudience, menu = false, advanced = false;
  const clicked = [], hit = {};
  function span(text, action, { nested = false, disabled = false } = {}) {
    const parent = { tagName: 'DIV', disabled, getAttribute: () => null };
    const inner = { textContent: text, parentElement: parent, getAttribute: () => null, getClientRects: () => [1],
      scrollIntoView: () => {}, getBoundingClientRect: () => ({ left: 10, right: 100, top: 10, bottom: 40 }),
      contains: node => node === hit && !covered,
      click: () => { clicked.push(text); action(); } };
    if (!nested) return [inner];
    return [{ ...inner, contains: node => node === inner || (node === hit && !covered) }, inner];
  }
  const buttonSpans = () => advanced ? [] : span(audience, () => { menu = true; }, { nested: true });
  const nextSpans = () => advanced ? [] : span(nextLabel, () => { advanced = true; }, { nested: true, disabled: disabledNext });
  const dialog = { getClientRects: () => [1], querySelectorAll: selector => selector.startsWith('[role="heading"]') ?
    [{ textContent: 'Live video', getAttribute: () => null }] : selector === 'span' ? [...buttonSpans(), ...nextSpans()] : [] };
  const window = { innerWidth: 1100, innerHeight: 700 }; window.top = window;
  window[Symbol.for('streammydrone.instagram.titleRecord')] = { revision: 'native-1', desired: 'Flight', actual: 'Flight' };
  const nativeButtons = [];
  const menuGroup = { parentElement: null, getAttribute: () => null, contains: node => nativeButtons.includes(node), querySelectorAll: () => nativeButtons };
  for (const text of ['Public', 'Practice']) {
    const button = span(text, () => { if (!ignoresSelection) { audience = text; menu = false; } })[0];
    button.tagName = 'BUTTON'; button.parentElement = menuGroup; nativeButtons.push(button);
  }
  const context = createContext({ window, location: { origin: 'https://www.instagram.com', pathname: '/' },
    getComputedStyle: () => ({ display: 'block', visibility: 'visible' }),
    document: { querySelector: () => null, elementFromPoint: () => hit, querySelectorAll: selector =>
      selector === '[role="dialog"]' ? [dialog] : selector === 'button' ? (menu && portalButtons ? nativeButtons : []) : selector === 'span' ? [...buttonSpans(), ...nextSpans(), ...(menu && !portalButtons ?
        ['Public', 'Practice'].flatMap(text => span(text, () => { if (!ignoresSelection) { audience = text; menu = false; } }, { nested: true })) : [])] : [] },
  });
  const code = source.replace("/*REVISION*/''", JSON.stringify('native-1')).replace("/*AUDIENCE*/'PRACTICE'", JSON.stringify(target));
  return { clicked, run: () => new Script(code).runInContext(context), proof: () => window[Symbol.for('streammydrone.instagram.audienceRecord')] };
}
test('real-device unlabelled DIV and nested plain-span picker can select Practice and advance', () => {
  const page = plainPicker();
  assert.equal(page.run(), 'audience-opened'); assert.equal(page.run(), 'selected'); assert.equal(page.run(), 'next');
  assert.deepEqual(page.clicked, ['Public', 'Practice', 'Next']);
  page.run(); assert.deepEqual(page.clicked, ['Public', 'Practice', 'Next']);
  assert.equal(page.proof().audience, 'PRACTICE');
});
test('delegated plain controls respect disabled ancestors and overlays', () => {
  const covered = plainPicker({ covered: true }); assert.equal(covered.run(), 'covered'); assert.deepEqual(covered.clicked, []);
  const disabled = plainPicker({ disabledNext: true }); disabled.run(); disabled.run();
  assert.equal(disabled.run(), 'waiting'); assert.deepEqual(disabled.clicked, ['Public', 'Practice']); assert.equal(disabled.proof(), undefined);
});
test('a plain option click must collapse the picker and show the chosen audience before Next', () => {
  const page = plainPicker({ ignoresSelection: true }); page.run(); page.run(); page.run(); page.run();
  assert.deepEqual(page.clicked, ['Public', 'Practice']); assert.equal(page.proof(), undefined);
});
test('the Turkish dotted-capital İleri draft control is supported', () => {
  const page = plainPicker({ nextLabel: 'İleri' }); page.run(); page.run(); assert.equal(page.run(), 'next');
  assert.deepEqual(page.clicked, ['Public', 'Practice', 'İleri']);
});

test('explicit Public is applied and verified through the same plain-span desktop picker', () => {
  const page = plainPicker({ initialAudience: 'Practice', target: 'PUBLIC' });
  assert.equal(page.run(), 'audience-opened'); assert.equal(page.run(), 'selected'); assert.equal(page.run(), 'next');
  assert.deepEqual(page.clicked, ['Practice', 'Public', 'Next']); assert.equal(page.proof().audience, 'PUBLIC');
});

test('real audience portal uses roleless Public and Practice BUTTONs outside the title dialog', () => {
  const page = plainPicker({ portalButtons: true });
  assert.equal(page.run(), 'audience-opened'); assert.equal(page.run(), 'selected'); assert.equal(page.run(), 'next');
  assert.deepEqual(page.clicked, ['Public', 'Practice', 'Next']); assert.equal(page.proof().audience, 'PRACTICE');
});
