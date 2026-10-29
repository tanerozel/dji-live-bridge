(() => {
  'use strict';
  // Only an explicit app End request can commit either step. Never start, share or delete.
  if (window.top !== window || location.origin !== 'https://www.instagram.com' ||
      /^\/(accounts|auth_platform|challenge|checkpoint)(\/|$)/.test(location.pathname) ||
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

  const text = n => (n.getAttribute('aria-label') || n.textContent || '').trim().replace(/\s+/g, ' ').toLowerCase().replace(/\u0307/g, '');
  const end = /^(end live video|end live|end broadcast|canlı videoyu bitir|canlı videonu bitir|canlı videoyu sonlandır|canlı videonu sonlandır|canlı yayını bitir|canlı yayını sonlandır|yayını bitir|yayını sonlandır|finalizar video en vivo|finalizar vídeo en vivo|terminer la vidéo en direct|live-video beenden|encerrar vídeo ao vivo|termina video in diretta|завершить прямой эфир|إنهاء البث المباشر|लाइव वीडियो समाप्त करें|akhiri video siaran langsung|ライブ動画を終了|라이브 방송 종료|结束直播|結束直播)$/i;
  const question = /^(end live video|end live|end broadcast|canlı videoyu bitir|canlı videonu bitir|canlı videoyu sonlandır|canlı videonu sonlandır|canlı yayını bitir|canlı yayını sonlandır|yayını bitir|yayını sonlandır|finalizar video en vivo|finalizar vídeo en vivo|terminer la vidéo en direct|live-video beenden|encerrar vídeo ao vivo|termina video in diretta|завершить прямой эфир|إنهاء البث المباشر|लाइव वीडियो समाप्त करें|akhiri video siaran langsung|ライブ動画を終了|라이브 방송 종료|结束直播|結束直播)\s*\??$/i;
  const isEndQuestion = value => question.test(value) ||
    /^(canlı (videoyu|videonu|yayını) (bitirmek|sonlandırmak) istediğinden emin misin|are you sure you want to end (your |the )?live video)\s*\??$/i.test(value);
  const ended = /^(live video ended|your live video has ended|your live has ended|broadcast ended|canlı video sona erdi|canlı videon sona erdi|canlı videon sonlandı|canlı video sonlandı|canlı yayın sona erdi|canlı yayın bitti|canlı videon bitti|canlı video bitti|video en vivo finalizado|la vidéo en direct est terminée|live-video beendet|vídeo ao vivo encerrado|video in diretta terminato|прямой эфир завершён|انتهى البث المباشر|लाइव वीडियो समाप्त हो गया|video siaran langsung berakhir|ライブ動画が終了しました|라이브 방송이 종료되었습니다|直播已结束|直播已結束)[.!]?$/i;
  const cancel = /^(cancel|not now|iptal|iptal et|vazgeç|cancelar|annuler|abbrechen|annulla|отмена|إلغاء|रद्द करें|batal|キャンセル|취소|取消)$/i;
  const genericEnd = /^(end|end now|bitir|sonlandır|finalizar|terminer|beenden|encerrar|termina|завершить|إنهاء|समाप्त करें|akhiri|終了|종료|结束|結束)$/i;
  const start = /^(go live|start live video|canlı yayına geç|canlı yayını başlat|canlı yayın başlat)$/i;
  const visible = n => n.getClientRects().length > 0 && getComputedStyle(n).visibility !== 'hidden' && getComputedStyle(n).display !== 'none';
  const enabled = n => !n.disabled && n.getAttribute('aria-disabled') !== 'true' && !n.closest('[inert], [aria-hidden="true"]');
  const reachable = n => {
    n.scrollIntoView({ block: 'center', inline: 'center', behavior: 'instant' });
    const r = n.getBoundingClientRect();
    if (r.width <= 0 || r.height <= 0) return false;
    const hit = document.elementFromPoint(r.left + r.width / 2, r.top + r.height / 2);
    return !!hit && (hit === n || n.contains(hit));
  };
  const allButtons = [...document.querySelectorAll('button, [role="button"]')].filter(visible);
  const buttons = allButtons.filter(enabled);
  const dialogs = [...document.querySelectorAll('[role="dialog"], [role="alertdialog"]')].filter(visible);
  const confirmations = dialogs.flatMap(dialog => {
    const headings = [...dialog.querySelectorAll('[role="heading"], h1, h2, h3')].filter(visible);
    if (!headings.some(n => isEndQuestion(text(n)))) return [];
    const own = buttons.filter(n => n.closest('[role="dialog"], [role="alertdialog"]') === dialog);
    if (own.filter(n => cancel.test(text(n))).filter(reachable).length !== 1) return [];
    return own.filter(n => end.test(text(n)) || genericEnd.test(text(n))).filter(reachable);
  });
  const ends = buttons.filter(n => end.test(text(n))).filter(reachable);
  const headings = [...document.querySelectorAll('[role="heading"], h1, h2, h3')].filter(visible);
  let step;
  let button;
  if (confirmations.length === 1) { step = 'confirm'; button = confirmations[0]; }
  else if (confirmations.length > 1) return 'waiting';
  else if (ends.length === 1) { step = 'end'; button = ends[0]; }
  else if (ends.length > 1) return 'waiting';
  else if (headings.some(n => ended.test(text(n)) && reachable(n))) return 'ended';
  // A visible Go live control is positive evidence of a draft, never a control to click here.
  else if (allButtons.filter(n => start.test(text(n))).filter(reachable).length === 1) return 'idle';
  else return 'waiting';
  const action = /*ACTION*/'';
  if (!action) return step;
  if (Date.now() >= /*DEADLINE*/0) return 'expired';
  if (action !== step || !enabled(button) || !reachable(button)) return 'waiting';
  const token = /*TOKEN*/'';
  if (!token) return 'blocked';
  const key = Symbol.for('streammydrone.instagram.endRequests');
  const dispatched = window[key] || (window[key] = new Set());
  const claim = token + ':' + step;
  if (dispatched.has(claim)) return 'dispatched';
  dispatched.add(claim);
  button.click();
  return 'clicked';
})();
