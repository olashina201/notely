import * as React from 'react';
import { commands, events } from './api';
import { HistoryPanel, NotesPanel, SettingsPanel } from './components/panels';
import { RegionSelector, type SelRect } from './components/RegionSelector';
import { CapturesBar, TranscriptView } from './components/views';
import { DEFAULT_HOTKEYS, PRESETS, type AppSettings, type CapturedImage, type MeetingRow, type ModelInfo, type NotesJson, type SelectorBegin, type TranscriptLine } from './types';

type Panel = 'assist' | 'transcript' | 'notes' | 'history' | 'settings';

const DEFAULT_SETTINGS: AppSettings = {
  liveModel: 'gemini-2.0-flash-live-001',
  fastModel: 'gemini-2.5-flash',
  smartModel: 'gemini-2.5-pro',
  language: 'en-US',
  overlayOpacity: 0.95,
  contentProtection: true,
  clickThrough: false,
  saveCaptures: false,
  thinkingLevel: 'low',
  searchGrounding: false,
  autoAttachScreenshot: true,
  instructionProfile: 'interview',
  customInstructions: '',
  hotkeys: { ...DEFAULT_HOTKEYS },
};

function parseTranscript(json: string): TranscriptLine[] {
  try {
    const v = JSON.parse(json);
    return Array.isArray(v) ? v : [];
  } catch {
    return [];
  }
}

function parseNotes(json: string | null): NotesJson | null {
  if (!json) return null;
  try {
    return JSON.parse(json) as NotesJson;
  } catch {
    return null;
  }
}

const PRESET_ICON: Record<string, string> = { say: '🪄', followup: '💬', recap: '🔄', screen: '⌖' };

export function App() {
  const [settings, setSettings] = React.useState<AppSettings>(DEFAULT_SETTINGS);
  const [ready, setReady] = React.useState(false);
  const [panel, setPanel] = React.useState<Panel>('assist');
  const [menuOpen, setMenuOpen] = React.useState(false);
  const [showTranscript, setShowTranscript] = React.useState(false);
  const [smartMode, setSmartMode] = React.useState(false);
  const [activePreset, setActivePreset] = React.useState<string | null>(null);
  const [recording, setRecording] = React.useState(false);
  const [liveStatus, setLiveStatus] = React.useState('idle');
  const [lines, setLines] = React.useState<TranscriptLine[]>([]);
  const [captures, setCaptures] = React.useState<CapturedImage[]>([]);
  const [question, setQuestion] = React.useState('');
  const [answer, setAnswer] = React.useState('');
  const [streaming, setStreaming] = React.useState(false);
  const [asking, setAsking] = React.useState(false);
  const [askingElapsed, setAskingElapsed] = React.useState(0);
  const [err, setErr] = React.useState<string | null>(null);

  React.useEffect(() => {
    if (!asking) {
      setAskingElapsed(0);
      return;
    }
    const t0 = Date.now();
    const iv = setInterval(() => {
      setAskingElapsed(Math.floor((Date.now() - t0) / 100) / 10);
    }, 100);
    return () => clearInterval(iv);
  }, [asking]);
  const [keyHas, setKeyHas] = React.useState(false);
  const [keyMsg, setKeyMsg] = React.useState<string | null>(null);
  const [models, setModels] = React.useState<ModelInfo[]>([]);
  const [modelsLoading, setModelsLoading] = React.useState(false);
  const [notes, setNotes] = React.useState<NotesJson | null>(null);
  const [notesGenerating, setNotesGenerating] = React.useState(false);
  const [history, setHistory] = React.useState<MeetingRow[]>([]);
  const [historySearching, setHistorySearching] = React.useState(false);
  const [selected, setSelected] = React.useState<{ row: MeetingRow; transcript: TranscriptLine[]; notes: NotesJson | null } | null>(null);
  const [selector, setSelector] = React.useState<SelectorBegin | null>(null);

  const settingsRef = React.useRef(settings);
  settingsRef.current = settings;
  const streamingRef = React.useRef(false);
  const selectorSaved = React.useRef<{ x: number; y: number; w: number; h: number } | null>(null);
  const inputRef = React.useRef<HTMLInputElement>(null);

  React.useEffect(() => {
    if (ready && panel === 'assist') {
      const t = setTimeout(() => {
        inputRef.current?.focus();
      }, 80);
      return () => clearTimeout(t);
    }
  }, [ready, panel]);

  // ---- initial load ----
  React.useEffect(() => {
    commands.settingsGet().then(setSettings).catch(() => {}).finally(() => setReady(true));
    commands.keyHas().then(setKeyHas).catch(() => {});
    refreshHistory();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  async function refreshHistory() {
    try {
      setHistory(await commands.historyList(50));
    } catch {
      /* ignore */
    }
  }

  // ---- backend events ----
  React.useEffect(() => {
    const subs: { (): void }[] = [];
    events.onTranscriptPartial((line) => {
      setLines((ls) => {
        const i = ls.findIndex((l) => l.id === line.id);
        if (i >= 0) {
          const next = ls.slice();
          next[i] = line;
          return next;
        }
        return [...ls, line];
      });
    }).then((u) => subs.push(u));
    events.onTranscriptFinal((line) => {
      setLines((ls) => {
        const i = ls.findIndex((l) => l.id === line.id);
        const next = i >= 0 ? ls.map((l, j) => (j === i ? line : l)) : [...ls, line];
        const finals = next.filter((l) => l.isFinal);
        const partials = next.filter((l) => !l.isFinal);
        const tail = finals.slice(-500);
        // Persist finals to Rust (this is what notes/history read).
        commands.meetingTranscriptAdd(line).catch(() => {});
        return [...tail, ...partials];
      });
    }).then((u) => subs.push(u));
    events.onTranscriptStatus((s) => setLiveStatus(s.status)).then((u) => subs.push(u));
    events.onCaptureTaken((img) => setCaptures((c) => [...c, img].slice(-4))).then((u) => subs.push(u));
    events.onAssistChunk((c) => {
      setAnswer((a) => a + c.text);
      setAsking(false);
    }).then((u) => subs.push(u));
    events.onAssistDone((d) => {
      streamingRef.current = false;
      setStreaming(false);
      setAsking(false);
      setAskingElapsed(0);
      if (d.error) setErr(d.error);
    }).then((u) => subs.push(u));
    events.onError((e) => {
      streamingRef.current = false;
      setStreaming(false);
      setAsking(false);
      setAskingElapsed(0);
      setErr(e.message);
    }).then((u) => subs.push(u));
    events.onStealthChanged((p) => {
      setSettings((s) => ({ ...s, contentProtection: p.stealth }));
    }).then((u) => subs.push(u));
    events.onHotkeyAssist(() => {
      if (!streamingRef.current) void runAssist(null);
    }).then((u) => subs.push(u));
    events.onHotkeyPreset((p) => {
      if (!streamingRef.current) void runAssist(p.preset);
    }).then((u) => subs.push(u));
    events.onHotkeyRegion(() => {
      void beginRegionSelect();
    }).then((u) => subs.push(u));
    return () => subs.forEach((u) => u());
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  // Auto-clear transient errors after a few seconds, but keep quota/model errors visible.
  React.useEffect(() => {
    if (!err) return;
    if (err.toLowerCase().includes('quota') || err.toLowerCase().includes('limit') || err.toLowerCase().includes('key') || err.toLowerCase().includes('model')) return;
    const t = setTimeout(() => setErr(null), 6000);
    return () => clearTimeout(t);
  }, [err]);

  // ---- meeting ----
  async function toggleMeeting() {
    setErr(null);
    try {
      if (recording) {
        const res = await commands.meetingStop();
        setRecording(false);
        setLiveStatus('idle');
        if (res.saved) await refreshHistory();
      } else {
        setLines([]);
        setCaptures([]);
        setNotes(null);
        setAnswer('');
        setActivePreset(null);
        await commands.meetingStart();
        setRecording(true);
        setLiveStatus('recording');
      }
    } catch (e) {
      setErr(String(e));
    }
  }

  // ---- stealth mode ----
  async function toggleStealth() {
    setErr(null);
    try {
      const next = await commands.overlayToggleStealth();
      setSettings((s) => ({ ...s, contentProtection: next }));
    } catch (e) {
      setErr(String(e));
    }
  }

  // ---- assist ----
  function cancelAssist() {
    streamingRef.current = false;
    setStreaming(false);
    setAsking(false);
    setAskingElapsed(0);
    setErr(null);
    void commands.assistCancel();
  }

  async function runAssist(preset: string | null, slot?: 'fast' | 'smart') {
    if (streamingRef.current) return;
    setErr(null);
    setAnswer('');
    setStreaming(true);
    setAsking(true);
    setAskingElapsed(0);
    streamingRef.current = true;
    setActivePreset(preset);
    setPanel('assist');
    setMenuOpen(false);
    const s = settingsRef.current;
    const needScreenshot = preset === 'screen' || captures.length > 0;
    try {
      await commands.assistRun({
        question: preset ? '' : question || 'What should I focus on?',
        preset,
        includeScreenshot: needScreenshot,
        profileId: s.instructionProfile,
        customInstructions: s.customInstructions || null,
        slot: slot ?? (smartMode ? 'smart' : 'fast'),
      });
      if (!preset) setQuestion('');
    } catch (e) {
      streamingRef.current = false;
      setStreaming(false);
      setAsking(false);
      setAskingElapsed(0);
      setErr(String(e));
    }
  }

  // ---- captures ----
  async function beginRegionSelect() {
    if (selector) return;
    setErr(null);
    try {
      const info = await commands.selectorBegin();
      selectorSaved.current = info.saved;
      setSelector(info);
    } catch (e) {
      setErr(String(e));
    }
  }

  async function finishRegionSelect(rect: SelRect | null) {
    const saved = selectorSaved.current;
    selectorSaved.current = null;
    setSelector(null);
    try {
      await commands.selectorEnd(saved);
    } catch {
      /* best effort */
    }
    if (!rect) return;
    try {
      const img = await commands.captureRegion(rect.x, rect.y, rect.w, rect.h);
      setCaptures((c) => [...c, img].slice(-4));
    } catch (e) {
      setErr(String(e));
    }
  }

  async function clearCaptures() {
    await commands.captureClear().catch(() => {});
    setCaptures([]);
  }

  // ---- notes / history ----
  async function generateNotes() {
    setErr(null);
    setNotesGenerating(true);
    try {
      const n = await commands.notesGenerate();
      setNotes(n as unknown as NotesJson);
      await refreshHistory();
    } catch (e) {
      setErr(String(e));
    } finally {
      setNotesGenerating(false);
    }
  }

  async function searchHistory(q: string) {
    setHistorySearching(true);
    try {
      setHistory(q.trim() ? await commands.historySearch(q.trim()) : await commands.historyList(50));
    } catch (e) {
      setErr(String(e));
    } finally {
      setHistorySearching(false);
    }
  }

  async function selectMeeting(id: number) {
    try {
      const row = await commands.historyGet(id);
      if (row) setSelected({ row, transcript: parseTranscript(row.transcriptJson), notes: parseNotes(row.notesJson) });
    } catch (e) {
      setErr(String(e));
    }
  }

  async function deleteMeeting(id: number) {
    try {
      await commands.historyDelete(id);
      if (selected?.row.id === id) setSelected(null);
      await refreshHistory();
    } catch (e) {
      setErr(String(e));
    }
  }

  // ---- window drag (any non-interactive area) ----
  function maybeDrag(e: React.MouseEvent) {
    if (e.button !== 0 || selector) return;
    if ((e.target as HTMLElement).closest('button,input,textarea,a,select,[data-nodrag]')) return;
    import('@tauri-apps/api/window').then(({ getCurrentWindow }) => {
      getCurrentWindow().startDragging().catch(() => {});
    });
  }

  if (!ready) return null;

  const opacity = selector ? 1 : settings.overlayOpacity;
  const glass = { background: `rgba(16,18,24,${opacity})`, backdropFilter: 'blur(20px)', WebkitBackdropFilter: 'blur(20px)' };
  const presetLabel = activePreset ? PRESETS.find((p) => p.id === activePreset)?.label ?? activePreset : null;

  return (
    <div className="h-full flex flex-col items-center pt-2 pb-3 px-4" onMouseDown={maybeDrag} style={{ cursor: 'default' }}>
      {/* Detached top pill: stealth controls */}
      <div className="flex items-center gap-1 pl-1.5 pr-1.5 py-1 rounded-full border border-white/15 shadow-2xl" style={glass}>
        <div className={`w-7 h-7 rounded-full flex items-center justify-center text-[15px] ${recording ? 'bg-red-500/25' : 'bg-white/10'}`} title={recording ? (liveStatus === 'reconnecting' ? 'Reconnecting…' : 'Recording') : 'Idle'}>
          {recording ? <span className="w-2 h-2 rounded-full bg-red-400 animate-pulse" /> : <span className="text-white/80">✦</span>}
        </div>
        <button onClick={() => void toggleStealth()}
          title={settings.contentProtection ? 'Stealth Mode is ACTIVE (hidden from screen share & Dock). Click to deactivate.' : 'Stealth Mode is OFF (visible in screen share & Dock). Click to activate.'}
          className={`flex items-center gap-1.5 px-3 py-1 rounded-full text-[12px] font-medium transition-all ${
            settings.contentProtection
              ? 'bg-emerald-500/20 border border-emerald-400/35 text-emerald-300 hover:bg-emerald-500/30'
              : 'bg-amber-500/15 border border-amber-400/30 text-amber-200 hover:bg-amber-500/25'
          }`}>
          <span className="text-[12px]">{settings.contentProtection ? '🥷' : '👀'}</span>
          <span>{settings.contentProtection ? 'Stealth' : 'Visible'}</span>
        </button>
        <button onClick={() => void commands.overlayHide()} title="Hide overlay (global hotkey brings it back)"
          className="flex items-center gap-1 px-3 py-1 rounded-full text-[13px] text-white/85 hover:bg-white/10">
          <span className="text-white/50 text-[11px]">∨</span> Hide
        </button>
        <button onClick={() => void toggleMeeting()} title={recording ? 'Stop meeting' : 'Start meeting'}
          className={`w-7 h-7 rounded-full flex items-center justify-center ${recording ? 'bg-red-500/80 hover:bg-red-500' : 'bg-white/10 hover:bg-white/20'}`}>
          {recording
            ? <span className="w-2.5 h-2.5 rounded-[3px] bg-white" />
            : <span className="w-2.5 h-2.5 rounded-full bg-white/85" />}
        </button>
      </div>

      {/* Main glass card */}
      <div className="w-full max-w-[760px] mt-2 rounded-2xl border border-white/12 shadow-2xl flex flex-col min-h-0 overflow-hidden" style={glass}>
        {panel === 'assist' && (
          <>
            {/* Active preset pill */}
            {presetLabel && (
              <div className="flex justify-end px-4 pt-3">
                <span className="px-3 py-1 rounded-full bg-blue-600/90 text-white text-[13px] font-medium shadow">{presetLabel}</span>
              </div>
            )}

            {/* Answer */}
            <div className="px-5 pt-3 pb-1 overflow-y-auto min-h-[72px] max-h-64">
              {!answer ? (
                <div className="text-[13px] text-white/35 text-center py-5">
                  {asking ? (
                    <span className="flex items-center justify-center gap-2 text-white/70">
                      <span className="w-2 h-2 rounded-full bg-blue-400 animate-ping" />
                      <span>{smartMode ? 'Smart reasoning…' : 'Thinking…'}</span>
                      <span className="text-[11px] text-white/40 tabular-nums">({askingElapsed.toFixed(1)}s)</span>
                      <button onClick={cancelAssist} className="text-[11px] text-white/75 hover:text-white bg-white/10 hover:bg-white/20 px-2 py-0.5 rounded-full ml-1 transition">Cancel</button>
                    </span>
                  ) : err ? (
                    <div className="flex flex-col items-center justify-center gap-1.5 py-1 text-center select-text">
                      <span className="text-amber-400 text-sm font-medium">⚠️ {err.toLowerCase().includes('quota') || err.toLowerCase().includes('limit') ? 'Model Quota Limit' : 'Assist Error'}</span>
                      <span className="text-[12.5px] text-white/80 max-w-lg leading-relaxed">{err.replace(/^Error:\s*/, '')}</span>
                      <div className="flex items-center gap-2 mt-2">
                        <button onClick={() => { setErr(null); void runAssist(null, 'fast'); }} className="px-3 py-1 rounded-lg bg-blue-600/80 hover:bg-blue-600 text-white text-[12px] font-medium shadow">
                          ⚡ Try Fast Model
                        </button>
                        <button onClick={() => setPanel('settings')} className="px-3 py-1 rounded-lg bg-white/10 hover:bg-white/20 text-white/90 text-[12px]">
                          ⚙️ Settings
                        </button>
                        <button onClick={() => setErr(null)} className="px-2 py-1 text-white/40 hover:text-white text-[12px]">
                          Dismiss
                        </button>
                      </div>
                    </div>
                  ) : recording ? (
                    'Listening… ask anything with Enter or ⌘/Ctrl+Enter.'
                  ) : (
                    'Start the meeting or ask a question — answers appear here, invisible to screen share.'
                  )}
                </div>
              ) : (
                <div className="text-[15px] leading-relaxed text-white/90 whitespace-pre-wrap select-text">
                  {answer}
                  {streaming && <span className="inline-block w-2 h-4 ml-1 align-middle bg-blue-400/80 animate-pulse" />}
                </div>
              )}
            </div>

            {/* Quick actions */}
            <div className="flex items-center justify-center gap-0.5 px-3 py-1.5 text-[13px] text-white/70 flex-wrap">
              <button onClick={() => { setActivePreset(null); setAnswer(''); }} className="flex items-center gap-1 px-2 py-1 rounded-lg hover:bg-white/10 hover:text-white/90" title="Clear answer">
                <span>✨</span> Assist
              </button>
              {PRESETS.map((p) => (
                <span key={p.id} className="flex items-center">
                  <span className="text-white/20">·</span>
                  <button onClick={() => void runAssist(p.id)} disabled={asking}
                    className={`flex items-center gap-1 px-2 py-1 rounded-lg hover:bg-white/10 hover:text-white/90 disabled:opacity-50 ${activePreset === p.id ? 'text-white/95' : ''}`}>
                    <span>{PRESET_ICON[p.id] ?? '•'}</span> {p.label}
                  </button>
                </span>
              ))}
            </div>

            {/* Live transcript (collapsible) + captures */}
            {showTranscript && <TranscriptView lines={lines} recording={recording} />}
            {captures.length > 0 && <CapturesBar captures={captures} onClear={() => void clearCaptures()} />}

            {/* Input row */}
            <div className="px-4 pb-4 pt-1" onClick={() => { commands.overlayFocus().catch(() => {}); inputRef.current?.focus(); }}>
              <div className="rounded-2xl border border-white/12 bg-white/[0.06] px-3.5 pt-2.5 pb-2">
                <input
                  ref={inputRef}
                  value={question}
                  onChange={(e) => setQuestion(e.target.value)}
                  onFocus={() => { commands.overlayFocus().catch(() => {}); }}
                  onKeyDown={(e) => {
                    if (e.key === 'Enter') {
                      e.preventDefault();
                      void runAssist(null);
                    }
                  }}
                  placeholder="Ask anything… (Press Enter or ⌘↵ to send)"
                  className="w-full bg-transparent outline-none text-[13.5px] text-white/90 placeholder:text-white/40" />
                <div className="flex items-center gap-1 mt-1.5">
                  <button onClick={() => setSmartMode((v) => !v)} title={smartMode ? 'Smart model (slower, stronger)' : 'Fast model'}
                    className={`flex items-center gap-1 px-2.5 py-1 rounded-full text-[12px] border ${smartMode ? 'bg-blue-600/80 border-blue-500/50 text-white' : 'bg-white/5 border-white/10 text-white/60 hover:text-white/85'}`}>
                    <span>⚡</span> Smart
                  </button>
                  <div className="relative">
                    <button onClick={() => setMenuOpen((v) => !v)} title="More"
                      className="px-2 py-1 rounded-full text-white/50 hover:text-white/85 hover:bg-white/10 text-[13px]">···</button>
                    {menuOpen && (
                      <div className="absolute bottom-8 left-0 w-48 rounded-xl border border-white/15 shadow-2xl py-1 text-[12.5px]" style={{ ...glass, background: 'rgba(24,26,34,0.98)' }}>
                        {(['transcript', 'notes', 'history', 'settings'] as Panel[]).map((p) => (
                          <button key={p} onClick={() => { setPanel(p); setMenuOpen(false); if (p === 'transcript') setShowTranscript((v) => !v); }}
                            className="w-full text-left px-3 py-1.5 text-white/75 hover:bg-white/10 hover:text-white capitalize">
                            {p === 'transcript' ? `${showTranscript ? 'Hide' : 'Show'} transcript` : p}
                            {p !== 'assist' && p !== 'transcript' && <span className="text-white/30"> →</span>}
                          </button>
                        ))}
                        <div className="border-t border-white/10 mt-1 pt-1">
                          <button onClick={() => { setMenuOpen(false); void toggleStealth(); }} className="w-full text-left px-3 py-1.5 text-white/75 hover:bg-white/10 hover:text-white flex items-center justify-between">
                            <span>{settings.contentProtection ? 'Deactivate stealth' : 'Activate stealth'}</span>
                            <span className="text-[12px]">{settings.contentProtection ? '👀' : '🥷'}</span>
                          </button>
                          <button onClick={() => { setMenuOpen(false); void beginRegionSelect(); }} className="w-full text-left px-3 py-1.5 text-white/75 hover:bg-white/10 hover:text-white">⌖ Region screenshot</button>
                          <button onClick={() => { setMenuOpen(false); void toggleMeeting(); }} className="w-full text-left px-3 py-1.5 text-white/75 hover:bg-white/10 hover:text-white">{recording ? '■ Stop meeting' : '● Start meeting'}</button>
                        </div>
                      </div>
                    )}
                  </div>
                  <div className="flex-1" />
                  {asking ? (
                    <button onClick={cancelAssist} className="text-[12px] text-white/70 hover:text-white bg-white/10 hover:bg-white/20 px-2.5 py-1 rounded-full transition">Cancel</button>
                  ) : (
                    captures.length > 0 && <span className="text-[11px] text-white/35">{captures.length} shot{captures.length > 1 ? 's' : ''}</span>
                  )}
                  <button onClick={() => void runAssist(null)} disabled={asking} title="Ask (⌘/Ctrl+Enter)"
                    className="w-8 h-8 rounded-full bg-blue-600 hover:bg-blue-500 disabled:opacity-50 flex items-center justify-center text-white shadow">
                    <span className="text-[13px] ml-0.5">▶</span>
                  </button>
                </div>
              </div>
            </div>
          </>
        )}

        {/* Secondary panels render inside the same card */}
        {panel !== 'assist' && (
          <div className="flex flex-col min-h-0 max-h-[420px]">
            <div className="flex items-center gap-2 px-4 pt-3 pb-1">
              <button onClick={() => setPanel('assist')} className="text-white/50 hover:text-white/90 text-[13px]">← Assist</button>
              <span className="text-[13px] text-white/80 capitalize">{panel === 'transcript' ? 'Transcript' : panel}</span>
            </div>
            {panel === 'transcript' && <TranscriptView lines={lines} recording={recording} />}
            {panel === 'notes' && (
              <NotesPanel notes={notes} generating={notesGenerating} disabled={lines.filter((l) => l.isFinal).length === 0} onGenerate={() => void generateNotes()} />
            )}
            {panel === 'history' && (
              <HistoryPanel rows={history} selected={selected} searching={historySearching}
                onSearch={(q) => void searchHistory(q)} onSelect={(id) => void selectMeeting(id)}
                onDelete={(id) => void deleteMeeting(id)} onRefresh={() => void refreshHistory()} />
            )}
            {panel === 'settings' && (
              <SettingsPanel
                settings={settings}
                keyHas={keyHas}
                models={models}
                modelsLoading={modelsLoading}
                keyMsg={keyMsg}
                onSave={async (s) => {
                  await commands.settingsSet(s);
                  setSettings(s);
                  await commands.registerHotkeys(s.hotkeys);
                }}
                onLoadModels={async () => {
                  setModelsLoading(true);
                  try {
                    setModels(await commands.modelsList());
                  } catch (e) {
                    setKeyMsg(String(e));
                  } finally {
                    setModelsLoading(false);
                  }
                }}
                onSetKey={async (k) => {
                  try {
                    await commands.keySet(k);
                    setKeyHas(true);
                    setKeyMsg('✓ Key saved securely.');
                    setErr(null);
                    setModelsLoading(true);
                    try {
                      setModels(await commands.modelsList());
                    } catch (e) {
                      setKeyMsg(`Key saved, but model test failed: ${e}`);
                    } finally {
                      setModelsLoading(false);
                    }
                  } catch (e) {
                    setKeyMsg(`Failed to save key: ${e}`);
                  }
                }}
                onClearKey={async () => {
                  await commands.keyClear();
                  setKeyHas(false);
                  setModels([]);
                  setKeyMsg('Key removed.');
                }}
                onTestKey={async () => {
                  try {
                    setKeyMsg(await commands.keyTest());
                  } catch (e) {
                    setKeyMsg(String(e));
                  }
                }}
              />
            )}
          </div>
        )}

        {/* Error bar */}
        {err && (
          <div className="px-4 py-2 border-t border-red-400/20 bg-red-500/15 text-[12px] text-red-200 break-words max-h-24 overflow-y-auto flex items-center justify-between gap-2">
            <span className="flex-1">{err.replace(/^Error:\s*/, '')}</span>
            <div className="flex items-center gap-1.5 shrink-0">
              {(err.toLowerCase().includes('settings') || err.toLowerCase().includes('key') || err.toLowerCase().includes('model')) && (
                <button onClick={() => setPanel('settings')} className="px-2 py-0.5 rounded bg-white/10 hover:bg-white/20 text-white text-[11px]">Open Settings</button>
              )}
              <button onClick={() => setErr(null)} className="px-1.5 py-0.5 rounded text-white/50 hover:text-white text-[11px]" title="Dismiss">✕</button>
            </div>
          </div>
        )}
      </div>

      {/* Region selector overlays everything while active */}
      {selector && <RegionSelector sel={selector} onDone={(r) => void finishRegionSelect(r)} />}
    </div>
  );
}
