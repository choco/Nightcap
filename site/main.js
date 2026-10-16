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

// The true-colour circle follows the pointer, staying under it through scrolling. After 2.5 s
// still, it wanders inside the lens area of the section most in view, or, on phones, which
// have no pointer to steer it, over the hero poster. It writes CSS variables, so a frame never
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
      lens.area = phone.matches ? page.querySelector('.poster') : mostVisibleArea();
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
const SUNRISE = '07:52';
const item = (title, extra = {}) => ({ title, ...extra });
const tail = [item('Settings…', { key: '⌘,' }), item('Quit Nightcap', { key: '⌘Q' })];
const MENUS = {
  mail: [
    item('Night Shift: On', { status: true, subtitle: `Following the schedule until ${SUNRISE}` }), 'sep',
    item('Turn Night Shift Off'), item('Turn Off For', { key: '▸' }), { header: 'Force Off' },
    item('In Mail'), 'sep', ...tail,
  ],
  figma: [
    item('Night Shift: Forced Off', { status: true, subtitle: 'While Figma is in front' }), 'sep',
    item('Turn Night Shift On', { subtitle: `Snoozes the Figma rule until ${SUNRISE}` }), item('Turn Off For', { key: '▸' }), { header: 'Force Off' },
    item('In Figma', { check: true, subtitle: 'From your config' }), 'sep', ...tail,
  ],
  safari: [
    item('Night Shift: Forced Off', { status: true, subtitle: 'While youtube.com is in front' }), 'sep',
    item('Turn Night Shift On', { subtitle: `Snoozes the youtube.com rule until ${SUNRISE}` }), item('Turn Off For', { key: '▸' }), { header: 'Force Off' },
    item('In Safari'), item('On youtube.com', { check: true, indented: true }), 'sep', ...tail,
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
  const row = el('div', 'menu-item' + (entry.status ? ' menu-status' : '') + (entry.indented ? ' menu-indented' : ''));
  row.append(el('span', 'menu-check', entry.check ? '✓' : ''));
  const text = el('span', 'menu-text');
  text.append(el('span', 'menu-title', entry.title));
  if (entry.subtitle) text.append(el('span', 'menu-subtitle', entry.subtitle));
  row.append(text, el('span', 'menu-key', entry.key || ''));
  return row;
}

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
  { time: '18:36', title: 'Sunset', warm: true, body: 'The schedule starts Night Shift, and Nightcap leaves it alone while Mail is in front.', status: 'Night Shift: On', reason: `Following the schedule until ${SUNRISE}`, item: 'In Mail' },
  { time: '21:15', title: 'You open Figma', warm: false, body: 'Figma has a rule, so Nightcap switches Night Shift off. The colours on screen are the colours in the file.', status: 'Night Shift: Forced Off', reason: 'While Figma is in front', item: 'In Figma', check: true, note: 'From your config' },
  { time: '22:40', title: 'Back to Mail', warm: true, body: 'Mail has no rule. Night Shift comes straight back, warm as before.', status: 'Night Shift: On', reason: `Following the schedule until ${SUNRISE}`, item: 'In Mail' },
  { time: '23:30', title: 'You want it warm', warm: true, body: 'You turn Night Shift on from the menu while Figma is in front. Nightcap takes it as meant: the Figma rule snoozes until sunrise.', status: 'Night Shift: On', reason: `Turned on by hand until ${SUNRISE}`, item: 'In Figma', note: `Snoozed until ${SUNRISE}` },
  { time: SUNRISE, title: 'Sunrise', warm: false, body: 'The schedule ends Night Shift, and the snooze ends with it. Tonight, Figma is true colour again.', status: 'Night Shift: Forced Off', reason: 'While Figma is in front', item: 'In Figma', check: true, note: 'From your config' },
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
  const status = el('div', 'menu-card-status');
  status.append(el('strong', '', moment.status), el('span', '', moment.reason));
  const row = el('div', 'menu-card-item');
  const text = el('span', '', moment.item);
  if (moment.note) text.append(el('span', 'menu-card-note', moment.note));
  row.append(el('span', '', moment.check ? '✓' : ''), text);
  momentMenu.replaceChildren(status, el('div', 'menu-card-sep'), el('div', 'menu-card-header', 'Force Off'), row);
  momentMenu.setAttribute('role', 'img');
  momentMenu.setAttribute('aria-label', `Nightcap's menu: ${moment.status}, ${moment.reason}`);
}
showMoment(3);

// Doodles: the margins get a jittered, staggered grid (one per cell, so no clumps or gaps)
// and the centre a few kept far apart. Kinds come from shuffled decks and never repeat a
// neighbour; each kind's sizes cycle through small, medium and large. Seeded, so the layout
// is the same on every visit.
const DOODLE_PATHS = {
  star: '<path d="M12 3.3l2.6 5.5 6 .8-4.4 4.1 1.1 6-5.3-2.9-5.3 2.9 1.1-6-4.4-4.1 6-.8z" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linejoin="round"/>',
  sparkle: '<path d="M12 3c.9 4.7 2 6.2 8.6 9-6.6 2.8-7.7 4.3-8.6 9-.9-4.7-2-6.2-8.6-9 6.6-2.8 7.7-4.3 8.6-9z" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linejoin="round"/>',
  dot: '<circle cx="12" cy="12" r="9" fill="currentColor"/>',
  moon: '<path d="M20 14.6A8.6 8.6 0 1 1 9.4 4.1a6.7 6.7 0 0 0 10.6 10.5z" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"/>',
  sun: '<g fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round"><circle cx="12" cy="12" r="4.4"/><path d="M12 2.6v2.6M12 18.8v2.6M2.6 12h2.6M18.8 12h2.6M5.4 5.4l1.9 1.9M16.7 16.7l1.9 1.9M5.4 18.6l1.9-1.9M16.7 7.3l1.9-1.9"/></g>',
};

function makeDoodles(bottom) {
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
  return placed;
}

function drawDoodles() {
  const heroHeight = document.querySelector('.hero').offsetHeight;
  const pageHeight = page.offsetHeight;
  const all = makeDoodles(pageHeight);
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
  render(document.querySelector('[data-doodles="page"]'), all.filter((d) => d.y > heroHeight && d.y + d.size < pageHeight - 24));
}
drawDoodles();
let resizeTimer;
window.addEventListener('resize', () => {
  clearTimeout(resizeTimer);
  resizeTimer = setTimeout(drawDoodles, 200);
});

// The download buttons point at the newest release's zip, as the site's appcast names it.
// Until it loads, or where the site has no appcast, they lead to the releases page.
fetch('appcast.xml')
  .then((response) => {
    if (!response.ok) throw new Error(`appcast.xml: ${response.status}`);
    return response.text();
  })
  .then((text) => {
    const feed = new DOMParser().parseFromString(text, 'application/xml');
    const enclosure = feed.querySelector('enclosure');
    const version = feed.getElementsByTagNameNS('*', 'shortVersionString')[0];
    if (!enclosure || !version) throw new Error('appcast.xml names no release');
    document.querySelectorAll('[data-download]').forEach((link) => { link.href = enclosure.getAttribute('url'); });
    document.querySelectorAll('[data-version]').forEach((label) => {
      label.textContent = version.textContent;
      label.hidden = false;
    });
  });
