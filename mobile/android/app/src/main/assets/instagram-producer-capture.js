(() => {
  'use strict';
  const pending = () => JSON.stringify({ status: 'waiting' });
  // Never inspect a login, checkpoint, 2FA page, another origin or a child frame.
  if (window.top !== window || location.origin !== 'https://www.instagram.com' ||
      /^\/(accounts|challenge|checkpoint)(\/|$)/.test(location.pathname)) return pending();
  if (document.querySelector('input[name="password"], input[autocomplete="current-password"], input[autocomplete="one-time-code"]')) return pending();
  const visible = node => node.getClientRects().length > 0 && getComputedStyle(node).visibility !== 'hidden' && getComputedStyle(node).display !== 'none';
  const directLabel = node => [node.getAttribute('aria-label'), node.getAttribute('placeholder'), node.getAttribute('name'),
    ...Array.from(node.labels || []).map(item => item.textContent)].filter(Boolean).join(' ');
  const fieldSelector = 'input, textarea, [role="textbox"], code, pre';
  // Instagram places the visible caption beside a wrapping LABEL, one level above the input.
  // Stop before a shared server/key container: its caption must not label another field.
  const label = node => {
    const labels = [directLabel(node)];
    let parent = node.parentElement;
    for (let depth = 0; parent && depth < 3; depth++, parent = parent.parentElement) {
      if (parent.querySelectorAll(fieldSelector).length !== 1) break;
      labels.push((parent.textContent || '').slice(0, 500));
    }
    return labels.join(' ');
  };
  const keyLabel = /stream\s*key|live\s*key|yay[ıi]n\s*anahtar|clé.*diffusion|schlüssel|chave.*transmiss|chiave.*stream|ключ.*транс|مفتاح.*بث|स्ट्रीम.*कुंजी|kunci.*stream|ストリーム.*キー|스트림.*키|串流.*[密金]|直播.*密/i;
  // A homepage login overlay may not change the URL or name its password input. Refuse it
  // before reading any field value; only explicitly labelled stream-key password fields qualify.
  if (Array.from(document.querySelectorAll('input[type="password"]')).some(node => !keyLabel.test(label(node)))) return pending();
  const fields = Array.from(document.querySelectorAll(fieldSelector)).filter(node => {
    if (!visible(node)) return false;
    const type = (node.getAttribute('type') || 'text').toLowerCase();
    if (['hidden', 'email', 'search', 'tel', 'number'].includes(type)) return false;
    if (type === 'password' && !keyLabel.test(label(node))) return false;
    return !/username|current-password|one-time-code/i.test(node.getAttribute('autocomplete') || '');
  });
  const value = node => String(node.value === undefined ? node.textContent : node.value).trim();
  const isServer = text => {
    try {
      const uri = new URL(text);
      return uri.protocol === 'rtmps:' && !uri.username && !uri.password && !uri.search && !uri.hash &&
        /^\/rtmp\/?$/.test(uri.pathname) && /(^|\.)(instagram\.com|fbcdn\.net)$/.test(uri.hostname);
    } catch (_) { return false; }
  };
  const servers = fields.map(node => ({ node, text: value(node) })).filter(item => isServer(item.text));
  if (servers.length !== 1) return pending();
  const server = servers[0];
  const candidates = fields.filter(node => node !== server.node).map(node => ({ node, text: value(node), named: keyLabel.test(label(node)) }))
    .filter(item => item.text.length >= 4 && item.text.length <= 512 && !/[\s\/#*\u0080-\uffff]/.test(item.text) && /[a-z0-9]/i.test(item.text))
    .filter(item => item.named && (keyLabel.test(directLabel(item.node)) || item.node.readOnly === true ||
      item.node.getAttribute('aria-readonly') === 'true' || item.node.getAttribute('type') === 'password' || ['CODE', 'PRE'].includes(item.node.tagName)));
  const named = candidates.filter(item => item.named);
  const keys = named.length ? named : candidates;
  if (keys.length !== 1) return pending();
  const title = window[Symbol.for('streammydrone.instagram.titleRecord')];
  const titleMatches = !!title && title.revision === /*TITLE_REVISION*/'' && title.actual === title.desired;
  const audience = window[Symbol.for('streammydrone.instagram.audienceRecord')];
  const audienceMatches = !!audience && audience.revision === /*TITLE_REVISION*/'' && audience.audience === /*AUDIENCE*/'PRACTICE';
  return JSON.stringify({ status: 'ready', origin: location.origin, serverUrl: server.text, streamKey: keys[0].text, titleMatches, audienceMatches });
})();
