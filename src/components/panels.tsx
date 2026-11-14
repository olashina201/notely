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
