(() => {
  'use strict';
  // Navigation aid only: never click a publishing/audience control or inspect form values.
  if (window.top !== window || location.origin !== 'https://www.instagram.com' ||
      /^\/(accounts|auth_platform|challenge|checkpoint)(\/|$)/.test(location.pathname)) return false;
  const labels = /^(go live|start live video|canlı yayına geç|canlı yayını başlat|canlı yayın başlat|iniciar video en vivo|transmitir en vivo|passer en direct|démarrer la vidéo en direct|live gehen|iniciar transmissão ao vivo|trasmetti in diretta|начать прямой эфир|بدء البث المباشر|लाइव जाएं|siarkan langsung|ライブ配信を開始|라이브 방송 시작|开始直播|開始直播)$/i;
  const visible = node => node.getClientRects().length > 0 &&
    getComputedStyle(node).visibility !== 'hidden' && getComputedStyle(node).display !== 'none';
  const controls = Array.from(document.querySelectorAll('button, [role="button"]'))
    .filter(node => visible(node) && labels.test((node.getAttribute('aria-label') || node.textContent || '').trim()));
  if (controls.length !== 1) return false;
  controls[0].scrollIntoView({ block: 'center', inline: 'center', behavior: 'instant' });
  return true;
})();
