import * as React from 'react';
import { commands } from '../api';
import type { AppSettings, MeetingRow, ModelInfo, NotesJson, TranscriptLine } from '../types';
import { HOTKEY_ACTIONS, PROFILES, PROVIDERS, type Provider } from '../types';
import { ProviderFields, CredentialEditor } from './ProviderSettings';
import { ScreenPermissions } from './ScreenPermissions';
import { Icon } from './Icon';
import { shortcutLabel } from './SharingControls';

// ---------- Settings ----------

export function SettingsPanel({ settings, onSave, onClose }: {
  settings: AppSettings;
  onSave: (s: AppSettings) => Promise<void>;
  onClose: () => void;
}) {
  const [draft, setDraft] = React.useState<AppSettings>(settings);
  const [profileEditor, setProfileEditor] = React.useState(settings.instructionProfile);
  const [saving, setSaving] = React.useState(false);
  const [saveMessage, setSaveMessage] = React.useState('');
  React.useEffect(() => setDraft(settings), [settings]);

  function set<K extends keyof AppSettings>(k: K, v: AppSettings[K]) {
    setDraft((d) => ({ ...d, [k]: v }));
  }

  async function save() {
    setSaving(true);
    setSaveMessage('');
    try {
      await onSave(draft);
      setSaveMessage('Settings saved.');
    } catch (e) {
      setSaveMessage(String(e));
    } finally {
      setSaving(false);
    }
  }

  return (
    <div className="flex-1 overflow-y-auto px-3 py-2 space-y-4 text-[12.5px] text-white/90">
      <section className="space-y-2">
        <label className="flex items-center gap-2">
          <span className="text-white/65 w-32">AI assistant</span>
          <select value={draft.assistantProvider} onChange={(e) => set('assistantProvider', e.target.value as Provider)}
            className="flex-1 bg-white/10 border border-white/12 rounded-lg px-2 py-1.5 [&>option]:text-black">
            {PROVIDERS.map((p) => <option key={p.id} value={p.id}>{p.name}</option>)}
          </select>
        </label>
        <div className="text-white/65 text-[11.5px]">Used for answers, screen analysis, and meeting notes. Save settings to apply.</div>
        <ProviderFields key={draft.assistantProvider} settings={draft} onChange={setDraft} />
      </section>
      <section className="space-y-2">
        <div className="text-[11px] uppercase tracking-wide text-white/65">Meeting transcription</div>
        <label className="flex items-center gap-2"><span className="text-white/65 w-32">Audio provider</span><select aria-label="Transcription provider" value={draft.transcriptionProvider} onChange={e => set('transcriptionProvider', e.target.value as AppSettings['transcriptionProvider'])} className="flex-1 min-w-0 bg-white/10 border border-white/12 rounded-lg px-2 py-1.5"><option value="openai">OpenAI</option><option value="gemini">Gemini</option></select></label>
        <p className="text-white/65 text-[11.5px]">Separate from your assistant. OpenAI meetings need only an OpenAI key; Gemini meetings need a Gemini key. Changes apply to the next meeting after saving.</p>
        <CredentialEditor key={`transcription-${draft.transcriptionProvider}`} provider={draft.transcriptionProvider} model={draft.transcriptionProvider === 'openai' ? draft.openaiTranscriptionModel : draft.liveModel} testConnection={() => commands.transcriptionTest(draft.transcriptionProvider, draft.transcriptionProvider === 'openai' ? draft.openaiTranscriptionModel : draft.liveModel, draft.language)} />
        <ModelField label="Transcription model" value={draft.transcriptionProvider === 'openai' ? draft.openaiTranscriptionModel : draft.liveModel} options={[]} onChange={v => draft.transcriptionProvider === 'openai' ? set('openaiTranscriptionModel', v) : set('liveModel', v)} />
        <p className="text-white/65 text-[11.5px]">Claude remains available for chat and meeting notes using either provider’s transcript.</p>
        <label className="flex items-center gap-2">
          <span className="text-white/65 w-32">Language</span>
          <input value={draft.language} onChange={(e) => set('language', e.target.value)}
            className="flex-1 min-w-0 bg-white/10 border border-white/12 rounded-lg px-2 py-1 text-[12px] outline-none" />
        </label>
      </section>
      <section className="profile-settings space-y-2">
        <h2>Make each profile yours</h2>
        <p>Give every experience its own context. Pick a profile here to edit it, then select it in chat whenever you need it.</p>
        <label className="profile-editor-label" htmlFor="profile-editor">Customize profile</label>
        <select id="profile-editor" value={profileEditor} onChange={(e) => setProfileEditor(e.target.value)} className="w-full rounded-lg px-2 py-2">
          {PROFILES.map((p) => <option key={p.id} value={p.id}>{p.name}</option>)}
        </select>
        <div className="profile-description">{PROFILES.find(p=>p.id === profileEditor)?.description}</div>
        <label className="profile-editor-label" htmlFor="profile-instructions">Extra instructions for {PROFILES.find(p=>p.id === profileEditor)?.name}</label>
        <textarea id="profile-instructions" value={draft.profileInstructions?.[profileEditor] ?? ''} onChange={(e) => set('profileInstructions', { ...draft.profileInstructions, [profileEditor]: e.target.value })}
          placeholder={PROFILES.find(p=>p.id === profileEditor)?.placeholder} rows={4} className="w-full rounded-lg px-3 py-2 resize-y" />
        <small>Only used with this profile. Saved when you choose Save settings.</small>
        <label className="profile-editor-label" htmlFor="global-instructions">Instructions for every profile</label>
        <textarea id="global-instructions" value={draft.customInstructions} onChange={(e) => set('customInstructions', e.target.value)} placeholder="Your preferred language, answer style, and shared context…" rows={2} className="w-full rounded-lg px-3 py-2 resize-y" />
      </section>

      <ScreenPermissions />
      {/* Overlay */}
      <section className="space-y-1.5">
        <div className="text-[11px] uppercase tracking-wide text-white/65">Overlay</div>
        <label className="flex items-center gap-2">
          <span className="text-white/65 w-32">Opacity</span>
          <input type="range" min={0.4} max={1} step={0.05} value={draft.overlayOpacity}
            onChange={(e) => set('overlayOpacity', Number(e.target.value))} className="flex-1" />
          <span className="text-white/65 w-8 text-right">{Math.round(draft.overlayOpacity * 100)}</span>
        </label>
        <Toggle label="Request screen-capture protection" value={draft.contentProtection} onChange={(v) => set('contentProtection', v)} />
        <div className="text-[11.5px] text-white/65">Click-through is temporary: use {shortcutLabel(draft.hotkeys.toggleClickThrough || 'CommandOrControl+Shift+T')} or the menu bar. Show Notely restores mouse input.</div>
        <Toggle label="Keep screenshots" value={draft.saveCaptures} onChange={(v) => set('saveCaptures', v)} />
        <Toggle label="Auto-attach screenshot" value={draft.autoAttachScreenshot} onChange={(v) => set('autoAttachScreenshot', v)} />
      </section>

      {/* Hotkeys */}
      <section className="space-y-1.5">
        <div className="text-[11px] uppercase tracking-wide text-white/65">Hotkeys</div>
        {HOTKEY_ACTIONS.map((a) => (
          <label key={a.id} className="flex items-center gap-2">
            <span className="text-white/65 flex-1">{a.label}</span>
            <input value={draft.hotkeys[a.id] ?? ''} onChange={(e) => set('hotkeys', { ...draft.hotkeys, [a.id]: e.target.value })}
              spellCheck={false}
              className="w-44 font-mono bg-white/10 border border-white/12 rounded-lg px-2 py-1 text-[11px] outline-none" />
          </label>
        ))}
      </section>

      <button onClick={save} disabled={saving}
        className="w-full py-2 rounded-lg bg-teal-700 hover:bg-teal-800 text-white text-[13px] disabled:opacity-50">
        {saving ? 'Saving…' : 'Save settings'}
      </button>
      <button onClick={onClose} className="w-full py-2 rounded-lg bg-white/10 hover:bg-white/10 text-white/90">Back to Assist · Esc</button>
      {saveMessage && <div role="status" className="text-[12px] text-white/65">{saveMessage}</div>}
      <div className="h-1" />
    </div>
  );
}

export function ModelField({ label, value, options, onChange }: { label: string; value: string; options: ModelInfo[]; onChange: (v: string) => void }) {
  const listId = `models-${label.replace(/\W+/g, '-')}`;
  return (
    <label className="flex items-center gap-2">
      <span className="text-white/65 w-32 shrink-0">{label}</span>
      <input value={value} onChange={(e) => onChange(e.target.value)} list={options.length ? listId : undefined} spellCheck={false}
        className="flex-1 min-w-0 font-mono bg-white/10 border border-white/12 rounded-lg px-2 py-1 text-[11.5px] outline-none" />
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
      <span className="text-white/65 flex-1">{label}</span>
      <button type="button" onClick={() => onChange(!value)}
        className={`w-9 h-5 rounded-full transition-colors relative ${value ? 'bg-sky-500/80' : 'bg-white/10'}`}>
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
        className="w-full py-1.5 rounded-lg bg-white/10 hover:bg-white/10 border border-white/12 text-[12.5px] disabled:opacity-50">
        {generating ? 'Generating…' : notes ? 'Regenerate notes' : 'Generate notes from transcript'}
      </button>
      {!notes && <div className="empty-state"><span className="empty-state-icon"><Icon name="notes" size={24} /></span><h2>Less note-taking. More being there.</h2><p>Your meeting’s key points, decisions, and next steps belong here. Record a conversation, then generate your notes.</p></div>}
      {notes && (
        <div className="space-y-2.5 text-[12.5px] leading-relaxed text-white/90">
          <section>
            <div className="text-[11px] uppercase tracking-wide text-white/65 mb-0.5">Summary</div>
            <p className="whitespace-pre-wrap">{notes.summary}</p>
          </section>
          {notes.decisions.length > 0 && (
            <section>
              <div className="text-[11px] uppercase tracking-wide text-white/65 mb-0.5">Decisions</div>
              <ul className="list-disc pl-5 space-y-0.5">{notes.decisions.map((d, i) => <li key={i}>{d}</li>)}</ul>
            </section>
          )}
          {notes.actionItems.length > 0 && (
            <section>
              <div className="text-[11px] uppercase tracking-wide text-white/65 mb-0.5">Action items</div>
              <ul className="list-disc pl-5 space-y-0.5">
                {notes.actionItems.map((a, i) => <li key={i}>{a.text}{a.owner ? <span className="text-white/65"> — {a.owner}</span> : null}</li>)}
              </ul>
            </section>
          )}
          {notes.followUpEmail && (
            <section>
              <div className="text-[11px] uppercase tracking-wide text-white/65 mb-0.5">Follow-up email</div>
              <div className="response-body bg-white/5 border border-white/12 rounded-lg p-2 whitespace-pre-wrap select-text">{notes.followUpEmail}</div>
            </section>
          )}
        </div>
      )}
    </div>
  );
}

// ---------- History ----------

export function HistoryPanel({
  onSelectChat,
  onContinueChat,
}: {
  onSelectChat?: (turns: { question: string; answer: string }[]) => void;
  onContinueChat?: (chatId: string, turns: { question: string; answer: string }[]) => void;
}) {
  const [q, setQ] = React.useState('');
  const [filter, setFilter] = React.useState<'all' | 'chat' | 'meeting'>('all');
  const [chats, setChats] = React.useState<any[]>([]);
  const [meetings, setMeetings] = React.useState<MeetingRow[]>([]);
  const [loading, setLoading] = React.useState(false);
  const [selectedChat, setSelectedChat] = React.useState<{ id: string; title: string; turns: any[] } | null>(null);
  const [selectedMeeting, setSelectedMeeting] = React.useState<{ row: MeetingRow; transcript: TranscriptLine[]; notes: NotesJson | null } | null>(null);

  const load = React.useCallback(async (query: string) => {
    setLoading(true);
    try {
      const [c, m] = await Promise.all([
        commands.chatsList(query).catch(() => []),
        query.trim() ? commands.historySearch(query).catch(() => []) : commands.historyList(50).catch(() => []),
      ]);
      setChats(c);
      setMeetings(m);
    } catch {
      /* ignore */
    } finally {
      setLoading(false);
    }
  }, []);

  React.useEffect(() => {
    void load(q);
  }, [load, q]);

  async function openChat(id: string, title: string) {
    try {
      const turns = await commands.chatsGet(id);
      setSelectedChat({ id, title, turns });
      setSelectedMeeting(null);
    } catch (e) {
      console.error(e);
    }
  }

  async function openMeeting(id: number) {
    try {
      const row = await commands.historyGet(id);
      if (row) {
        let transcript: TranscriptLine[] = [];
        try { transcript = JSON.parse(row.transcriptJson); } catch {}
        let notes: NotesJson | null = null;
        try { if (row.notesJson) notes = JSON.parse(row.notesJson); } catch {}
        setSelectedMeeting({ row, transcript, notes });
        setSelectedChat(null);
      }
    } catch (e) {
      console.error(e);
    }
  }

  async function deleteChat(id: string, e: React.MouseEvent) {
    e.stopPropagation();
    await commands.chatsDelete(id).catch(() => {});
    if (selectedChat?.id === id) setSelectedChat(null);
    void load(q);
  }

  async function deleteMeeting(id: number, e: React.MouseEvent) {
    e.stopPropagation();
    await commands.historyDelete(id).catch(() => {});
    if (selectedMeeting?.row.id === id) setSelectedMeeting(null);
    void load(q);
  }

  // Selected chat details
  if (selectedChat) {
    return (
      <div className="flex-1 flex flex-col min-h-0 text-[12.5px] text-white/90">
        <div className="flex items-center justify-between px-3 py-2 border-b border-white/10">
          <button onClick={() => setSelectedChat(null)} className="text-white/60 hover:text-white flex items-center gap-1 text-[12px]">
            ← Back to history
          </button>
          {onContinueChat && (
            <button
              onClick={() => onContinueChat(selectedChat.id, selectedChat.turns)}
              className="px-2.5 py-1 rounded bg-blue-600/80 hover:bg-blue-600 text-white text-[11.5px]"
            >
              Continue in chat →
            </button>
          )}
        </div>
        <div className="flex-1 overflow-y-auto px-4 py-3 space-y-4 min-h-0 select-text">
          <h3 className="text-[14px] font-semibold text-white/95">{selectedChat.title}</h3>
          {selectedChat.turns.map((t, idx) => (
            <div key={idx} className="space-y-1.5 pb-3 border-b border-white/5 last:border-b-0">
              <div className="text-[11.5px] font-medium text-sky-300">Q: {t.question}</div>
              <div className="text-[12.5px] text-white/80 whitespace-pre-wrap leading-relaxed">{t.answer}</div>
              <div className="text-[10px] text-white/40">{t.provider} · {t.model} · {new Date(t.createdAt).toLocaleTimeString()}</div>
            </div>
          ))}
        </div>
      </div>
    );
  }

  // Selected meeting details
  if (selectedMeeting) {
    return (
      <div className="flex-1 flex flex-col min-h-0 text-[12.5px] text-white/90">
        <div className="flex items-center justify-between px-3 py-2 border-b border-white/10">
          <button onClick={() => setSelectedMeeting(null)} className="text-white/60 hover:text-white flex items-center gap-1 text-[12px]">
            ← Back to history
          </button>
        </div>
        <div className="flex-1 overflow-y-auto px-4 py-3 space-y-3 min-h-0 select-text">
          <h3 className="text-[14px] font-semibold text-white/95">
            {selectedMeeting.row.title || `Meeting ${new Date(selectedMeeting.row.startedAt).toLocaleString()}`}
          </h3>
          {selectedMeeting.notes && (
            <div className="space-y-2 p-2.5 rounded-lg bg-white/5 border border-white/10">
              <div className="text-[11px] uppercase tracking-wide text-white/60">Meeting Summary</div>
              <p className="whitespace-pre-wrap text-[12px]">{selectedMeeting.notes.summary}</p>
            </div>
          )}
          <div className="text-[11px] uppercase tracking-wide text-white/60">Transcript ({selectedMeeting.transcript.length} lines)</div>
          <div className="space-y-1 max-h-48 overflow-y-auto">
            {selectedMeeting.transcript.map((l) => (
              <div key={l.id} className="text-[12px] leading-snug">
                <span className={l.speaker === 'me' ? 'text-sky-300' : 'text-emerald-300'}>{l.speaker === 'me' ? 'Me' : 'Others'}</span>
                <span className="text-white/30"> · </span>
                <span className="text-white/85">{l.text}</span>
              </div>
            ))}
          </div>
        </div>
      </div>
    );
  }

  const items = [
    ...(filter === 'all' || filter === 'chat' ? chats.map((c) => ({ type: 'chat' as const, id: c.id, title: c.title, time: c.updatedAt, profile: c.profileId, subtitle: `${c.turnCount} turn${c.turnCount === 1 ? '' : 's'}` })) : []),
    ...(filter === 'all' || filter === 'meeting' ? meetings.map((m) => ({ type: 'meeting' as const, id: m.id, title: m.title || `Meeting ${new Date(m.startedAt).toLocaleString()}`, time: m.startedAt, profile: m.profileId, subtitle: m.notesJson ? 'Notes ready' : 'Transcript' })) : []),
  ].sort((a, b) => b.time - a.time);

  return (
    <div className="flex-1 flex flex-col min-h-0">
      <div className="flex gap-1.5 px-3 py-2 border-b border-white/10">
        <input
          value={q}
          onChange={(e) => setQ(e.target.value)}
          placeholder="Search past chats & meetings…"
          className="flex-1 min-w-0 bg-white/10 border border-white/15 rounded-lg px-2.5 py-1 text-[12px] outline-none text-white placeholder:text-white/40"
        />
        <div className="flex items-center gap-1 bg-white/10 rounded-lg p-0.5 border border-white/10 text-[11px]">
          <button onClick={() => setFilter('all')} className={`px-2 py-0.5 rounded ${filter === 'all' ? 'bg-white/20 text-white' : 'text-white/60'}`}>All</button>
          <button onClick={() => setFilter('chat')} className={`px-2 py-0.5 rounded ${filter === 'chat' ? 'bg-white/20 text-white' : 'text-white/60'}`}>Chats</button>
          <button onClick={() => setFilter('meeting')} className={`px-2 py-0.5 rounded ${filter === 'meeting' ? 'bg-white/20 text-white' : 'text-white/60'}`}>Meetings</button>
        </div>
      </div>
      <div className="flex-1 overflow-y-auto min-h-0">
        {loading && <div className="text-[12px] text-white/40 text-center py-4">Loading…</div>}
        {!loading && items.length === 0 && (
          <div className="text-[12px] text-white/35 text-center py-6">
            {q ? 'No matching history found.' : 'No saved chats or meetings yet.'}
          </div>
        )}
        {items.map((item) => (
          <div
            key={`${item.type}-${item.id}`}
            onClick={() => (item.type === 'chat' ? openChat(item.id as string, item.title) : openMeeting(item.id as number))}
            className="flex items-center justify-between px-3.5 py-2.5 border-b border-white/5 hover:bg-white/5 cursor-pointer transition"
          >
            <div className="flex-1 min-w-0 pr-2">
              <div className="flex items-center gap-1.5">
                <span className="text-[11px]">{item.type === 'chat' ? '💬' : '🎙️'}</span>
                <span className="text-[12.5px] text-white/90 truncate font-medium">{item.title}</span>
              </div>
              <div className="text-[11px] text-white/45 mt-0.5">
                {new Date(item.time).toLocaleDateString(undefined, { month: 'short', day: 'numeric', hour: '2-digit', minute: '2-digit' })}
                {' · '}{item.subtitle}
                {item.profile ? ` · ${item.profile}` : ''}
              </div>
            </div>
            <button
              onClick={(e) => (item.type === 'chat' ? deleteChat(item.id as string, e) : deleteMeeting(item.id as number, e))}
              className="text-white/30 hover:text-red-300 p-1 text-[12px]"
              title="Delete permanently"
            >
              ✕
            </button>
          </div>
        ))}
      </div>
    </div>
  );
}
