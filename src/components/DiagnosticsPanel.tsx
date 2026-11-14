import * as React from 'react';
import { commands } from '../api';
import type { DiagnosticEntry, DiagnosticsSnapshot } from '../types';
import { Icon } from './Icon';

type Filter = 'all' | 'issues' | 'requests' | 'audio';
const labels: Record<string, string> = {
  'audio.status': 'Meeting audio', 'audio.error': 'Meeting audio error',
  'request.failed': 'Assistant request failed', 'request.started': 'Assistant request started',
  'request.finished': 'Assistant request finished', 'request.completed': 'Assistant request completed',
  'ui.request_failed': 'Assistant reported an error', 'capture.permission_denied': 'Screen capture access denied',
  'credentials.lookup_started': 'Checking API credentials', 'credentials.lookup_finished': 'API credentials checked',
  'app.started': 'Notely opened', 'meeting.saved': 'Meeting saved',
};
const readable = (text: string) => text.replace(/[._]/g, ' ').replace(/([a-z])([A-Z])/g, '$1 $2').replace(/^./, c => c.toUpperCase());
const title = (entry: DiagnosticEntry) => labels[entry.stage] || readable(entry.stage);
const isIssue = (entry: DiagnosticEntry) => ['error', 'warn', 'warning'].includes(entry.level);
const providers: Record<string, string> = { openai: 'OpenAI', claude: 'Claude', gemini: 'Gemini' };
const fieldLabel: Record<string, string> = { elapsedMs: 'Elapsed time', providerRequestId: 'Provider request ID', failureKind: 'Failure category', imageBytes: 'Image bytes', imageBytesBefore: 'Original image bytes', payloadBytes: 'Payload bytes', includeScreenshot: 'Screenshot included', jsonMode: 'JSON mode' };
function valueText(key: string, value: unknown) {
  if (key === 'provider' && typeof value === 'string') return providers[value] || value;
  if (['failureKind', 'kind', 'status'].includes(key) && typeof value === 'string') return readable(value);
  if (key === 'elapsedMs' && typeof value === 'number') return `${(value / 1000).toFixed(2)} seconds`;
  if (key === 'status' && typeof value === 'number') return `HTTP ${value}`;
  if (typeof value === 'boolean') return value ? 'Yes' : 'No';
  return value === null ? '—' : String(value);
}

export function DiagnosticsPanel() {
  const [snapshot, setSnapshot] = React.useState<DiagnosticsSnapshot | null>(null);
  const [error, setError] = React.useState('');
  const [autoRefresh, setAutoRefresh] = React.useState(true);
  const [query, setQuery] = React.useState('');
  const [filter, setFilter] = React.useState<Filter>('all');
  const [copied, setCopied] = React.useState(false);
  const [busy, setBusy] = React.useState(false);
  const [updated, setUpdated] = React.useState<Date | null>(null);
  const [notice, setNotice] = React.useState('');
  const generation = React.useRef(0);
  const refresh = React.useCallback(async () => {
    const version = ++generation.current;
    try {
      const result = await commands.diagnosticsRecent();
      if (version === generation.current) { setSnapshot(result); setUpdated(new Date()); setError(''); }
    } catch (e) { if (version === generation.current) setError(String(e)); }
  }, []);
  React.useEffect(() => {
    void refresh();
    const interval = autoRefresh ? setInterval(() => void refresh(), 3000) : null;
    return () => { generation.current++; if (interval) clearInterval(interval); };
  }, [autoRefresh, refresh]);
  React.useEffect(() => {
    if (!copied) return;
    const timer = setTimeout(() => setCopied(false), 2500);
    return () => clearTimeout(timer);
  }, [copied]);

  const entries = snapshot?.entries ?? [];
  const issues = entries.filter(isIssue).length;
  const matches: Record<Filter, (entry: DiagnosticEntry) => boolean> = {
    all: () => true, issues: isIssue, requests: entry => !!entry.requestId,
    audio: entry => /^(audio|meeting|transcript)\./.test(entry.stage),
  };
  const rows = entries.filter(entry => matches[filter](entry) && (!query.trim() || `${title(entry)} ${JSON.stringify(entry)}`.toLowerCase().includes(query.trim().toLowerCase())));
  const text = rows.map(entry => JSON.stringify(entry)).join('\n');
  const occurrences = new Map<string, number>();

  return <div className="diagnostics-panel">
    <div className="diagnostics-intro"><p>See what happened, and where something needs a hand.</p><span>Only technical activity is logged. Keys, prompts, screenshots, and answers stay out of these logs.</span></div>
    <div className="diagnostics-overview"><div><strong>{entries.length}</strong><span>recent events</span><span className="diagnostics-divider" /><strong className={issues ? 'has-issues' : ''}>{issues}</strong><span>{issues === 1 ? 'issue' : 'issues'} in this log</span></div><label className="diagnostics-live"><input type="checkbox" checked={autoRefresh} onChange={e => setAutoRefresh(e.target.checked)} /><span className={autoRefresh ? 'live-dot active' : 'live-dot'} />Live updates</label></div>
    <div className="diagnostics-toolbar">
      <label className="diagnostics-search"><Icon name="search" size={16} /><input aria-label="Filter diagnostics" placeholder="Search activity, provider, or request ID…" value={query} onChange={e => { setQuery(e.target.value); setCopied(false); }} /></label>
      <div className="diagnostics-tools"><button disabled={busy} onClick={async () => { setBusy(true); await refresh(); setBusy(false); }}><Icon name="refresh" size={14} />Refresh</button><button disabled={!text} onClick={async () => {
        try { await navigator.clipboard.writeText(text); setCopied(true); }
        catch { setError('Clipboard is unavailable. Expand an event to select its details or use the log file below.'); }
      }}><Icon name={copied ? 'check' : 'copy'} size={14} />{copied ? 'Copied' : 'Copy visible'}</button><button className="diagnostics-clear" disabled={busy || !entries.length} onClick={async () => {
        setBusy(true); generation.current++;
        try { await commands.diagnosticsClear(); await refresh(); setNotice('Logs cleared.'); }
        catch (e) { setError(String(e)); }
        finally { setBusy(false); }
      }}><Icon name="trash" size={14} />Clear logs</button></div>
    </div>
    <div className="diagnostics-filterbar"><div className="diagnostics-filters" role="group" aria-label="Activity type">{(['all', 'issues', 'requests', 'audio'] as Filter[]).map(id => <button key={id} aria-pressed={filter === id} onClick={() => { setFilter(id); setCopied(false); }}>{id === 'all' ? 'All activity' : id === 'issues' ? `Issues${issues ? ` (${issues})` : ''}` : id === 'requests' ? 'AI requests' : 'Meeting & audio'}</button>)}</div><span>{rows.length} shown · newest first</span></div>
    {(error || snapshot?.writeError) && <div role="alert" className="diagnostics-alert"><Icon name="warning" size={17} /><span>{error || snapshot?.writeError}</span></div>}
    <div className="diagnostics-events" aria-label="Diagnostic events">
      {!snapshot && !error && <div className="diagnostics-empty">Loading recent activity…</div>}
      {snapshot && !rows.length && <div className="diagnostics-empty"><Icon name={filter === 'issues' ? 'check' : 'search'} size={24} /><strong>{entries.length ? 'No matching events' : 'A fresh start'}</strong><p>{entries.length ? 'Try another filter or search term.' : 'Run a request or start a meeting. Its technical activity will appear here.'}</p>{(query || filter !== 'all') && <button onClick={() => { setQuery(''); setFilter('all'); }}>Show all activity</button>}</div>}
      {rows.map(entry => {
        const identity = JSON.stringify(entry);
        const occurrence = occurrences.get(identity) ?? 0;
        occurrences.set(identity, occurrence + 1);
        const severity = entry.level === 'error' ? 'error' : isIssue(entry) ? 'warning' : 'info';
        const status = entry.fields.status;
        const provider = typeof entry.fields.provider === 'string' ? providers[entry.fields.provider] || entry.fields.provider : null;
        return <details key={`${identity}-${occurrence}`} className={`diagnostic-event ${severity}`}>
          <summary><span className="diagnostic-severity"><Icon name={severity === 'info' ? 'activity' : 'warning'} size={16} /></span><div className="diagnostic-event-main"><div className="diagnostic-event-title"><strong>{title(entry)}</strong><span className={`diagnostic-badge ${severity}`}>{severity === 'error' ? 'Error' : severity === 'warning' ? 'Warning' : 'Info'}</span></div><div className="diagnostic-meta">{provider && <span className="diagnostic-provider">{provider}</span>}{status !== undefined && <span>{typeof status === 'number' ? `HTTP ${status}` : readable(String(status))}</span>}{typeof entry.fields.kind === 'string' && <span>{readable(entry.fields.kind)}</span>}{typeof entry.fields.elapsedMs === 'number' && <span>{(entry.fields.elapsedMs / 1000).toFixed(2)}s</span>}{entry.requestId && <span className="diagnostic-request">#{entry.requestId.slice(0, 8)}</span>}<code>{entry.stage}</code></div></div><time dateTime={entry.timestamp}>{new Date(entry.timestamp).toLocaleTimeString([], { hour: '2-digit', minute: '2-digit', second: '2-digit' })}</time><span className="diagnostic-chevron"><Icon name="hide" size={14} /></span></summary>
          <div className="diagnostic-details"><dl><div><dt>Recorded</dt><dd>{new Date(entry.timestamp).toLocaleString()}</dd></div>{entry.requestId && <div><dt>Request ID</dt><dd><code>{entry.requestId}</code></dd></div>}{Object.entries(entry.fields).map(([key, value]) => <div key={key}><dt>{fieldLabel[key] || readable(key)}</dt><dd>{valueText(key, value)}</dd></div>)}</dl><details className="diagnostic-raw"><summary>View raw event</summary><pre>{JSON.stringify(entry, null, 2)}</pre></details></div>
        </details>;
      })}
    </div>
    {snapshot && <footer className="diagnostics-footer"><span>{updated ? `Updated ${updated.toLocaleTimeString()}` : 'Waiting for activity'}</span><details><summary>Local log file</summary><code>{snapshot.path}</code></details></footer>}
    <span className="sr-only" role="status">{notice || (copied ? 'Visible logs copied.' : '')}</span>
  </div>;
}
