import { useEffect, type RefObject } from 'react';
import styles from './Lens.module.css';

/** The page's Night Shift tint and the ring around the true-colour circle cut out of it. */
export function Lens() {
  return (
    <>
      <div className={styles.tint} aria-hidden="true">
        <span />
        <span />
      </div>
      <div className={styles.ring} aria-hidden="true" />
    </>
  );
}

/** The true-colour circle follows the pointer, staying under it through scrolling. After 2.5 s
 *  still, it wanders inside the lens area (`data-lens-area`) of the section (`data-lens-section`)
 *  most in view, or, on phones, which have no pointer to steer it, over the page's first lens
 *  area, the hero's scene. It writes CSS variables, so a frame never lays the page out again. */
export function useLens(page: RefObject<HTMLElement | null>) {
  useEffect(() => {
    const root = page.current;
    if (!root) throw new Error('the lens has no page');
    const phone = window.matchMedia('(max-width: 560px)');
    const reducedMotion = window.matchMedia('(prefers-reduced-motion: reduce)');
    const lens: { client: { x: number; y: number } | null; lastMove: number; area: Element | null; picked: number; pos: { x: number; y: number } | null } = {
      client: null,
      lastMove: 0,
      area: null,
      picked: 0,
      pos: null,
    };
    const move = (event: PointerEvent) => {
      if (event.pointerType !== 'mouse') return;
      lens.client = { x: event.clientX, y: event.clientY };
      lens.lastMove = performance.now();
    };
    const leave = () => {
      lens.lastMove = 0;
    };
    root.addEventListener('pointermove', move);
    root.addEventListener('pointerleave', leave);

    const mostVisibleArea = () => {
      const height = window.innerHeight;
      let best: Element | null = null;
      let bestShare = 0;
      for (const section of root.querySelectorAll('[data-lens-section]')) {
        const r = section.getBoundingClientRect();
        const shown = Math.max(0, Math.min(r.bottom, height) - Math.max(r.top, 0));
        const share = r.height > 0 ? shown / r.height : 0;
        if (share > bestShare + 0.001) {
          bestShare = share;
          best = section.matches('[data-lens-area]') ? section : section.querySelector('[data-lens-area]');
        }
      }
      return best;
    };

    const tick = (now: number) => {
      frame = requestAnimationFrame(tick);
      const box = root.getBoundingClientRect();
      let target: { x: number; y: number };
      if (lens.client && now - lens.lastMove < 2500) {
        target = { x: lens.client.x - box.left, y: lens.client.y - box.top };
      } else {
        if (!lens.area || now - lens.picked > 300) {
          lens.area = phone.matches ? root.querySelector('[data-lens-area]') : mostVisibleArea();
          lens.picked = now;
        }
        if (!lens.area) return;
        const a = lens.area.getBoundingClientRect();
        const cx = a.left + a.width / 2 - box.left;
        const cy = a.top + a.height / 2 - box.top;
        const t = now / 1000;
        target = reducedMotion.matches
          ? { x: cx, y: cy }
          : { x: cx + Math.sin(t * 0.55) * a.width * 0.32, y: cy + Math.cos(t * 0.4) * a.height * 0.32 };
      }
      if (!lens.pos) lens.pos = { ...target };
      const ease = reducedMotion.matches ? 1 : 0.1;
      lens.pos.x += (target.x - lens.pos.x) * ease;
      lens.pos.y += (target.y - lens.pos.y) * ease;
      root.style.setProperty('--lx', lens.pos.x.toFixed(1) + 'px');
      root.style.setProperty('--ly', lens.pos.y.toFixed(1) + 'px');
    };
    let frame = requestAnimationFrame(tick);
    return () => {
      cancelAnimationFrame(frame);
      root.removeEventListener('pointermove', move);
      root.removeEventListener('pointerleave', leave);
    };
  }, [page]);
}
