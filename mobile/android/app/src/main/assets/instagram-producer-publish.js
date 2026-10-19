(() => {
  'use strict';
  // Invoked only for an explicit app Start request. Never read form values or choose an audience.
  if (window.top !== window || location.origin !== 'https://www.instagram.com' ||
      /^\/(accounts|challenge|checkpoint)(\/|$)/.test(location.pathname) ||
      document.querySelector('input[autocomplete="current-password"], input[autocomplete="one-time-code"], input[name="username"]')) return 'blocked';
  // Live Producer can keep its readonly stream key in a password-type input.
  // That field is not a login prompt. Only its label/readonly metadata is inspected.
  const keyLabel = /stream\s*key|live\s*key|yay[ıi]n\s*anahtar|clé.*diffusion|schlüssel|chave.*transmiss|chiave.*stream|ключ.*транс|مفتاح.*بث|स्ट्रीम.*कुंजी|kunci.*stream|ストリーム.*キー|스트림.*키|串流.*[密金]|直播.*密/i;
  const keyField = node => {
    if (node.readOnly !== true && node.getAttribute('aria-readonly') !== 'true') return false;
    const labels = [node.getAttribute('aria-label'), node.getAttribute('placeholder'), node.getAttribute('name'),
      ...Array.from(node.labels || []).map(label => label.textContent)];
    let parent = node.parentElement;
    for (let depth = 0; parent && depth < 3; parent = parent.parentElement, depth++) {
      if (parent.querySelectorAll('input, textarea, [role="textbox"], code, pre').length !== 1) break;
      labels.push((parent.textContent || '').slice(0, 500));
    }
    return keyLabel.test(labels.filter(Boolean).join(' '));
  };
  if (Array.from(document.querySelectorAll('input[type="password"]')).some(node => !keyField(node))) return 'blocked';

  const start = /^(go live|start live video|canlı yayına geç|canlı yayını başlat|canlı yayın başlat|iniciar video en vivo|transmitir en vivo|passer en direct|démarrer la vidéo en direct|live gehen|iniciar transmissão ao vivo|trasmetti in diretta|начать прямой эфир|بدء البث المباشر|लाइव जाएं|siarkan langsung|ライブ配信を開始|라이브 방송 시작|开始直播|開始直播)$/i;
  const end = /^(end live video|end live|end broadcast|canlı videoyu bitir|canlı videonu bitir|canlı videoyu sonlandır|canlı videonu sonlandır|canlı yayını sonlandır|canlı yayını bitir|yayını bitir|finalizar video en vivo|finalizar vídeo en vivo|terminer la vidéo en direct|live-video beenden|encerrar vídeo ao vivo|termina video in diretta|завершить прямой эфир|إنهاء البث المباشر|लाइव वीडियो समाप्त करें|akhiri video siaran langsung|ライブ動画を終了|라이브 방송 종료|结束直播|結束直播)$/i;
  const visible = node => node.getClientRects().length > 0 &&
    getComputedStyle(node).visibility !== 'hidden' && getComputedStyle(node).display !== 'none';
  const controls = Array.from(document.querySelectorAll('button, [role="button"]')).filter(visible);
  const label = node => (node.getAttribute('aria-label') || node.textContent || '').trim().replace(/\s+/g, ' ').toLowerCase().replace(/\u0307/g, '');
  const enabled = node => !node.disabled && node.getAttribute('aria-disabled') !== 'true' && !node.closest('[inert], [aria-hidden="true"]');
  const reachable = node => {
    node.scrollIntoView({ block: 'center', inline: 'center', behavior: 'instant' });
    const rect = node.getBoundingClientRect();
    if (rect.width <= 0 || rect.height <= 0) return false;
    const hit = document.elementFromPoint(rect.left + rect.width / 2, rect.top + rect.height / 2);
    return !!hit && (hit === node || node.contains(hit));
  };
  // Producer keeps disabled/covered copies of controls in older dialog layers.
  // Uniqueness applies to controls the user can actually activate.
  const starts = controls.filter(node => start.test(label(node)) && enabled(node)).filter(reachable);
  const ends = controls.filter(node => end.test(label(node)) && enabled(node)).filter(reachable);
  if (ends.length === 1 && starts.length === 0) return 'live';
  if (starts.length !== 1 || ends.length !== 0) return 'waiting';
  const button = starts[0];
  if (!enabled(button) || !reachable(button)) return 'waiting';
  if (!/*CLICK*/false) return 'ready';
  const token = /*TOKEN*/'';
  if (!token || location.href !== /*EXPECTED_HREF*/'') return 'blocked';
  if (Date.now() >= /*DEADLINE*/0) return 'expired';
  const key = Symbol.for('streammydrone.instagram.publishRequests');
  const dispatched = window[key] || (window[key] = new Set());
  if (dispatched.has(token)) return 'dispatched';
  dispatched.add(token);
  button.click();
  return 'clicked';
})();
