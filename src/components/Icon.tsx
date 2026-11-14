import type { CSSProperties } from 'react';

const paths: Record<string, string> = {
  search: 'M21 21l-5-5 M18 10a8 8 0 1 1-16 0 8 8 0 0 1 16 0Z',
  refresh: 'M20 7a9 9 0 1 0 1 8 M20 2v5h-5',
  trash: 'M3 6h18 M9 6V3h6v3 M5 6l1 15h12l1-15 M10 10v7 M14 10v7',
  warning: 'm12 3 10 18H2L12 3Z M12 9v5 M12 17h.01',
  activity: 'M3 12h4l3-8 4 16 3-8h4',
  help: 'M9.5 8.5a2.5 2.5 0 1 1 4 2c-1 .7-1.5 1-1.5 2 M12 16h.01 M22 12a10 10 0 1 1-20 0 10 10 0 0 1 20 0Z',
  'eye-off': 'm3 3 18 18 M10.6 10.6a2 2 0 0 0 2.8 2.8 M9.8 5.3A11 11 0 0 1 12 5c6 0 10 7 10 7a17 17 0 0 1-3.1 3.8 M6.2 6.2C3.6 8.1 2 12 2 12s4 7 10 7a11 11 0 0 0 5.8-1.8',
  expand: 'M8 3H3v5 M16 3h5v5 M21 16v5h-5 M8 21H3v-5',
  code: 'm8 6-6 6 6 6 M16 6l6 6-6 6 M14 3l-4 18',
  pen: 'm16 3 5 5-12 12-6 1 1-6L16 3Z M13 6l5 5',
  notebook: 'M7 3h11a2 2 0 0 1 2 2v14a2 2 0 0 1-2 2H7a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2Z M9 3v18 M3 7h4 M3 12h4 M3 17h4 M12 8h5 M12 12h4',
  layers: 'm12 3 9 5-9 5-9-5 9-5Z M3 12l9 5 9-5 M3 16l9 5 9-5',
  timer: 'M9 2h6 M12 2v3 M19 5l2 2 M12 9v5l3 2 M20 14a8 8 0 1 1-16 0 8 8 0 0 1 16 0Z',
  briefcase: 'M9 6V3h6v3 M4 6h16v14H4V6Z M4 11c5 4 11 4 16 0 M10 12v3h4v-3',
  screen: 'M4 4h16v12H4z M8 20h8 M12 16v4',
  chat: 'M21 11.5a8.5 8.5 0 0 1-8.5 8.5H4l-2 2V11.5a9.5 9.5 0 0 1 19 0Z M7 9h9 M7 13h6',
  notes: 'M8 3H5v18h14V3h-3 M8 2h8v4H8z M8 11h8 M8 15h5',
  history: 'M3 11a9 9 0 1 1 2 7 M3 4v7h7 M12 7v5l3 2',
  settings: 'M19.45 10.52 L21.40 12.00 L21.22 13.83 L19.02 14.91 L18.32 16.22 L18.65 18.65 L17.22 19.82 L14.91 19.02 L13.48 19.45 L12.00 21.40 L10.17 21.22 L9.09 19.02 L7.78 18.32 L5.35 18.65 L4.18 17.22 L4.98 14.91 L4.55 13.48 L2.60 12.00 L2.78 10.17 L4.98 9.09 L5.68 7.78 L5.35 5.35 L6.78 4.18 L9.09 4.98 L10.52 4.55 L12.00 2.60 L13.83 2.78 L14.91 4.98 L16.22 5.68 L18.65 5.35 L19.82 6.78 L19.02 9.09Z M15.2 12a3.2 3.2 0 1 1-6.4 0 3.2 3.2 0 0 1 6.4 0Z',
  shield: 'm12 3 8 3v6c0 5-8 9-8 9s-8-4-8-9V6l8-3Z M8 12l3 3 5-6',
  hide: 'm5 9 7 7 7-7',
  arrow: 'M12 19V5 M6 11l6-6 6 6',
  copy: 'M9 9h11v12H9z M15 9V3H3v12h6',
  plus: 'M12 5v14 M5 12h14',
  check: 'm5 12 4 4L19 6',
  close: 'm6 6 12 12 M6 18 18 6',
  region: 'M8 3H3v5 M16 3h5v5 M21 16v5h-5 M8 21H3v-5 M8 8h8v8H8z',
  mic: 'M9 5a3 3 0 0 1 6 0v7a3 3 0 0 1-6 0V5Z M5 10v2a7 7 0 0 0 14 0v-2 M12 19v3 M8 22h8',
  wave: 'M3 10v4 M7 6v12 M12 3v18 M17 6v12 M21 10v4',
  bolt: 'm13 2-9 12h7l-1 8 10-13h-8l1-7Z',
  more: 'M5 12h.01 M12 12h.01 M19 12h.01',
};
export function Icon({ name, size = 18, style }: { name: string; size?: number; style?: CSSProperties }) {
  return <svg width={size} height={size} viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.65" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true" style={style}><path d={paths[name] ?? paths.notebook} /></svg>;
}
