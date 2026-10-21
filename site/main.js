'use strict';

const root = document.documentElement;
const page = document.querySelector('[data-lens-root]');
const phone = window.matchMedia('(max-width: 560px)');
const reducedMotion = window.matchMedia('(prefers-reduced-motion: reduce)');

// Appearance: the system's, unless the footer switch chose one; remembered on this device.
function applyTheme(choice) {
  if (choice === 'light' || choice === 'dark') root.dataset.theme = choice;
  else delete root.dataset.theme;
  document.querySelectorAll('[data-theme-option]').forEach((button) => {
    button.setAttribute('aria-pressed', String(button.dataset.themeOption === choice));
  });
}
let themeChoice = 'system';
try {
  themeChoice = localStorage.getItem('nightcap-theme') || 'system';
} catch (e) {}
applyTheme(themeChoice);
document.querySelectorAll('[data-theme-option]').forEach((button) => {
  button.addEventListener('click', () => {
    themeChoice = button.dataset.themeOption;
    applyTheme(themeChoice);
    try {
      if (themeChoice === 'system') localStorage.removeItem('nightcap-theme');
      else localStorage.setItem('nightcap-theme', themeChoice);
    } catch (e) {}
  });
});

// The page is under Night Shift; the switch in the nav takes it off.
let shiftOn = true;
const shiftSwitch = document.querySelector('[data-shift-switch]');
shiftSwitch.addEventListener('click', () => {
  shiftOn = !shiftOn;
  shiftSwitch.setAttribute('aria-pressed', String(shiftOn));
  shiftSwitch.querySelector('.shift-label').textContent = shiftOn ? 'Night Shift: On' : 'Night Shift: Off';
  page.classList.toggle('shift-off', !shiftOn);
  showDesk();
});

// On an upright iPhone with a notch the hero starts with a runway for Safari's status bar
// (index.html, style.css), scrolled past on arrival unless the link points into the page or the
// reader has already scrolled.
const runway = parseFloat(getComputedStyle(document.querySelector('.hero')).paddingTop);
if (runway > 0 && !location.hash && window.scrollY < runway) window.scrollTo({ top: runway, behavior: 'instant' });

// The end of the page in view, for the body's background (style.css).
function markEnd() {
  document.body.classList.toggle('at-end', window.scrollY + window.innerHeight / 2 > root.scrollHeight / 2);
}
markEnd();
window.addEventListener('scroll', markEnd, { passive: true });
window.addEventListener('resize', markEnd);

// The true-colour circle follows the pointer, staying under it through scrolling. After 2.5 s
// still, it wanders inside the lens area of the section most in view, or, on phones, which
// have no pointer to steer it, over the hero's scene. It writes CSS variables, so a frame never
// lays the page out again.
const lens = { client: null, lastMove: 0, area: null, picked: 0, pos: null };
page.addEventListener('pointermove', (event) => {
  if (event.pointerType !== 'mouse') return;
  lens.client = { x: event.clientX, y: event.clientY };
  lens.lastMove = performance.now();
});
page.addEventListener('pointerleave', () => { lens.lastMove = 0; });

function mostVisibleArea() {
  const height = window.innerHeight;
  let best = null;
  let bestShare = 0;
  page.querySelectorAll('[data-lens-section]').forEach((section) => {
    const r = section.getBoundingClientRect();
    const shown = Math.max(0, Math.min(r.bottom, height) - Math.max(r.top, 0));
    const share = r.height > 0 ? shown / r.height : 0;
    if (share > bestShare + 0.001) {
      bestShare = share;
      best = section.matches('[data-lens-area]') ? section : section.querySelector('[data-lens-area]');
    }
  });
  return best;
}

function tick(now) {
  requestAnimationFrame(tick);
  const box = page.getBoundingClientRect();
  let target;
  if (lens.client && now - lens.lastMove < 2500) {
    target = { x: lens.client.x - box.left, y: lens.client.y - box.top };
  } else {
    if (!lens.area || now - lens.picked > 300) {
      lens.area = phone.matches ? page.querySelector('.scene') : mostVisibleArea();
      lens.picked = now;
    }
    if (!lens.area) return;
    const a = lens.area.getBoundingClientRect();
    const cx = a.left + a.width / 2 - box.left;
    const cy = a.top + a.height / 2 - box.top;
    const t = now / 1000;
    target = reducedMotion.matches ? { x: cx, y: cy } : {
      x: cx + Math.sin(t * 0.55) * a.width * 0.32,
      y: cy + Math.cos(t * 0.4) * a.height * 0.32,
    };
  }
  if (!lens.pos) lens.pos = { x: target.x, y: target.y };
  const ease = reducedMotion.matches ? 1 : 0.1;
  lens.pos.x += (target.x - lens.pos.x) * ease;
  lens.pos.y += (target.y - lens.pos.y) * ease;
  page.style.setProperty('--lx', lens.pos.x.toFixed(1) + 'px');
  page.style.setProperty('--ly', lens.pos.y.toFixed(1) + 'px');
}
requestAnimationFrame(tick);

// The live Mac: the Nightcap menu as the app builds it, for whatever is in front.
const MOON_ON = '<svg viewBox="0 0 24 24"><path d="M21 12.79A9 9 0 1 1 11.21 3 7 7 0 0 0 21 12.79z" fill="currentColor"/></svg>';
const MOON_FORCED = '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor"><path d="M21 12.79A9 9 0 1 1 11.21 3 7 7 0 0 0 21 12.79z" stroke-width="2" stroke-linejoin="round"/><path d="M15.5 2.5h4l-4 4.5h4" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round"/></svg>';
const MOON_OFF = '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor"><path d="M21 12.79A9 9 0 1 1 11.21 3 7 7 0 0 0 21 12.79z" stroke-width="2" stroke-linejoin="round"/></svg>';
const MOONS = { on: MOON_ON, forced: MOON_FORCED, off: MOON_OFF };
// The menu's other images, drawn like the moons: a 24 grid, 2px strokes, round joins.
const glyph = (paths) => `<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">${paths}</svg>`;
const ICONS = {
  timer: glyph('<circle cx="12" cy="13.5" r="7.5"/><path d="M12 10v3.5l2.3 2.3M9.5 2.5h5"/>'),
  globe: glyph('<circle cx="12" cy="12" r="9"/><path d="M3 12h18M12 3c2.4 2.6 3.6 5.6 3.6 9s-1.2 6.4-3.6 9c-2.4-2.6-3.6-5.6-3.6-9S9.6 5.6 12 3z"/>'),
  gear: glyph('<circle cx="12" cy="12" r="3"/><path d="M12 2.8v2.4M12 18.8v2.4M2.8 12h2.4M18.8 12h2.4M5.5 5.5l1.7 1.7M16.8 16.8l1.7 1.7M5.5 18.5l1.7-1.7M16.8 7.2l1.7-1.7"/><circle cx="12" cy="12" r="6.2"/>'),
  power: glyph('<path d="M12 3v8M6.3 6.8a8 8 0 1 0 11.4 0"/>'),
};
// The slider's ends, as the app draws them: a small four-rayed sun, a large eight-rayed one.
const SUN_LESS = '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.4" stroke-linecap="round"><circle cx="12" cy="12" r="4"/><path d="M12 3v2M12 19v2M3 12h2M19 12h2"/></svg>';
const SUN_MORE = '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round"><circle cx="12" cy="12" r="4.2"/><path d="M12 2.5v2.5M12 19v2.5M2.5 12H5M19 12h2.5M5.3 5.3l1.8 1.8M16.9 16.9l1.8 1.8M5.3 18.7l1.8-1.8M16.9 7.1l1.8-1.8"/></svg>';
const APPS = { mail: ['M', '#3d6fd6'], figma: ['F', '#a259ff'], safari: ['S', '#1d9bf0'] };
const SUNRISE = '07:52';
const KEYS = '⌃⌥⌘N';
const item = (title, extra = {}) => ({ title, ...extra });
// The Night Shift item, which opens the menu: checked while on, why, and what choosing it does.
const nightShift = (moon, reason, detail) => ({ nightShift: true, moon, reason, detail });
const STRENGTH = { slider: 0.62 };
const turnOffFor = item('Turn Off For', { key: '›', icon: 'timer' });
const tail = [item('Settings…', { key: '⌘,', icon: 'gear' }), item('Quit Nightcap', { key: '⌘Q', icon: 'power' })];
const MENUS = {
  mail: [
    nightShift('on', `Following the schedule until ${SUNRISE}`), STRENGTH, 'sep',
    turnOffFor, 'sep', { header: 'Force Off' },
    item('In Mail', { app: 'mail' }), 'sep', ...tail,
  ],
  figma: [
    nightShift('forced', 'While Figma is in front', `Snoozes the Figma rule until ${SUNRISE}`), STRENGTH, 'sep',
    turnOffFor, 'sep', { header: 'Force Off' },
    item('In Figma', { check: true, subtitle: 'From your config', app: 'figma' }), 'sep', ...tail,
  ],
  safari: [
    nightShift('forced', 'While youtube.com is in front', `Snoozes the youtube.com rule until ${SUNRISE}`), STRENGTH, 'sep',
    turnOffFor, 'sep', { header: 'Force Off' },
    item('In Safari', { app: 'safari' }), item('On youtube.com', { check: true, indented: true, icon: 'globe' }), 'sep', ...tail,
  ],
};
const APP_NAMES = { mail: 'Mail', figma: 'Figma', safari: 'Safari' };

function el(tag, className, text) {
  const node = document.createElement(tag);
  if (className) node.className = className;
  if (text) node.textContent = text;
  return node;
}

function menuEntry(entry) {
  if (entry === 'sep') return el('div', 'menu-sep');
  if (entry.header) return el('div', 'menu-header', entry.header);
  if (entry.slider !== undefined) {
    const slider = el('div', 'menu-slider');
    const less = el('span', 'menu-sun menu-sun-less');
    less.innerHTML = SUN_LESS;
    const track = el('span', 'menu-track');
    track.style.setProperty('--value', entry.slider);
    track.append(el('span', 'menu-track-fill'), el('span', 'menu-track-knob'));
    const more = el('span', 'menu-sun menu-sun-more');
    more.innerHTML = SUN_MORE;
    slider.append(less, track, more);
    return slider;
  }
  const row = el('div', 'menu-item' + (entry.indented ? ' menu-indented' : ''));
  row.append(el('span', 'menu-check', entry.check || entry.moon === 'on' ? '✓' : ''));
  const image = el('span', 'menu-image');
  if (entry.app) {
    const [letter, tile] = APPS[entry.app];
    const app = el('span', 'menu-app', letter);
    app.style.setProperty('--tile', tile);
    image.append(app);
  } else {
    image.innerHTML = entry.nightShift ? MOONS[entry.moon] : ICONS[entry.icon] || '';
  }
  const text = el('span', 'menu-text');
  text.append(el('span', 'menu-title', entry.nightShift ? 'Night Shift' : entry.title));
  [entry.nightShift ? entry.reason : entry.subtitle, entry.detail].filter(Boolean)
    .forEach((line) => text.append(el('span', 'menu-subtitle', line)));
  row.append(image, text, el('span', 'menu-key', entry.nightShift ? KEYS : entry.key || ''));
  return row;
}

// The hero's scene: the menu while Figma is in front.
document.querySelector('[data-scene-menu]').replaceChildren(...MENUS.figma.map(menuEntry));
document.querySelector('[data-scene-moon]').innerHTML = MOON_FORCED;

let front = 'figma';
let picked = false;
function showDesk() {
  document.querySelectorAll('[data-window]').forEach((win) => { win.hidden = win.dataset.window !== front; });
  document.querySelectorAll('[data-menu]').forEach((menu) => menu.replaceChildren(...MENUS[front].map(menuEntry)));
  document.querySelectorAll('[data-app]').forEach((button) => button.setAttribute('aria-pressed', String(button.dataset.app === front)));
  document.querySelector('[data-app-name]').textContent = APP_NAMES[front];
  const warm = front === 'mail';
  document.querySelector('[data-desk-moon]').innerHTML = warm ? MOON_ON : MOON_FORCED;
  document.querySelector('[data-desk-tint]').classList.toggle('on', warm && shiftOn);
}
document.querySelectorAll('[data-app]').forEach((button) => {
  button.addEventListener('click', () => {
    picked = true;
    front = button.dataset.app;
    showDesk();
  });
});
showDesk();
const cycle = ['figma', 'mail', 'safari', 'mail'];
let step = 0;
setInterval(() => {
  if (picked) return;
  step = (step + 1) % cycle.length;
  front = cycle[step];
  showDesk();
}, 3200);

// One evening: what each moment does to the screen and the menu.
const MOMENTS = [
  { time: '18:36', title: 'Sunset', warm: true, body: 'The schedule starts Night Shift, and Nightcap leaves it alone while Mail is in front.', moon: 'on', reason: `Following the schedule until ${SUNRISE}`, app: 'mail', item: 'In Mail' },
  { time: '21:15', title: 'You open Figma', warm: false, body: 'Figma has a rule, so Nightcap switches Night Shift off. The colours on screen are the colours in the file.', moon: 'forced', reason: 'While Figma is in front', detail: `Snoozes the Figma rule until ${SUNRISE}`, app: 'figma', item: 'In Figma', check: true, note: 'From your config' },
  { time: '22:40', title: 'Back to Mail', warm: true, body: 'Mail has no rule. Night Shift comes straight back, warm as before.', moon: 'on', reason: `Following the schedule until ${SUNRISE}`, app: 'mail', item: 'In Mail' },
  { time: '23:30', title: 'You want it warm', warm: true, body: 'You turn Night Shift on from the menu while Figma is in front. Nightcap takes it as meant: the Figma rule snoozes until sunrise.', moon: 'on', reason: `Turned on by hand until ${SUNRISE}`, app: 'figma', item: 'In Figma', note: `Snoozed until ${SUNRISE}` },
  { time: SUNRISE, title: 'Sunrise', warm: false, body: 'The schedule ends Night Shift, and the snooze ends with it. Tonight, Figma is true colour again.', moon: 'forced', reason: 'While Figma is in front', detail: `Snoozes the Figma rule until ${SUNRISE}`, app: 'figma', item: 'In Figma', check: true, note: 'From your config' },
];
const momentsBox = document.querySelector('[data-moments]');
const sky = document.querySelector('[data-sky]');
const momentBody = document.querySelector('[data-moment-body]');
const momentMenu = document.querySelector('[data-moment-menu]');
const markers = MOMENTS.map((_, i) => {
  const marker = el('span', 'sky-marker');
  marker.style.left = 10 + i * 20 + '%';
  sky.append(marker);
  return marker;
});
const momentButtons = MOMENTS.map((moment, i) => {
  const button = el('button', 'moment');
  button.type = 'button';
  const time = el('span', 'moment-time');
  const dot = el('span', 'moment-dot' + (moment.warm ? ' warm' : ''));
  dot.setAttribute('role', 'img');
  dot.setAttribute('aria-label', moment.warm ? 'Warm screen' : 'True colour');
  time.append(dot, moment.time);
  button.append(time, el('span', 'moment-title', moment.title));
  button.addEventListener('click', () => showMoment(i));
  momentsBox.append(button);
  return button;
});
function showMoment(index) {
  const moment = MOMENTS[index];
  momentButtons.forEach((button, i) => button.setAttribute('aria-pressed', String(i === index)));
  markers.forEach((marker, i) => marker.classList.toggle('on', i === index));
  momentBody.textContent = moment.body;
  momentBody.classList.remove('swap');
  void momentBody.offsetWidth;
  momentBody.classList.add('swap');
  momentMenu.replaceChildren(...[
    nightShift(moment.moon, moment.reason, moment.detail), 'sep', { header: 'Force Off' },
    item(moment.item, { app: moment.app, check: moment.check, subtitle: moment.note }),
  ].map(menuEntry));
  const state = { on: 'on', forced: 'forced off', off: 'off' }[moment.moon];
  momentMenu.setAttribute('role', 'img');
  momentMenu.setAttribute('aria-label', `Nightcap's menu: Night Shift ${state}, ${moment.reason}`);
}
showMoment(3);

// The shortcut: one set of keys under two cards, telling a tap from a hold as the app does.
// Until pressed, the keys act it out on the cards: a tap, a tap back, then a hold. Pressed, the
// press lifts the page's warmth at once, and a release within 0.3 s was a tap, which switches
// the page's Night Shift as the nav's switch does. The card for the move that happened lights.
{
  const section = document.querySelector('#shortcut');
  const tray = section.querySelector('[data-shortcut-keys]');
  const keys = tray.querySelector('.keys');
  const hint = section.querySelector('[data-key-hint]');
  const tapCard = section.querySelector('[data-move="tap"]');
  const holdCard = section.querySelector('[data-move="hold"]');
  const paint = (card, warm, moon) => {
    card.querySelector('[data-mini-tint]').classList.toggle('on', warm);
    card.querySelector('[data-mini-moon]').innerHTML = MOONS[moon];
  };
  const light = (card) => [tapCard, holdCard].forEach((c) => c.classList.toggle('lit', c === card));
  const showTap = (on) => paint(tapCard, on, on ? 'on' : 'off');
  const showHold = (peeking) => paint(holdCard, !peeking, 'on');
  showTap(true);
  showHold(false);

  const timers = [];
  const at = (ms, run) => timers.push(setTimeout(run, ms));
  function demo() {
    at(0, () => keys.classList.add('down'));
    at(140, () => { keys.classList.remove('down'); light(tapCard); showTap(false); });
    at(1700, () => keys.classList.add('down'));
    at(1840, () => { keys.classList.remove('down'); showTap(true); });
    at(3300, () => { keys.classList.add('down'); light(holdCard); showHold(true); });
    at(5500, () => { keys.classList.remove('down'); showHold(false); });
    at(6800, () => { light(null); demo(); });
  }
  if (!reducedMotion.matches) demo();
  let demoing = !reducedMotion.matches;

  let pressedAt = null;
  let lastMove = null;
  function press() {
    if (pressedAt !== null) return;
    if (demoing) {
      demoing = false;
      timers.forEach(clearTimeout);
      light(null);
      showTap(shiftOn);
    }
    pressedAt = performance.now();
    keys.classList.add('down');
    if (!shiftOn) return;
    showHold(true);
    page.classList.remove('peek-return');
    page.classList.add('peeking');
    hint.textContent = 'True colour while you hold. Let go and the warmth comes back.';
  }
  function release() {
    if (pressedAt === null) return;
    const held = performance.now() - pressedAt;
    pressedAt = null;
    keys.classList.remove('down');
    showHold(false);
    const peeked = page.classList.contains('peeking');
    page.classList.remove('peeking');
    if (held < 300) {
      shiftSwitch.click();
      showTap(shiftOn);
      light(tapCard);
      hint.textContent = shiftOn ? 'A tap: Night Shift is back on. Now hold the keys.' : 'A tap: Night Shift is off, page and all. Tap again to turn it back on.';
      lastMove = 'tap';
    } else if (peeked) {
      page.classList.add('peek-return');
      setTimeout(() => page.classList.remove('peek-return'), 300);
      light(holdCard);
      hint.textContent = lastMove === 'tap' ? 'A hold: a peek, and Night Shift stayed on.' : 'A hold: a peek. Now tap the keys instead.';
      lastMove = 'hold';
    } else {
      hint.textContent = 'Night Shift is off, so there was nothing to peek past. Tap to turn it back on.';
    }
  }
  tray.addEventListener('pointerdown', (event) => {
    event.preventDefault();
    tray.setPointerCapture(event.pointerId);
    press();
  });
  tray.addEventListener('pointerup', release);
  tray.addEventListener('pointercancel', release);
  tray.addEventListener('keydown', (event) => {
    if ((event.key === ' ' || event.key === 'Enter') && !event.repeat) {
      event.preventDefault();
      press();
    }
  });
  tray.addEventListener('keyup', (event) => { if (event.key === ' ' || event.key === 'Enter') release(); });
  tray.addEventListener('click', (event) => event.preventDefault());
}

// Doodles: the margins get a jittered, staggered grid (one per cell, so no clumps or gaps),
// the centre a few kept far apart, and the hero's open space a scatter around its content. None
// covers the hero's content or a section's. Kinds come from shuffled decks and never repeat a
// neighbour; each kind's sizes cycle through small, medium and large. Seeded, so the layout is
// the same on every visit.
const DOODLE_PATHS = {
  star: '<path d="M12 3.3l2.6 5.5 6 .8-4.4 4.1 1.1 6-5.3-2.9-5.3 2.9 1.1-6-4.4-4.1 6-.8z" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linejoin="round"/>',
  sparkle: '<path d="M12 3c.9 4.7 2 6.2 8.6 9-6.6 2.8-7.7 4.3-8.6 9-.9-4.7-2-6.2-8.6-9 6.6-2.8 7.7-4.3 8.6-9z" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linejoin="round"/>',
  dot: '<circle cx="12" cy="12" r="9" fill="currentColor"/>',
  moon: '<path d="M20 14.6A8.6 8.6 0 1 1 9.4 4.1a6.7 6.7 0 0 0 10.6 10.5z" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"/>',
  sun: '<g fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round"><circle cx="12" cy="12" r="4.4"/><path d="M12 2.6v2.6M12 18.8v2.6M2.6 12h2.6M18.8 12h2.6M5.4 5.4l1.9 1.9M16.7 16.7l1.9 1.9M5.4 18.6l1.9-1.9M16.7 7.3l1.9-1.9"/></g>',
};

function makeDoodles(bottom, hero) {
  let seed = 20261016;
  const rand = () => {
    seed = (seed + 0x6d2b79f5) | 0;
    let t = Math.imul(seed ^ (seed >>> 15), 1 | seed);
    t = (t + Math.imul(t ^ (t >>> 7), 61 | t)) ^ t;
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  };
  const between = (a, b) => a + rand() * (b - a);
  const shuffle = (xs) => {
    const a = xs.slice();
    for (let i = a.length - 1; i > 0; i--) {
      const j = Math.floor(rand() * (i + 1));
      [a[i], a[j]] = [a[j], a[i]];
    }
    return a;
  };
  const deckOf = ['star', 'star', 'sparkle', 'sparkle', 'dot', 'dot', 'moon', 'sun'];
  let deck = [];
  const drawKind = (avoid) => {
    if (deck.length === 0) deck = shuffle(deckOf);
    let i = deck.findIndex((k) => !avoid.includes(k));
    if (i < 0) i = 0;
    return deck.splice(i, 1)[0];
  };
  const sizes = { star: [12, 24], sparkle: [12, 26], dot: [4, 8], moon: [18, 32], sun: [22, 36] };
  const tiers = {};
  const sizeOf = (kind) => {
    if (!tiers[kind] || tiers[kind].length === 0) tiers[kind] = shuffle([0.15, 0.5, 0.85]);
    const f = Math.min(1, Math.max(0, tiers[kind].pop() + between(-0.1, 0.1)));
    return Math.round(sizes[kind][0] + f * (sizes[kind][1] - sizes[kind][0]));
  };
  const counts = {};
  let placedCount = 0;
  const doodle = (kind, x, y) => {
    counts[kind] = (counts[kind] || 0) + 1;
    const anim = kind === 'sun' ? 'spin' : kind === 'moon' ? 'sway' : counts[kind] % 5 === 0 ? 'drift' : 'twinkle';
    const dur = anim === 'spin' ? between(45, 80) : anim === 'sway' ? between(6, 9) : anim === 'drift' ? between(8, 11) : between(3, 6);
    const gold = kind === 'sun' || kind === 'moon' || counts[kind] % 4 === 0;
    return {
      kind, x, y, size: sizeOf(kind), keep: placedCount++ % 4 === 0,
      rot: kind === 'sun' ? 0 : Math.round(between(-25, 25)), anim,
      delay: between(0, 4), dur, gold,
    };
  };

  const top = 24;
  const cell = 150;
  const columns = [[1, 8], [8, 15], [85, 92], [92, 99]];
  const placed = [];
  const above = columns.map(() => null);
  for (let row = 0; top + row * cell < bottom; row++) {
    let left = null;
    columns.forEach(([x0, x1], c) => {
      const stagger = c % 2 === 1 ? cell / 2 : 0;
      const y0 = top + row * cell + stagger;
      if (y0 > bottom) return;
      const kind = drawKind([above[c], left].filter(Boolean));
      // Of a few spots in the cell, the one farthest from every doodle so far.
      let best = null;
      let bestGap = -1;
      for (let i = 0; i < 10; i++) {
        const x = between(x0 + 0.6, x1 - 0.6);
        const y = between(y0 + cell * 0.1, y0 + cell * 0.9);
        const gap = placed.reduce((g, d) => Math.min(g, Math.hypot((d.x - x) / 100 * 1440, d.y - y)), Infinity);
        if (gap > bestGap) { bestGap = gap; best = { x, y }; }
      }
      placed.push(doodle(kind, best.x, best.y));
      above[c] = kind;
      left = kind;
    });
  }
  const centre = [];
  for (let tries = 0; tries < 400 && centre.length < 14; tries++) {
    const x = between(16, 84);
    const y = between(700, bottom);
    if (centre.every((d) => Math.hypot((d.x - x) / 100 * 1440, d.y - y) > 300)) centre.push({ x, y });
  }
  centre.forEach((d) => placed.push(doodle(drawKind([]), d.x, d.y)));
  // A doodle at (x, y) spans at most 36px, the largest sun.
  const covers = (x, y) => hero.clear.some((r) => x + 36 > r.left && x < r.right && y + 36 > r.top && y < r.bottom);
  const open = placed.filter((d) => d.y > hero.height || !covers(d.x / 100 * hero.width, d.y));
  const wanted = Math.round((hero.width * hero.height) / (110 * 110));
  for (let tries = 0, added = 0; tries < 4000 && added < wanted; tries++) {
    const px = between(12, hero.width - 12);
    const y = between(12, hero.height - 12);
    const x = px / hero.width * 100;
    if (covers(px, y) || open.some((d) => Math.hypot((d.x - x) / 100 * hero.width, d.y - y) < 92)) continue;
    open.push(doodle(drawKind([]), x, y));
    added++;
  }
  return open;
}

function drawDoodles() {
  const hero = document.querySelector('.hero');
  const heroBox = hero.getBoundingClientRect();
  const heroHeight = hero.offsetHeight;
  const pageHeight = page.offsetHeight;
  // The nav gets a narrower margin than the content, so the band above the content fills too.
  const clear = [['.nav', 12], ['.hero-text', 26], ['.scene', 26], ['.hero-cue', 20]].map(([selector, margin]) => {
    const r = hero.querySelector(selector).getBoundingClientRect();
    return { left: r.left - heroBox.left - margin, right: r.right - heroBox.left + margin, top: r.top - heroBox.top - margin, bottom: r.bottom - heroBox.top + margin };
  });
  const all = makeDoodles(pageHeight, { width: heroBox.width, height: heroHeight, clear });
  const pageBox = page.getBoundingClientRect();
  const content = [...page.querySelectorAll('main > section:not(.dusk) > *, main > .dusk, main > footer')].map((node) => {
    const r = node.getBoundingClientRect();
    return { left: r.left - pageBox.left - 12, right: r.right - pageBox.left + 12, top: r.top - pageBox.top - 12, bottom: r.bottom - pageBox.top + 12 };
  });
  const offContent = (d) => {
    const x = d.x / 100 * pageBox.width;
    return !content.some((r) => x + d.size > r.left && x < r.right && d.y + d.size > r.top && d.y < r.bottom);
  };
  const render = (box, list) => box.replaceChildren(...list.map((d) => {
    const outer = el('span', 'doodle' + (d.keep ? ' keep' : ''));
    outer.style.cssText = `left:${d.x.toFixed(2)}%;top:${Math.round(d.y)}px;width:${d.size}px;height:${d.size}px;transform:rotate(${d.rot}deg);color:var(${d.gold ? '--doodle-gold' : '--doodle'})`;
    const inner = el('span', d.anim);
    inner.style.cssText = `animation-delay:${d.delay.toFixed(2)}s;animation-duration:${d.dur.toFixed(1)}s`;
    inner.innerHTML = `<svg viewBox="0 0 24 24">${DOODLE_PATHS[d.kind]}</svg>`;
    outer.append(inner);
    return outer;
  }));
  render(document.querySelector('[data-doodles="hero"]'), all.filter((d) => d.y + d.size < heroHeight));
  render(document.querySelector('[data-doodles="page"]'), all.filter((d) => d.y > heroHeight && d.y + d.size < pageHeight - 24 && offContent(d)));
}
drawDoodles();
let resizeTimer;
function redrawDoodles() {
  clearTimeout(resizeTimer);
  resizeTimer = setTimeout(drawDoodles, 200);
}
window.addEventListener('resize', redrawDoodles);
// The web fonts arriving reflow the text, which the doodles keep clear of.
const reflow = new ResizeObserver(redrawDoodles);
reflow.observe(document.querySelector('.hero-text'));
reflow.observe(page);

// The download buttons point at the newest release's DMG, as the Pages deploy writes it into
// download.json. Until it loads, or where the site has none, they lead to the releases page.
fetch('download.json')
  .then((response) => {
    if (!response.ok) throw new Error(`download.json: ${response.status}`);
    return response.json();
  })
  .then(({ version, url }) => {
    document.querySelectorAll('[data-download]').forEach((link) => { link.href = url; });
    document.querySelectorAll('[data-version]').forEach((label) => {
      label.textContent = version;
      label.hidden = false;
    });
  });
