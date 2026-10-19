(() => {
  'use strict';
  // Native confirmation supplies title/audience. This helper only prepares the draft;
  // Go live and End are handled by separate, explicitly armed requests.
  if (window.top !== window || location.origin !== 'https://www.instagram.com') return 'blocked';
  // A signed-out start page has no form, only links to sign in or sign up: the user signs in.
  if (/^\/(accounts|challenge|checkpoint)(\/|$)/.test(location.pathname) ||
      document.querySelector('input[name="username"], input[autocomplete="one-time-code"], input[autocomplete="current-password"], ' +
        'a[href^="/accounts/login"], a[href^="https://www.instagram.com/accounts/login"], a[href^="/accounts/emailsignup"]')) return 'auth';
  const visible = node => node.getClientRects().length > 0 && getComputedStyle(node).display !== 'none' && getComputedStyle(node).visibility !== 'hidden';
  const label = node => String(node.getAttribute('aria-label') || node.textContent || '').trim().replace(/\s+/g, ' ');
  const liveHeading = /^(live video|canlı video|video en vivo|vídeo en directo|vidéo en direct|live-video|video in diretta|vídeo ao vivo|прямой эфир|فيديو مباشر|लाइव वीडियो|video siaran langsung|ライブ動画|라이브 방송|直播视频|直播影片)$/i;
  const dialogs = Array.from(document.querySelectorAll('[role="dialog"]')).filter(node => visible(node) &&
    Array.from(node.querySelectorAll('[role="heading"], h1, h2, h3')).some(heading => liveHeading.test(label(heading))));
  // Unnamed password overlays must never be automated. Producer key inputs are readonly.
  if (Array.from(document.querySelectorAll('input[type="password"]')).some(node => !node.readOnly)) return 'auth';
  if (dialogs.length !== 1) return dialogs.length ? 'ambiguous' : 'waiting';
  const dialog = dialogs[0];
  const revision = /*REVISION*/'';
  const audience = /*AUDIENCE*/'PRACTICE';
  if (!['PRACTICE', 'PUBLIC'].includes(audience) || !revision) return 'blocked';
  const title = window[Symbol.for('streammydrone.instagram.titleRecord')];
  if (!title || title.revision !== revision || title.actual !== title.desired) return 'waiting';
  const patterns = {
    PRACTICE: /^(practice|alıştırma|práctica|pratique|entraînement|übung|prova|prática|практика|تدريب|अभ्यास|latihan|練習|연습|练习|練習模式)$/i,
    PUBLIC: /^(public|herkese açık|público|publique|öffentlich|pubblico|pública|общедоступная|для всех|عام|सार्वजनिक|publik|公開|공개|公开|公開模式)$/i,
  };
  const wanted = patterns[audience];
  const anyAudience = text => Object.values(patterns).some(pattern => pattern.test(text));
  const enabled = node => {
    if (!visible(node)) return false;
    // Delegated plain-span controls inherit disabled state from their wrapping button.
    for (let parent = node, depth = 0; parent && depth < 15; parent = parent.parentElement, depth++) {
      if (parent.disabled || parent.getAttribute('aria-disabled') === 'true') return false;
      if (parent === dialog) break;
    }
    return true;
  };
  const leafSpans = (root, pattern) => {
    const spans = Array.from(root.querySelectorAll('span')).filter(node => enabled(node) && pattern.test(label(node)));
    return spans.filter(node => !spans.some(other => other !== node && node.contains(other)));
  };
  const selector = 'button, [role="button"], [role="combobox"], [role="radio"], [role="option"], [role="menuitem"], input[type="radio"], select';
  const controls = Array.from(dialog.querySelectorAll(selector)).filter(enabled);
  const actionsKey = Symbol.for('streammydrone.instagram.setupActions');
  if (!window[actionsKey] || window[actionsKey].revision !== revision) window[actionsKey] = { revision, claims: new Set() };
  const claims = window[actionsKey].claims;
  const uncovered = node => {
    node.scrollIntoView({ block: 'center', inline: 'center', behavior: 'instant' });
    const rect = node.getBoundingClientRect();
    const x = (Math.max(0, rect.left) + Math.min(window.innerWidth, rect.right)) / 2;
    const y = (Math.max(0, rect.top) + Math.min(window.innerHeight, rect.bottom)) / 2;
    const top = document.elementFromPoint(x, y);
    return Number.isFinite(x) && Number.isFinite(y) && !!top && (top === node || node.contains(top));
  };
  const click = (node, action) => {
    if (claims.has(action)) return 'waiting';
    if (!uncovered(node)) return 'covered';
    claims.add(action); // Claim before dispatch; a lost callback cannot cause a second click.
    node.click();
    return action;
  };
  // Native select controls expose their selected option; never infer a choice from defaults.
  const selects = controls.filter(node => node.tagName === 'SELECT' && Array.from(node.options).some(option => wanted.test(label(option))));
  if (selects.length > 1) return 'ambiguous';
  let selected = false;
  if (selects.length === 1) {
    const select = selects[0];
    const options = Array.from(select.options).filter(option => wanted.test(label(option)) && !option.disabled);
    if (options.length !== 1) return 'ambiguous';
    selected = select.value === options[0].value;
    if (!selected) {
      if (claims.has('select')) return 'waiting';
      if (!uncovered(select)) return 'covered';
      const setter = Object.getOwnPropertyDescriptor(HTMLSelectElement.prototype, 'value')?.set;
      if (!setter) return 'unavailable';
      claims.add('select'); setter.call(select, options[0].value);
      select.dispatchEvent(new Event('input', { bubbles: true }));
      select.dispatchEvent(new Event('change', { bubbles: true }));
      return 'selected';
    }
  }
  const choices = controls.filter(node => wanted.test(label(node)));
  const checked = node => node.checked === true || node.getAttribute('aria-checked') === 'true' || node.getAttribute('aria-selected') === 'true';
  const trigger = node => node.getAttribute('role') === 'combobox' || node.getAttribute('aria-haspopup') != null ||
    (node.getAttribute('role') === 'button' || node.tagName === 'BUTTON') && !['option', 'radio', 'menuitem'].includes(node.getAttribute('role'));
  const audiencePattern = new RegExp(Object.values(patterns).map(pattern => pattern.source).join('|'), 'i');
  const plainAudience = leafSpans(dialog, audiencePattern);
  const allAudience = leafSpans(document, audiencePattern);
  const audienceTriggers = controls.filter(node => trigger(node) && anyAudience(label(node)));
  // The observed desktop picker is a plain span inside unlabelled DIVs. It is a
  // collapsed choice only when exactly one audience label is visible, including portals.
  const plainCollapsed = plainAudience.length === 1 && allAudience.length === 1;
  if (!selected && plainCollapsed && wanted.test(label(plainAudience[0]))) selected = true;
  if (!selected) selected = choices.some(checked) || (choices.length === 1 && audienceTriggers.length === 1 && trigger(choices[0]) &&
    choices[0].getAttribute('aria-expanded') !== 'true');
  if (!selected) {
    // Dropdown options can live in a portal outside the title dialog. Exact, semantic,
    // visible audience entries only; no broad page text or coordinate guessing.
    const options = Array.from(document.querySelectorAll('[role="option"], [role="menuitem"], [role="radio"], input[type="radio"]'))
      .filter(node => enabled(node) && wanted.test(label(node)));
    if (options.length > 1) return 'ambiguous';
    if (options.length === 1) return click(options[0], 'selected');
    if (claims.has('audience-opened')) {
      // Device evidence: the audience portal uses ordinary BUTTONs without role=option.
      // Require exactly the Public/Practice pair in a small shared menu container, never
      // arbitrary page buttons or the original title-dialog trigger. WebKit shows the menu as
      // a sheet that also offers Close friends and Cancel; those two, and only those, may
      // share the container, and the title dialog's own Public/Practice button is not counted.
      const sheetExtra = /^(close friends|yakın arkadaşlar|mejores amigos|amis proches|enge freunde|amici più stretti|melhores amigos|близкие друзья|الأصدقاء المقربون|क्लोज़ फ़्रेंड्स|teman dekat|親しい友達|친한 친구|密友|摯友|cancel|iptal|vazgeç|cancelar|annuler|abbrechen|annulla|отмена|إلغاء|रद्द करें|batal|キャンセル|취소|取消)$/i;
      let buttons = Array.from(document.querySelectorAll('button')).filter(node =>
        enabled(node) && node.tagName === 'BUTTON' && anyAudience(label(node)));
      if (buttons.length !== 2) buttons = buttons.filter(node => !dialog.contains(node));
      const desiredButtons = buttons.filter(node => wanted.test(label(node)));
      if (buttons.length === 2 && desiredButtons.length === 1 &&
          Object.values(patterns).every(pattern => buttons.some(node => pattern.test(label(node))))) {
        let group = buttons[0].parentElement;
        for (let depth = 0; group && depth < 4 && group !== dialog && group !== document.body; group = group.parentElement, depth++) {
          const members = Array.from(group.querySelectorAll('button')).filter(enabled);
          if (buttons.every(node => group.contains(node)) && (members.length === 2 ||
              members.every(node => buttons.includes(node) || sheetExtra.test(label(node))))) return click(desiredButtons[0], 'selected');
        }
      }
      const plainOptions = leafSpans(document, wanted);
      if (plainOptions.length > 1) return 'ambiguous';
      if (plainOptions.length === 1 && !plainCollapsed) return click(plainOptions[0], 'selected');
    }
    const triggers = audienceTriggers.length ? audienceTriggers : plainCollapsed ? plainAudience : [];
    if (triggers.length > 1) return 'ambiguous';
    if (triggers.length === 1) return click(triggers[0], 'audience-opened');
    return 'unavailable';
  }
  const nextPattern = /^(next|continue|ileri|İleri|devam|devam et|sonraki|siguiente|suivant|weiter|avanti|próximo|далее|التالي|आगे|berikutnya|次へ|다음|下一步)$/i;
  const semanticNext = controls.filter(node => nextPattern.test(label(node)));
  const next = semanticNext.length ? semanticNext : leafSpans(dialog, nextPattern);
  if (next.length !== 1) return next.length ? 'ambiguous' : 'waiting';
  const result = click(next[0], 'next');
  if (result === 'next') window[Symbol.for('streammydrone.instagram.audienceRecord')] = { revision, audience };
  return result;
})();
