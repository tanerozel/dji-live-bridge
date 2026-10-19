(() => {
  'use strict';
  if (window.top !== window || location.origin !== 'https://www.instagram.com' ||
      /^\/(accounts|challenge|checkpoint)(\/|$)/.test(location.pathname)) return 'blocked';
  if (document.querySelector('input[type="password"], input[name="username"], input[autocomplete="one-time-code"]')) return 'blocked';
  const visible = node => node.getClientRects().length > 0 &&
    getComputedStyle(node).visibility !== 'hidden' && getComputedStyle(node).display !== 'none';
  const liveHeading = /^(live video|canlı video|video en vivo|vídeo en directo|vidéo en direct|live-video|video in diretta|vídeo ao vivo|прямой эфир|فيديو مباشر|लाइव वीडियो|video siaran langsung|ライブ動画|라이브 방송|直播视频|直播影片)$/i;
  const dialogs = Array.from(document.querySelectorAll('[role="dialog"]')).filter(dialog => visible(dialog) &&
    Array.from(dialog.querySelectorAll('[role="heading"], h1, h2, h3')).some(node => liveHeading.test(node.textContent.trim())));
  if (dialogs.length !== 1) return dialogs.length ? 'ambiguous' : 'waiting';
  const fields = Array.from(dialogs[0].querySelectorAll('input, textarea')).filter(node => visible(node) &&
    !node.disabled && !node.readOnly && (node.tagName === 'TEXTAREA' || (node.getAttribute('type') || 'text').toLowerCase() === 'text') &&
    !/username|email|password|one-time-code/i.test([node.getAttribute('name'), node.getAttribute('autocomplete')].join(' ')));
  const titleLabel = /title|başlık|titel|titre|título|titolo|название|عنوان|शीर्षक|judul|タイトル|제목|标题|標題/i;
  const named = fields.filter(node => titleLabel.test([node.getAttribute('aria-label'), node.getAttribute('placeholder'),
    node.getAttribute('name'), ...Array.from(node.labels || []).map(label => label.textContent)].join(' ')));
  // The observed Live video setup has one text input; never guess among multiple unnamed inputs.
  const candidates = named.length ? named : fields;
  if (candidates.length !== 1) return candidates.length ? 'ambiguous' : 'waiting';
  const field = candidates[0];
  const desired = /*TITLE*/'Stream My Drone';
  const revision = /*REVISION*/'default';
  const force = /*FORCE*/false;
  if (field.maxLength >= 0 && desired.length > field.maxLength) return 'too-long';
  const token = Symbol.for('streammydrone.instagram.handledTitleFields');
  if (!(window[token] instanceof WeakMap)) window[token] = new WeakMap();
  const handled = window[token];
  const recordKey = Symbol.for('streammydrone.instagram.titleRecord');
  if (handled.get(field) === revision) return field.value === desired ? 'matches' : 'unchanged';
  // An explicit native title choice can replace the setup value once; later page edits survive.
  const record = { actual: field.value, desired, revision };
  // Keep the last title edit, not a reference to an input React may reuse as the key field.
  const changed = () => {
    if (location.origin !== 'https://www.instagram.com' || /^\/(accounts|challenge|checkpoint)(\/|$)/.test(location.pathname) ||
        document.querySelector('input[type="password"], input[name="username"], input[autocomplete="one-time-code"]') ||
        field.readOnly || field.disabled || !['text', ''].includes((field.getAttribute('type') || '').toLowerCase()) ||
        /username|email|password|one-time-code/i.test([field.getAttribute('name'), field.getAttribute('autocomplete')].join(' '))) return;
    record.actual = field.value;
  };
  field.addEventListener('input', changed);
  field.addEventListener('change', changed);
  if (field.value === desired || (!force && field.value.trim())) {
    handled.set(field, revision);
    window[recordKey] = record;
    return field.value === desired ? 'matches' : 'unchanged';
  }
  const prototype = field.tagName === 'TEXTAREA' ? HTMLTextAreaElement.prototype : HTMLInputElement.prototype;
  const setter = Object.getOwnPropertyDescriptor(prototype, 'value')?.set;
  if (!setter) return 'waiting';
  setter.call(field, desired);
  record.actual = desired;
  field.dispatchEvent(new Event('input', { bubbles: true }));
  field.dispatchEvent(new Event('change', { bubbles: true }));
  record.actual = field.value;
  handled.set(field, revision);
  window[recordKey] = record;
  // Only a status is returned. Next, audience and publishing are separate actions.
  return 'filled';
})();
