import * as React from 'react';
import { getCurrentWindow, currentMonitor, LogicalSize } from '@tauri-apps/api/window';
import { Icon } from './Icon';

type Direction = 'East' | 'North' | 'NorthEast' | 'NorthWest' | 'South' | 'SouthEast' | 'SouthWest' | 'West';
export function WindowResize({ disabled, onError }: { disabled: boolean; onError: (message: string) => void }) {
  const [open, setOpen] = React.useState(false);
  const errorRef = React.useRef(onError);
  errorRef.current = onError;
  React.useEffect(() => {
    if (disabled) { setOpen(false); return; }
    let disposed = false;
    let stop: (() => void) | undefined;
    let timer: ReturnType<typeof setTimeout> | undefined;
    const win = getCurrentWindow();
    void (async () => {
      try {
        const raw = localStorage.getItem('notely.window-size');
        if (raw) {
          const saved = JSON.parse(raw);
          if (Number.isFinite(saved.width) && Number.isFinite(saved.height)) await applySize(saved.width, saved.height);
        }
        if (disposed) return;
        const unlisten = await win.onResized(() => {
          clearTimeout(timer);
          timer = setTimeout(() => {
            void (async () => {
              const size = (await win.innerSize()).toLogical(await win.scaleFactor());
              if (!disposed) localStorage.setItem('notely.window-size', JSON.stringify({ width: size.width, height: size.height }));
            })().catch(() => {});
          }, 400);
        });
        if (disposed) unlisten(); else stop = unlisten;
      } catch { /* Resize dragging remains available if saved dimensions cannot be restored. */ }
    })();
    return () => { disposed = true; clearTimeout(timer); stop?.(); };
  }, [disabled]);

  async function applySize(width: number, height: number) {
    const monitor = await currentMonitor();
    const maxWidth = monitor ? monitor.workArea.size.width / monitor.scaleFactor - 16 : 1400;
    const maxHeight = monitor ? monitor.workArea.size.height / monitor.scaleFactor - 16 : 900;
    await getCurrentWindow().setSize(new LogicalSize(Math.min(maxWidth, Math.max(380, width)), Math.min(maxHeight, Math.max(360, height))));
  }
  async function resize(direction: Direction, event: React.PointerEvent) {
    if (event.button !== 0) return;
    event.preventDefault(); event.stopPropagation();
    try { await getCurrentWindow().startResizeDragging(direction); }
    catch { errorRef.current('Could not resize the window. Try a size preset from the resize button.'); }
  }
  if (disabled) return null;
  return <>
    <div className="size-control" data-nodrag>
      <button className="icon-button" aria-label="Window size" title="Window size · drag any edge to resize" aria-expanded={open} onClick={() => setOpen(!open)}><Icon name="expand" size={16} /></button>
      {open && <><button className="popover-backdrop" aria-label="Close window size menu" onClick={() => setOpen(false)} /><div className="size-menu" role="group" aria-label="Window size presets"><strong>Make room for your thoughts</strong>{[['Compact', 620, 480], ['Comfortable', 820, 600], ['Spacious', 1040, 760]].map(([label, w, h]) => <button key={label} onClick={() => { setOpen(false); void applySize(Number(w), Number(h)).catch(() => onError('Could not change the window size.')); }}><span>{label}</span><small>{w} × {h}</small></button>)}<p>You can also drag any edge or corner.</p></div></>}
    </div>
    {(['North', 'South', 'East', 'West', 'NorthEast', 'NorthWest', 'SouthWest'] as Direction[]).map((direction) => <div key={direction} className="resize-edge" data-direction={direction} data-nodrag aria-hidden="true" onPointerDown={(e) => void resize(direction, e)} />)}
    <button className="resize-corner" data-nodrag aria-label="Resize window" title="Drag to resize · arrow keys to adjust" onPointerDown={(e) => void resize('SouthEast', e)} onKeyDown={(e) => {
      if (!['ArrowUp', 'ArrowDown', 'ArrowLeft', 'ArrowRight'].includes(e.key)) return;
      e.preventDefault(); const key = e.key; const delta = e.shiftKey ? 40 : 10;
      void (async () => { const win = getCurrentWindow(); const size = (await win.innerSize()).toLogical(await win.scaleFactor()); await applySize(size.width + (key === 'ArrowRight' ? delta : key === 'ArrowLeft' ? -delta : 0), size.height + (key === 'ArrowDown' ? delta : key === 'ArrowUp' ? -delta : 0)); })().catch(() => onError('Could not resize the window.'));
    }}><svg width="12" height="12" viewBox="0 0 12 12" aria-hidden="true"><path d="M3 10 10 3M7 10l3-3" stroke="currentColor" strokeWidth="1.4" strokeLinecap="round" /></svg></button>
  </>;
}
