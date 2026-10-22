import { useEffect, useState, type ReactNode, type RefObject } from 'react';
import { placeDoodles, type Box, type Doodle, type DoodleKind, type DoodleMotion } from '../lib/doodles';
import { cx } from '../lib/cx';

export type { Doodle };

const PATHS: Record<DoodleKind, ReactNode> = {
  star: <path d="M12 3.3l2.6 5.5 6 .8-4.4 4.1 1.1 6-5.3-2.9-5.3 2.9 1.1-6-4.4-4.1 6-.8z" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinejoin="round" />,
  sparkle: <path d="M12 3c.9 4.7 2 6.2 8.6 9-6.6 2.8-7.7 4.3-8.6 9-.9-4.7-2-6.2-8.6-9 6.6-2.8 7.7-4.3 8.6-9z" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinejoin="round" />,
  dot: <circle cx="12" cy="12" r="9" fill="currentColor" />,
  moon: <path d="M20 14.6A8.6 8.6 0 1 1 9.4 4.1a6.7 6.7 0 0 0 10.6 10.5z" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round" />,
  sun: (
    <g fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round">
      <circle cx="12" cy="12" r="4.4" />
      <path d="M12 2.6v2.6M12 18.8v2.6M2.6 12h2.6M18.8 12h2.6M5.4 5.4l1.9 1.9M16.7 16.7l1.9 1.9M5.4 18.6l1.9-1.9M16.7 7.3l1.9-1.9" />
    </g>
  ),
};

const MOTIONS: Record<DoodleMotion, string> = {
  twinkle: 'animate-twinkle',
  drift: 'animate-drift',
  sway: 'animate-sway',
  spin: 'animate-spin',
};

/** A layer of doodles behind the content; on phones only the ones marked to keep. */
export function Doodles({ doodles }: { doodles: Doodle[] }) {
  return (
    <div className="pointer-events-none absolute inset-0 z-0" aria-hidden="true">
      {doodles.map((d, index) => (
        <span
          key={index}
          className={cx('absolute', !d.keep && 'max-sm:hidden')}
          style={{
            left: `${d.x.toFixed(2)}%`,
            top: `${Math.round(d.y)}px`,
            width: `${d.size}px`,
            height: `${d.size}px`,
            transform: `rotate(${d.rot}deg)`,
            color: `var(${d.gold ? '--doodle-gold' : '--doodle'})`,
          }}
        >
          <span
            className={cx('block size-full', MOTIONS[d.motion], 'motion-reduce:animate-none')}
            style={{ animationDelay: `${d.delay.toFixed(2)}s`, animationDuration: `${d.duration.toFixed(1)}s` }}
          >
            <svg className="block size-full" viewBox="0 0 24 24">
              {PATHS[d.kind]}
            </svg>
          </span>
        </span>
      ))}
    </div>
  );
}

const box = (r: DOMRect, origin: DOMRect, margin: number): Box => ({
  left: r.left - origin.left - margin,
  right: r.right - origin.left + margin,
  top: r.top - origin.top - margin,
  bottom: r.bottom - origin.top + margin,
});

/** The doodles for the hero's layer and the page's, placed once the page is laid out and again
 *  when it resizes or the web fonts reflow its text. The hero's content boxes are marked
 *  `data-doodle-clear` with the margin to keep; the page's are the sections' blocks. */
export function useDoodles(page: RefObject<HTMLElement | null>) {
  const [doodles, setDoodles] = useState<{ hero: Doodle[]; page: Doodle[] }>({ hero: [], page: [] });
  useEffect(() => {
    const root = page.current;
    if (!root) throw new Error('the doodles have no page');
    const draw = () => {
      const hero = root.querySelector<HTMLElement>('[data-hero]');
      if (!hero) throw new Error('the doodles have no hero');
      const heroBox = hero.getBoundingClientRect();
      const heroHeight = hero.offsetHeight;
      const pageHeight = root.offsetHeight;
      const clear = [...hero.querySelectorAll<HTMLElement>('[data-doodle-clear]')].map((node) =>
        box(node.getBoundingClientRect(), heroBox, Number(node.dataset.doodleClear)),
      );
      const all = placeDoodles(pageHeight, { width: heroBox.width, height: heroHeight, clear });
      const pageBox = root.getBoundingClientRect();
      const content = [...root.querySelectorAll('main > section:not([data-dusk]) > *, main > [data-dusk], main > footer')].map((node) =>
        box(node.getBoundingClientRect(), pageBox, 12),
      );
      const offContent = (d: Doodle) => {
        const x = (d.x / 100) * pageBox.width;
        return !content.some((r) => x + d.size > r.left && x < r.right && d.y + d.size > r.top && d.y < r.bottom);
      };
      setDoodles({
        hero: all.filter((d) => d.y + d.size < heroHeight),
        page: all.filter((d) => d.y > heroHeight && d.y + d.size < pageHeight - 24 && offContent(d)),
      });
    };
    draw();
    let timer: ReturnType<typeof setTimeout>;
    const redraw = () => {
      clearTimeout(timer);
      timer = setTimeout(draw, 200);
    };
    window.addEventListener('resize', redraw);
    // The web fonts arriving reflow the text, which the doodles keep clear of.
    const reflow = new ResizeObserver(redraw);
    root.querySelectorAll('[data-hero] [data-doodle-clear]').forEach((node) => reflow.observe(node));
    reflow.observe(root);
    return () => {
      clearTimeout(timer);
      window.removeEventListener('resize', redraw);
      reflow.disconnect();
    };
  }, [page]);
  return doodles;
}
