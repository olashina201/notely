import * as React from 'react';
import type { SelectorBegin } from '../types';

export interface SelRect {
  x: number;
  y: number;
  w: number;
  h: number;
}

interface Pt {
  x: number;
  y: number;
}

/**
 * Fullscreen crosshair selector.
 * Reports the drag rect in global capture coordinates (the same space as
 * `SelectorBegin.monitor`, which the backend resolves to xcap space).
 * Fractions of the covering window — unit-free, so HiDPI can't skew it.
 */
export function RegionSelector({ sel, onDone }: { sel: SelectorBegin; onDone: (r: SelRect | null) => void }) {
  const [start, setStart] = React.useState<Pt | null>(null);
  const [cur, setCur] = React.useState<Pt | null>(null);
  const doneRef = React.useRef(onDone);
  doneRef.current = onDone;

  React.useEffect(() => {
    const h = (e: KeyboardEvent) => {
      if (e.key === 'Escape') doneRef.current(null);
    };
    window.addEventListener('keydown', h);
    return () => window.removeEventListener('keydown', h);
  }, []);

  function toCapture(a: Pt, b: Pt): SelRect | null {
    const vw = window.innerWidth || 1;
    const vh = window.innerHeight || 1;
    const fx0 = Math.min(a.x, b.x) / vw;
    const fx1 = Math.max(a.x, b.x) / vw;
    const fy0 = Math.min(a.y, b.y) / vh;
    const fy1 = Math.max(a.y, b.y) / vh;
    const w = Math.round((fx1 - fx0) * sel.monitor.w);
    const h = Math.round((fy1 - fy0) * sel.monitor.h);
    if (w < 2 || h < 2) return null;
    return {
      x: Math.round(sel.monitor.x + fx0 * sel.monitor.w),
      y: Math.round(sel.monitor.y + fy0 * sel.monitor.h),
      w,
      h,
    };
  }

  const box =
    start && cur
      ? {
          left: Math.min(start.x, cur.x),
          top: Math.min(start.y, cur.y),
          width: Math.abs(start.x - cur.x),
          height: Math.abs(start.y - cur.y),
        }
      : null;

  return (
    <div
      className="fixed inset-0 z-50 select-none"
      style={{ cursor: 'crosshair', background: 'rgba(0,0,0,0.35)' }}
      onMouseDown={(e) => {
        if (e.button !== 0) return;
        const p = { x: e.clientX, y: e.clientY };
        setStart(p);
        setCur(p);
      }}
      onMouseMove={(e) => {
        if (start) setCur({ x: e.clientX, y: e.clientY });
      }}
      onMouseUp={(e) => {
        if (!start) return;
        const end = { x: e.clientX, y: e.clientY };
        setStart(null);
        setCur(null);
        doneRef.current(toCapture(start, end));
      }}
      onContextMenu={(e) => {
        e.preventDefault();
        setStart(null);
        setCur(null);
        doneRef.current(null);
      }}
    >
      <div className="absolute top-3 left-1/2 -translate-x-1/2 px-3 py-1.5 rounded-lg bg-black/70 border border-white/15 text-[12px] text-white/85 pointer-events-none whitespace-nowrap">
        Drag to select a region · Esc or right-click to cancel
      </div>
      {box && (
        <div
          className="absolute border-2 border-sky-400 bg-sky-400/10 pointer-events-none"
          style={{ left: box.left, top: box.top, width: box.width, height: box.height }}
        />
      )}
    </div>
  );
}
