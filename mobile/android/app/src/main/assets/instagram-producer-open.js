(() => {
  'use strict';
  // Open setup only. Never submit details, choose an audience, publish or end a broadcast.
  if (window.top !== window || location.origin !== 'https://www.instagram.com' ||
      /^\/(accounts|challenge|checkpoint)(\/|$)/.test(location.pathname)) return 'blocked';
  if (document.querySelector('input[type="password"], input[name="username"], input[autocomplete="one-time-code"]')) return 'blocked';
  const visible = node => node.getClientRects().length > 0 &&
    getComputedStyle(node).display !== 'none' && getComputedStyle(node).visibility !== 'hidden';
  const label = node => String(node.getAttribute('aria-label') || node.textContent || '').trim().replace(/\s+/g, ' ');
  const controls = Array.from(document.querySelectorAll('button, [role="button"], [role="menuitem"], a')).filter(node =>
    visible(node) && !node.disabled && node.getAttribute('aria-disabled') !== 'true');
  const live = /^(live video|canlı video|video en vivo|vídeo en directo|vidéo en direct|live-video|video in diretta|vídeo ao vivo|прямой эфир|فيديو مباشر|लाइव वीडियो|video siaran langsung|ライブ動画|라이브 방송|直播视频|直播影片)$/i;
  const create = /^(create|oluştur|crear|créer|erstellen|crea|criar|создать|إنشاء|बनाएँ|buat|作成|만들기|创建|建立)$/i;
  const createIcon = /^(new post|yeni gönderi|nouvelle publication|nueva publicación|nuovo post|neuer beitrag|nova publicação|новая публикация|منشور جديد|नई पोस्ट|postingan baru|新規投稿|새 게시물|新建帖子|新增貼文)$/i;
  const allowCreate = /*ALLOW_CREATE*/true;
  const unique = candidates => candidates.length === 1 ? candidates[0] : null;
  const click = node => {
    node.scrollIntoView({ block: 'center', inline: 'center', behavior: 'instant' });
    const rect = node.getBoundingClientRect();
    const x = (Math.max(0, rect.left) + Math.min(window.innerWidth, rect.right)) / 2;
    const y = (Math.max(0, rect.top) + Math.min(window.innerHeight, rect.bottom)) / 2;
    const top = document.elementFromPoint(x, y);
    // Programmatic click must not bypass a cookie/login/details overlay covering the control.
    if (!Number.isFinite(x) || !Number.isFinite(y) || !top || (top !== node && !node.contains(top))) return false;
    node.click();
    return true;
  };
  // Prefer an already-open Create menu. Exact labels exclude "Go live" and "End live video".
  // Instagram's desktop submenu also uses plain nested spans with delegated click handlers.
  // Use only the deepest exact Live video label; broad ancestor text can include both menu items.
  const liveSpans = Array.from(document.querySelectorAll('span')).filter(node => visible(node) && live.test(label(node)));
  const leafSpans = liveSpans.filter(node => !liveSpans.some(other => other !== node && node.contains(other)));
  const semanticEntries = controls.filter(node => live.test(label(node)));
  const entries = semanticEntries.length ? semanticEntries : leafSpans;
  const entry = unique(entries);
  if (entry) return click(entry) ? 'opened' : 'covered';
  if (entries.length > 1) return 'ambiguous';
  // Instagram can mark the entire desktop shell as a dialog. A generic role=dialog test
  // therefore blocks the home page too. Preserve a visible Producer explicitly; hit testing
  // below protects other foreground dialogs without treating the shell as an active modal.
  if (controls.some(node => /^(go live|canlı yayına geç|canlı yayını başlat|end live video|canlı videoyu bitir)$/i.test(label(node)))) return 'waiting';
  if (!allowCreate) return 'unavailable';
  const candidates = controls.filter(node => create.test(label(node)) || createIcon.test(label(node)) ||
    Array.from(node.querySelectorAll('svg[aria-label]')).some(icon => create.test(label(icon)) || createIcon.test(label(icon))));
  const trigger = unique(candidates);
  if (!trigger) return candidates.length > 1 ? 'ambiguous' : 'unavailable';
  return click(trigger) ? 'menu' : 'covered';
})();
