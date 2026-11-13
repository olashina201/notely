import * as React from 'react';
import { commands } from '../api';
import type { DiagnosticsSnapshot } from '../types';

export function DiagnosticsPanel() {
  const [snapshot, setSnapshot] = React.useState<DiagnosticsSnapshot | null>(null);
  const [error, setError] = React.useState('');
  const [autoRefresh, setAutoRefresh] = React.useState(true);
  const [query, setQuery] = React.useState('');
  const [copied, setCopied] = React.useState(false);
  const refresh = React.useCallback(async () => {
    try { setSnapshot(await commands.diagnosticsRecent()); setError(''); }
    catch (e) { setError(String(e)); }
  }, []);

  React.useEffect(() => {
    let active = true;
    const update = async () => {
      try { const result = await commands.diagnosticsRecent(); if (active) { setSnapshot(result); setError(''); } }
      catch (e) { if (active) setError(String(e)); }
    };
    void update();
    const interval = autoRefresh ? setInterval(() => void update(), 3000) : null;
    return () => { active = false; if (interval) clearInterval(interval); };
  }, [autoRefresh]);

  const rows = snapshot?.entries.filter((entry) => !query || JSON.stringify(entry).toLowerCase().includes(query.toLowerCase())) ?? [];
  const text = rows.map((entry) => JSON.stringify(entry)).join('\n');

  return (
    <div className="flex flex-col min-h-0 px-3 py-2 gap-2 text-[12px] text-white/90">
      <div className="text-white/65">Local diagnostics record request stages, timings, HTTP status, and failures. Keys, prompts, screenshots, and answer text are excluded.</div>
      {snapshot && <div className="text-[11px] text-white/65 break-all select-text">Log file: {snapshot.path}</div>}
      <div className="flex items-center gap-2 flex-wrap">
        <button onClick={() => void refresh()} className="rounded bg-white/10 px-2 py-1 hover:bg-white/10">Refresh</button>
        <button disabled={!text} onClick={async () => {
          try { await navigator.clipboard.writeText(text); setCopied(true); }
          catch { setError('Clipboard is unavailable. Expand an entry to select its text, or read the log file above.'); }
        }} className="rounded bg-white/10 px-2 py-1 hover:bg-white/10 disabled:opacity-40">{copied ? 'Copied' : 'Copy shown logs'}</button>
        <button onClick={async () => {
          try { await commands.diagnosticsClear(); await refresh(); }
          catch (e) { setError(String(e)); }
        }} className="rounded bg-white/10 px-2 py-1 hover:bg-white/10">Clear logs</button>
        <label className="flex items-center gap-1 text-white/65"><input type="checkbox" checked={autoRefresh} onChange={(e) => setAutoRefresh(e.target.checked)} />Live refresh</label>
      </div>
      <input aria-label="Filter diagnostics" placeholder="Filter by request ID, provider, stage, or status…" value={query}
        onChange={(e) => { setQuery(e.target.value); setCopied(false); }} className="bg-white/10 rounded-lg px-2 py-1.5 outline-none" />
      {(error || snapshot?.writeError) && <div role="alert" className="text-red-300 break-words">{error || snapshot?.writeError}</div>}
      <div className="overflow-y-auto min-h-0 max-h-64 space-y-1">
        {snapshot && rows.length === 0 && <div className="text-white/50 py-3">No matching activity. Run a request, then check here.</div>}
        {rows.map((entry, i) => (
          <details key={`${entry.timestamp}-${entry.stage}-${i}`} className={`rounded px-2 py-1.5 border ${entry.level === 'error' ? 'border-red-400/25 bg-red-500/10' : 'border-white/12 bg-white/5'}`}>
            <summary className="cursor-pointer break-words">
              <span className="text-white/50 mr-2">{new Date(entry.timestamp).toLocaleTimeString()}</span>
              <span className={entry.level === 'error' ? 'text-red-300' : 'text-white/90'}>{entry.stage}</span>
              {typeof entry.fields.provider === 'string' && <span className="ml-2 text-sky-300">{entry.fields.provider}</span>}
              {typeof entry.fields.status === 'number' && <span className="ml-2">HTTP {entry.fields.status}</span>}
              {typeof entry.fields.elapsedMs === 'number' && <span className="ml-2 text-white/50">{(entry.fields.elapsedMs / 1000).toFixed(2)}s</span>}
              {entry.requestId && <span className="ml-2 text-white/50 font-mono">#{entry.requestId.slice(0, 8)}</span>}
            </summary>
            <pre className="select-text whitespace-pre-wrap break-all mt-1.5 text-[11px] text-white/65">{JSON.stringify(entry, null, 2)}</pre>
          </details>
        ))}
      </div>
    </div>
  );
}
