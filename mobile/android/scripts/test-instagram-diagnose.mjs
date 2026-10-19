import { readFileSync } from 'node:fs';
import vm from 'node:vm';
import test from 'node:test';
import assert from 'node:assert/strict';
const script = readFileSync(new URL('../app/src/main/assets/instagram-producer-diagnose.js', import.meta.url), 'utf8');
test('diagnostics return fixed structural vocabulary without values or surrounding content', () => {
  const parent = { tagName: 'DIV', getAttribute: () => 'button' };
  const nodes = ['Public', 'Practice', 'Next', 'private account title'].map(text => ({
    tagName: 'SPAN', parentElement: parent, getClientRects: () => [1], getAttribute: () => null,
    get textContent() { return text; }, get value() { throw Error('Never read a field value'); },
  }));
  const window = {}; window.top = window;
  const output = vm.runInNewContext(script, { window, location: { origin: 'https://www.instagram.com', pathname: '/' },
    getComputedStyle: () => ({}), document: { querySelectorAll: () => nodes } });
  assert.equal(output, 'public:span:none:div:button:enabled,practice:span:none:div:button:enabled,next:span:none:div:button:enabled');
});
test('authentication and other origins cannot be inspected', () => {
  for (const location of [{ origin: 'https://www.instagram.com', pathname: '/accounts/login/' }, { origin: 'https://evil.example', pathname: '/' }]) {
    const window = {}; window.top = window;
    assert.equal(vm.runInNewContext(script, { window, location, document: { querySelectorAll() { throw Error('Blocked'); } } }), 'blocked');
  }
});
