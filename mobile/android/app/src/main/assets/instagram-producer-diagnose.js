(() => {
  'use strict';
  // Fixed structural codes only. Never read input values, page titles, credentials or cookies.
  if (window.top !== window || location.origin !== 'https://www.instagram.com' ||
      /^\/(accounts|challenge|checkpoint)(\/|$)/.test(location.pathname)) return 'blocked';
  const visible = node => node.getClientRects().length > 0 && getComputedStyle(node).display !== 'none' && getComputedStyle(node).visibility !== 'hidden';
  const words = {
    practice: /^(practice|alıştırma)$/i, public: /^(public|herkese açık)$/i,
    next: /^(next|continue|ileri|İleri|devam|devam et|sonraki)$/i, audience: /^(audience|kitle|yayın kitlesi)$/i,
  };
  const roles = new Set(['button', 'combobox', 'option', 'menuitem', 'radio', 'listbox', 'dialog']);
  const tags = new Set(['BUTTON', 'DIV', 'SPAN', 'LABEL', 'SELECT', 'INPUT', 'OPTION']);
  const nodes = Array.from(document.querySelectorAll('button, [role], span, label, select, option')).filter(visible);
  const codes = new Set();
  for (const node of nodes) {
    const text = String(node.getAttribute('aria-label') || node.innerText || node.textContent || '').trim().split(/\r?\n/)[0].trim().replace(/\s+/g, ' ');
    const kind = Object.keys(words).find(key => words[key].test(text));
    if (!kind) continue;
    const tag = tags.has(node.tagName) ? node.tagName.toLowerCase() : 'other';
    const role = roles.has(node.getAttribute('role')) ? node.getAttribute('role') : 'none';
    const parent = node.parentElement;
    const parentTag = parent && tags.has(parent.tagName) ? parent.tagName.toLowerCase() : 'other';
    const parentRole = parent && roles.has(parent.getAttribute('role')) ? parent.getAttribute('role') : 'none';
    const disabled = node.disabled || node.getAttribute('aria-disabled') === 'true';
    codes.add([kind, tag, role, parentTag, parentRole, disabled ? 'disabled' : 'enabled'].join(':'));
  }
  return Array.from(codes).slice(0, 30).join(',') || 'no-controls';
})();
