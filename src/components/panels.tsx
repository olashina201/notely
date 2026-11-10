import * as React from 'react';
import type { AppSettings, MeetingRow, ModelInfo, NotesJson, TranscriptLine } from '../types';
import { HOTKEY_ACTIONS, PROFILES } from '../types';

// ---------- Settings ----------

export function SettingsPanel({
  settings,
  keyHas,
  models,
  modelsLoading,
  keyMsg,
  onSave,
  onLoadModels,
  onSetKey,
  onClearKey,
  onTestKey,
}: {
  settings: AppSettings;
  keyHas: boolean;
  models: ModelInfo[];
  modelsLoading: boolean;
  keyMsg: string | null;
  onSave: (s: AppSettings) => Promise<void>;
  onLoadModels: () => void;
  onSetKey: (k: string) => Promise<void>;
  onClearKey: () => Promise<void>;
  onTestKey: () => Promise<void>;
}) {
  const [draft, setDraft] = React.useState<AppSettings>(settings);
  const [keyInput, setKeyInput] = React.useState('');
  const [saving, setSaving] = React.useState(false);
  React.useEffect(() => setDraft(settings), [settings]);

  function set<K extends keyof AppSettings>(k: K, v: AppSettings[K]) {
    setDraft((d) => ({ ...d, [k]: v }));
  }

  async function save() {
    setSaving(true);
    try {
      if (keyInput.trim()) {
        await onSetKey(keyInput.trim());
        setKeyInput('');
      }
      await onSave(draft);
    } finally {
      setSaving(false);
    }
  }

  const liveModels = models.filter((m) => m.id.toLowerCase().includes('live'));
  const genModels = models.filter((m) => !m.id.toLowerCase().includes('live'));

  return (
    <div className="flex-1 overflow-y-auto px-3 py-2 space-y-4 text-[12.5px] text-white/85">
      {/* API key */}
      <section className="space-y-1.5">
        <div className="text-[11px] uppercase tracking-wide text-white/45">Gemini API key</div>
        <div className="flex items-center gap-2">
          <span className={`inline-block w-2 h-2 rounded-full ${keyHas ? 'bg-emerald-400' : 'bg-red-400'}`} />
          <span className="text-white/70">{keyHas ? 'Key stored in OS keychain' : 'No key set'}</span>
        </div>
        <div className="flex gap-1.5">
          <input
            type="password"
            value={keyInput}
            onChange={(e) => setKeyInput(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === 'Enter' && keyInput.trim()) {
                e.preventDefault();
                void onSetKey(keyInput.trim()).then(() => setKeyInput(''));
              }
            }}
            placeholder="AIza…"
            className="flex-1 min-w-0 bg-white/10 border border-white/15 rounded-lg px-2 py-1.5 text-[12px] outline-none focus:border-sky-400/60"
          />
          <button
            onClick={() => {
              if (keyInput.trim()) {
                void onSetKey(keyInput.trim()).then(() => setKeyInput(''));
              }
            }}
            className="px-2.5 py-1.5 rounded-lg bg-sky-500/80 hover:bg-sky-500 text-white text-[12px]"
          >
            Save
          </button>
        </div>
        <div className="flex gap-1.5">
          <button onClick={onTestKey} className="px-2.5 py-1 rounded-lg bg-white/10 hover:bg-white/15 border border-white/10 text-[12px]">
            Test key
          </button>
          <button onClick={onClearKey} className="px-2.5 py-1 rounded-lg bg-white/10 hover:bg-white/15 border border-white/10 text-[12px]">
            Remove
          </button>
          <button onClick={onLoadModels} disabled={modelsLoading} className="px-2.5 py-1 rounded-lg bg-white/10 hover:bg-white/15 border border-white/10 text-[12px] disabled:opacity-50">
            {modelsLoading ? 'Loading…' : 'Load models'}
          </button>
        </div>
        {keyMsg && <div className="text-[11.5px] text-white/60 break-words">{keyMsg}</div>}
      </section>

      {/* Models */}
      <section className="space-y-1.5">
        <div className="text-[11px] uppercase tracking-wide text-white/45">Models</div>
        <ModelField label="Live transcription" value={draft.liveModel} options={liveModels.length ? liveModels : models} onChange={(v) => set('liveModel', v)} />
        <ModelField label="Fast assist" value={draft.fastModel} options={genModels.length ? genModels : models} onChange={(v) => set('fastModel', v)} />
        <ModelField label="Smart assist / notes" value={draft.smartModel} options={genModels.length ? genModels : models} onChange={(v) => set('smartModel', v)} />
        <label className="flex items-center gap-2 pt-1">
          <span className="text-white/60 w-32">Language</span>
          <input value={draft.language} onChange={(e) => set('language', e.target.value)}
            className="flex-1 min-w-0 bg-white/10 border border-white/15 rounded-lg px-2 py-1 text-[12px] outline-none" />
        </label>
        <label className="flex items-center gap-2">
          <span className="text-white/60 w-32">Thinking</span>
          <select value={draft.thinkingLevel} onChange={(e) => set('thinkingLevel', e.target.value)}
            className="flex-1 bg-white/10 border border-white/15 rounded-lg px-2 py-1 text-[12px] outline-none [&>option]:text-black">
            <option value="off">Off (fastest)</option>
            <option value="low">Low</option>
            <option value="high">High (slower)</option>
          </select>
        </label>
        <Toggle label="Google Search grounding" value={draft.searchGrounding} onChange={(v) => set('searchGrounding', v)} />
      </section>

      {/* Profile */}
      <section className="space-y-1.5">
        <div className="text-[11px] uppercase tracking-wide text-white/45">Assistant profile</div>
        <select value={draft.instructionProfile} onChange={(e) => set('instructionProfile', e.target.value)}
          className="w-full bg-white/10 border border-white/15 rounded-lg px-2 py-1.5 text-[12px] outline-none [&>option]:text-black">
          {PROFILES.map((p) => <option key={p.id} value={p.id}>{p.name}</option>)}
        </select>
        <textarea value={draft.customInstructions} onChange={(e) => set('customInstructions', e.target.value)}
          placeholder="Extra instructions (optional)"
          rows={2}
          className="w-full bg-white/10 border border-white/15 rounded-lg px-2 py-1.5 text-[12px] outline-none resize-none" />
      </section>

      {/* Overlay */}
      <section className="space-y-1.5">
        <div className="text-[11px] uppercase tracking-wide text-white/45">Overlay</div>
        <label className="flex items-center gap-2">
          <span className="text-white/60 w-32">Opacity</span>
          <input type="range" min={0.4} max={1} step={0.05} value={draft.overlayOpacity}
            onChange={(e) => set('overlayOpacity', Number(e.target.value))} className="flex-1" />
          <span className="text-white/50 w-8 text-right">{Math.round(draft.overlayOpacity * 100)}</span>
        </label>
        <Toggle label="Hide from screen share" value={draft.contentProtection} onChange={(v) => set('contentProtection', v)} />
        <Toggle label="Click-through" value={draft.clickThrough} onChange={(v) => set('clickThrough', v)} />
        <Toggle label="Keep screenshots" value={draft.saveCaptures} onChange={(v) => set('saveCaptures', v)} />
        <Toggle label="Auto-attach screenshot" value={draft.autoAttachScreenshot} onChange={(v) => set('autoAttachScreenshot', v)} />
      </section>

      {/* Hotkeys */}
      <section className="space-y-1.5">
        <div className="text-[11px] uppercase tracking-wide text-white/45">Hotkeys</div>
        {HOTKEY_ACTIONS.map((a) => (
          <label key={a.id} className="flex items-center gap-2">
            <span className="text-white/60 flex-1">{a.label}</span>
            <input value={draft.hotkeys[a.id] ?? ''} onChange={(e) => set('hotkeys', { ...draft.hotkeys, [a.id]: e.target.value })}
              spellCheck={false}
              className="w-44 font-mono bg-white/10 border border-white/15 rounded-lg px-2 py-1 text-[11px] outline-none" />
          </label>
        ))}
      </section>

      <button onClick={save} disabled={saving}
        className="w-full py-2 rounded-lg bg-sky-500/80 hover:bg-sky-500 text-white text-[13px] disabled:opacity-50">
        {saving ? 'Saving…' : 'Save settings'}
      </button>
      <div className="h-1" />
    </div>
  );
}

function ModelField({ label, value, options, onChange }: { label: string; value: string; options: ModelInfo[]; onChange: (v: string) => void }) {
  const listId = `models-${label.replace(/\W+/g, '-')}`;
  return (
    <label className="flex items-center gap-2">
      <span className="text-white/60 w-32 shrink-0">{label}</span>
      <input value={value} onChange={(e) => onChange(e.target.value)} list={options.length ? listId : undefined} spellCheck={false}
        className="flex-1 min-w-0 font-mono bg-white/10 border border-white/15 rounded-lg px-2 py-1 text-[11.5px] outline-none" />
      {options.length > 0 && (
        <datalist id={listId}>
          {options.map((m) => <option key={m.id} value={m.id}>{m.displayName || m.id}</option>)}
        </datalist>
      )}
    </label>
  );
}

function Toggle({ label, value, onChange }: { label: string; value: boolean; onChange: (v: boolean) => void }) {
  return (
    <label className="flex items-center gap-2 cursor-pointer">
      <span className="text-white/60 flex-1">{label}</span>
      <button type="button" onClick={() => onChange(!value)}
        className={`w-9 h-5 rounded-full transition-colors relative ${value ? 'bg-sky-500/80' : 'bg-white/15'}`}>
        <span className="block w-4 h-4 rounded-full bg-white absolute top-0.5 transition-all" style={{ left: value ? 18 : 2 }} />
      </button>
    </label>
  );
}

// ---------- Notes ----------

export function NotesPanel({ notes, generating, onGenerate, disabled }: {
  notes: NotesJson | null;
  generating: boolean;
  disabled: boolean;
  onGenerate: () => void;
}) {
  return (
    <div className="flex-1 overflow-y-auto px-3 py-2 space-y-2 min-h-0">
      <button onClick={onGenerate} disabled={generating || disabled}
        className="w-full py-1.5 rounded-lg bg-white/10 hover:bg-white/15 border border-white/10 text-[12.5px] disabled:opacity-50">
        {generating ? 'Generating…' : notes ? 'Regenerate notes' : 'Generate notes from transcript'}
      </button>
      {!notes && <div className="text-[12px] text-white/35 text-center py-4">Stop the meeting, then generate structured notes here.</div>}
      {notes && (
        <div className="space-y-2.5 text-[12.5px] leading-relaxed text-white/85">
          <section>
            <div className="text-[11px] uppercase tracking-wide text-white/45 mb-0.5">Summary</div>
            <p className="whitespace-pre-wrap">{notes.summary}</p>
          </section>
          {notes.decisions.length > 0 && (
            <section>
              <div className="text-[11px] uppercase tracking-wide text-white/45 mb-0.5">Decisions</div>
              <ul className="list-disc pl-5 space-y-0.5">{notes.decisions.map((d, i) => <li key={i}>{d}</li>)}</ul>
            </section>
          )}
          {notes.actionItems.length > 0 && (
            <section>
              <div className="text-[11px] uppercase tracking-wide text-white/45 mb-0.5">Action items</div>
              <ul className="list-disc pl-5 space-y-0.5">
                {notes.actionItems.map((a, i) => <li key={i}>{a.text}{a.owner ? <span className="text-white/50"> — {a.owner}</span> : null}</li>)}
              </ul>
            </section>
          )}
          {notes.followUpEmail && (
            <section>
              <div className="text-[11px] uppercase tracking-wide text-white/45 mb-0.5">Follow-up email</div>
              <div className="response-body bg-white/5 border border-white/10 rounded-lg p-2 whitespace-pre-wrap select-text">{notes.followUpEmail}</div>
            </section>
          )}
        </div>
      )}
    </div>
  );
}

// ---------- History ----------

export function HistoryPanel({ rows, selected, onSearch, onSelect, onDelete, onRefresh, searching }: {
  rows: MeetingRow[];
  selected: { row: MeetingRow; transcript: TranscriptLine[]; notes: NotesJson | null } | null;
  searching: boolean;
  onSearch: (q: string) => void;
  onSelect: (id: number) => void;
  onDelete: (id: number) => void;
  onRefresh: () => void;
}) {
  const [q, setQ] = React.useState('');
  return (
    <div className="flex-1 flex flex-col min-h-0">
      <div className="flex gap-1.5 px-3 py-2 border-b border-white/10">
        <input value={q} onChange={(e) => { setQ(e.target.value); onSearch(e.target.value); }}
          placeholder="Search past meetings…"
          className="flex-1 min-w-0 bg-white/10 border border-white/15 rounded-lg px-2 py-1.5 text-[12px] outline-none" />
        <button onClick={onRefresh} className="px-2.5 rounded-lg bg-white/10 hover:bg-white/15 border border-white/10 text-[12px]">↻</button>
      </div>
      <div className="flex-1 overflow-y-auto min-h-0">
        {searching && <div className="text-[12px] text-white/40 text-center py-3">Searching…</div>}
        {!searching && rows.length === 0 && <div className="text-[12px] text-white/35 text-center py-4">No saved meetings yet.</div>}
        {rows.map((r) => (
          <div key={r.id}
            className={`px-3 py-2 border-b border-white/5 cursor-pointer hover:bg-white/5 ${selected?.row.id === r.id ? 'bg-white/5' : ''}`}
            onClick={() => onSelect(r.id)}>
            <div className="flex items-center justify-between gap-2">
              <span className="text-[12.5px] text-white/85 truncate">{r.title || `Meeting ${new Date(r.startedAt).toLocaleString()}`}</span>
              <button onClick={(e) => { e.stopPropagation(); onDelete(r.id); }}
                className="text-white/35 hover:text-red-300 text-[12px] shrink-0">✕</button>
            </div>
            <div className="text-[11px] text-white/40">
              {new Date(r.startedAt).toLocaleString()}{r.profileId ? ` · ${r.profileId}` : ''}{r.notesJson ? ' · notes ✓' : ''}
            </div>
          </div>
        ))}
      </div>
      {selected && (
        <div className="max-h-64 overflow-y-auto border-t border-white/10 px-3 py-2 space-y-2">
          {selected.notes && (
            <div className="text-[12px] text-white/80">
              <div className="text-[11px] uppercase tracking-wide text-white/45">Summary</div>
              <p className="whitespace-pre-wrap">{selected.notes.summary}</p>
            </div>
          )}
          <div className="text-[11px] uppercase tracking-wide text-white/45">Transcript ({selected.transcript.length} lines)</div>
          <div className="space-y-1 max-h-40 overflow-y-auto">
            {selected.transcript.map((l) => (
              <div key={l.id} className="text-[12px] leading-snug">
                <span className={l.speaker === 'me' ? 'text-sky-300' : 'text-emerald-300'}>{l.speaker === 'me' ? 'Me' : 'Others'}</span>
                <span className="text-white/25"> · </span>
                <span className="text-white/80">{l.text}</span>
              </div>
            ))}
          </div>
        </div>
      )}
    </div>
  );
}
