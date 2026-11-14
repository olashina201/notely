import ghost from '../assets/ghost.svg?raw';

// Use the same silhouette as the native app and tray assets.
export function GhostMark({ size = 24 }: { size?: number }) {
  return <span className="ghost-mark" style={{ width: size, height: size }} aria-hidden="true" dangerouslySetInnerHTML={{ __html: ghost }} />;
}
