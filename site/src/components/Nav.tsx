import type { ReactNode } from 'react';
import { cx } from '../lib/cx';

export function Nav({ shiftOn, onShift }: { shiftOn: boolean; onShift: () => void }) {
  return (
    // While the hero's column is clearly wider, the nav keeps to 864px, a step inside the
    // sections' column, and below lg it joins the hero's, the difference being too small to look
    // deliberate. Below md its wide link would push the Night Shift switch onto a second row.
    <nav
      className="flex flex-wrap items-center gap-x-6 gap-y-3 py-6 max-md:gap-x-4 max-sm:flex-nowrap max-sm:gap-1.5 lg:mx-auto lg:w-full lg:max-w-[864px]"
      aria-label="Main"
      data-doodle-clear="12"
    >
      <a className="flex items-center gap-3 font-display text-[30px] leading-none text-ink no-underline hover:text-ink" href="#top">
        <img className="max-sm:size-9" src="icon.svg" alt="Nightcap" width="44" height="44" />
        <span className="max-sm:hidden">Nightcap</span>
      </a>
      <div className="grow" />
      <div className="inline-flex items-center gap-0.5 rounded-full bg-nav-bg p-[3px] shadow-[inset_0_0_0_1px_var(--line)] backdrop-blur-[10px] max-sm:p-0.5">
        <NavLink href="#how" wide>
          How it works
        </NavLink>
        <span className="h-3.5 w-px bg-line max-md:hidden" aria-hidden="true" />
        <NavLink href="https://github.com/choco/Nightcap">
          GitHub
          <svg viewBox="0 0 24 24" width="13" height="13" fill="none" stroke="currentColor" strokeWidth="2.2" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
            <path d="M8 16 16 8M9.5 8H16v6.5" />
          </svg>
        </NavLink>
      </div>
      <button
        className="group inline-flex h-9 cursor-pointer items-center gap-2.5 rounded-full border border-line-strong bg-surface pr-3.5 pl-2 font-text text-[14px] leading-none font-semibold text-ink"
        type="button"
        aria-pressed={shiftOn}
        onClick={onShift}
      >
        <span className="relative h-5 w-[34px] rounded-full bg-line-strong transition-[background-color] duration-[160ms] group-aria-pressed:bg-ember motion-reduce:transition-none">
          <span className="absolute top-0.5 left-0.5 size-4 rounded-full bg-[#ffffff] shadow-[0_1px_2px_rgba(11,19,48,0.3)] transition-[left] duration-[160ms] group-aria-pressed:left-4 motion-reduce:transition-none" />
        </span>
        <span>{shiftOn ? 'Night Shift: On' : 'Night Shift: Off'}</span>
      </button>
    </nav>
  );
}

function NavLink({ href, wide, children }: { href: string; wide?: boolean; children: ReactNode }) {
  return (
    <a
      className={cx(
        'inline-flex h-[30px] items-center gap-1 rounded-full px-3 text-[14px] font-semibold text-ink no-underline transition-[background-color] duration-[160ms] hover:bg-nav-hover hover:text-ink motion-reduce:transition-none max-sm:h-7 max-sm:px-[9px] max-sm:text-[13px]',
        wide && 'max-md:hidden',
      )}
      href={href}
    >
      {children}
    </a>
  );
}
