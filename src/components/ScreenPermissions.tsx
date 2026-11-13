import * as React from 'react';
import { commands } from '../api';

export function ScreenPermissions() {
  const [permission, setPermission] = React.useState<{ granted: boolean; platform: string } | null>(null);
  const [busy, setBusy] = React.useState(false);
  const [message, setMessage] = React.useState('');
  React.useEffect(() => {
    let active = true;
    commands.screenPermissionGet().then((p) => { if (active) setPermission(p); }).catch((e) => { if (active) setMessage(String(e)); });
    return () => { active = false; };
  }, []);

  async function action(kind: 'request' | 'refresh' | 'settings' | 'test') {
    setBusy(true);
    setMessage('');
    try {
      if (kind === 'request') {
        const p = await commands.screenPermissionRequest();
        setPermission(p);
        setMessage(p.granted ? 'Screen capture access is available.' : 'macOS has not confirmed access for this running build. If access is already enabled, try Test screenshot. A changed signing identity may require selecting this app again in macOS settings.');
      } else if (kind === 'refresh') {
        const p = await commands.screenPermissionGet();
        setPermission(p);
        if (!p.granted) setMessage('The preflight check is inconclusive for this build. Test screenshot verifies access through ScreenCaptureKit itself.');
      } else if (kind === 'settings') {
        await commands.screenPermissionOpenSettings();
        setMessage('Restore Notely from its menu-bar icon after changing the macOS permission.');
      } else {
        const image = await commands.captureFull();
        setPermission(await commands.screenPermissionGet());
        setMessage(`Screenshot captured: ${image.width} × ${image.height}. It is attached to Assist; you can clear it there.`);
      }
    } catch (e) { setMessage(String(e)); }
    finally { setBusy(false); }
  }

  const buttonClass = 'px-2.5 py-1 rounded-lg bg-white/10 hover:bg-white/10 disabled:opacity-40 text-[12px]';
  return <section className="space-y-2">
    <div className="text-[11px] uppercase tracking-wide text-white/65">Screen capture</div>
    <div className={permission?.granted ? 'text-emerald-300' : 'text-amber-300'}>
      {permission ? permission.granted ? 'Screen capture access available' : 'Screen capture access not confirmed' : 'Checking access…'}
    </div>
    <div className="text-[11.5px] text-white/65">An enabled macOS permission can refer to an earlier build. Test screenshot checks the running app directly. Notely hides while permission controls open; return using its menu-bar icon. This permission is separate from screen-share protection.</div>
    <div className="flex gap-1.5 flex-wrap">
      <button disabled={busy || permission?.granted} onClick={() => void action('request')} className={buttonClass}>Request access</button>
      <button disabled={busy} onClick={() => void action('settings')} className={buttonClass}>Open macOS settings</button>
      <button disabled={busy} onClick={() => void action('refresh')} className={buttonClass}>Check access</button>
      <button disabled={busy} onClick={() => void action('test')} className={buttonClass}>{busy ? 'Working…' : 'Test screenshot'}</button>
    </div>
    {message && <div role="status" className="text-[11.5px] text-white/90 break-words">{message}</div>}
  </section>;
}
