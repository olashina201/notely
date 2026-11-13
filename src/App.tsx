import * as React from 'react';
import { getCurrentWindow } from '@tauri-apps/api/window';
import { commands, events, reportDiagnostic } from './api';
import eventNames from './events.json';
import { Icon } from './components/Icon';
import { HistoryPanel, type HistorySelection } from './components/HistoryPanel';
import { WindowResize } from './components/WindowResize';
import { SharingControls, shortcutLabel } from './components/SharingControls';
import { DiagnosticsPanel } from './components/DiagnosticsPanel';
import { NotesPanel, SettingsPanel } from './components/panels';
import { RegionSelector, type SelRect } from './components/RegionSelector';
import { CapturesBar, ResponseView, TranscriptView } from './components/views';
import { DEFAULT_HOTKEYS, PRESETS, PROFILES, PROVIDERS, type ChatSummary, type ChatTurn, type HistoryEntry, type Provider, type AppSettings, type CapturedImage, type MeetingRow, type NotesJson, type SelectorBegin, type TranscriptLine } from './types';

type Panel = 'assist' | 'transcript' | 'notes' | 'history' | 'settings' | 'diagnostics';

const DEFAULT_SETTINGS: AppSettings = {
  assistantProvider: 'gemini',
  openaiFastModel: 'gpt-4.1-mini',
  openaiSmartModel: 'gpt-4.1',
  claudeFastModel: 'claude-haiku-4-5-20251001',
  claudeSmartModel: 'claude-sonnet-4-6',
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
  instructionProfile: 'general',
  customInstructions: '',
  profileInstructions: {},
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

const PRESET_ICON: Record<string, string> = { say: 'chat', followup: 'chat', recap: 'notes', screen: 'screen' };
const PRESET_DETAIL: Record<string, string> = { say: 'Find the right words', followup: 'Keep the conversation going', recap: 'Catch up on the key points', screen: 'Make sense of what’s in front of you' };

export function App() {
  const [settings, setSettings] = React.useState<AppSettings>(DEFAULT_SETTINGS);
  const [ready, setReady] = React.useState(false);
  const [eventsReady, setEventsReady] = React.useState(false);
  const [panel, setPanel] = React.useState<Panel>('assist');
  const [copied, setCopied] = React.useState(false);
  const copyTimer = React.useRef<ReturnType<typeof setTimeout>>();
  React.useEffect(() => () => clearTimeout(copyTimer.current), []);
  async function copyAnswer() {
    try {
      await navigator.clipboard.writeText(answer);
      setCopied(true);
      clearTimeout(copyTimer.current);
      copyTimer.current = setTimeout(() => setCopied(false), 2000);
    } catch { setErr('Could not copy the answer. Select the text and press ⌘C.'); }
  }
  const [smartMode, setSmartMode] = React.useState(true);
  const [activePreset, setActivePreset] = React.useState<string | null>(null);
  const [meetingBusy, setMeetingBusy] = React.useState(false);
  const [preferencesBusy, setPreferencesBusy] = React.useState(false);
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
  const [notes, setNotes] = React.useState<NotesJson | null>(null);
  const [notesGenerating, setNotesGenerating] = React.useState(false);
  const [history, setHistory] = React.useState<MeetingRow[]>([]);
  const [chats, setChats] = React.useState<ChatSummary[]>([]);
  const [recentChats, setRecentChats] = React.useState<ChatSummary[]>([]);
  const [historyQuery, setHistoryQuery] = React.useState('');
  const historyQueryRef = React.useRef('');
  const searchGeneration = React.useRef(0);
  const selectionGeneration = React.useRef(0);
  const [historyLoading, setHistoryLoading] = React.useState(false);
  const currentChatId = React.useRef<string>(crypto.randomUUID());
  const completedTurns = React.useRef<ChatTurn[]>([]);
  const [pastTurns, setPastTurns] = React.useState<ChatTurn[]>([]);
  const [responseLabel, setResponseLabel] = React.useState('');
  const [historySearching, setHistorySearching] = React.useState(false);
  const [selected, setSelected] = React.useState<HistorySelection | null>(null);
  const [selector, setSelector] = React.useState<SelectorBegin | null>(null);

  const settingsRef = React.useRef(settings);
  settingsRef.current = settings;
  const streamingRef = React.useRef(false);
  const activeRequest = React.useRef<string | null>(null);
  const requestStarted = React.useRef(0);
  const requestHasImages = React.useRef(false);
  const firstTokenSeen = React.useRef(false);
  const lastPrompt = React.useRef<{ preset: string | null; question: string }>({ preset: null, question: '' });
  const [firstTokenSeconds, setFirstTokenSeconds] = React.useState<number | null>(null);
  const runAssistRef = React.useRef(runAssist);
  runAssistRef.current = runAssist;
  const beginRegionRef = React.useRef(beginRegionSelect);
  beginRegionRef.current = beginRegionSelect;
  const selectorSaved = React.useRef<{ x: number; y: number; w: number; h: number } | null>(null);
  const inputRef = React.useRef<HTMLTextAreaElement>(null);

  React.useEffect(() => {
    if (ready && panel === 'assist') {
      const t = setTimeout(() => {
        inputRef.current?.focus();
      }, 80);
      return () => clearTimeout(t);
    }
  }, [ready, panel]);

  function openPanel(next: Panel) {
    setPanel(next);
    if (next !== 'assist') void commands.overlayInteractive().catch((e) => setErr(String(e)));
  }

  React.useEffect(() => {
    const escape = (event: KeyboardEvent) => {
      if (event.key === 'Escape' && !selector) {
        if (panel !== 'assist') {
          event.preventDefault();
          setPanel('assist');
          void commands.overlayInteractive().catch((e) => setErr(String(e)));
        }
      }
    };
    window.addEventListener('keydown', escape);
    return () => window.removeEventListener('keydown', escape);
  }, [panel, selector]);

  // ---- initial load ----
  React.useEffect(() => {
    commands.settingsGet().then(s => setSettings({ ...DEFAULT_SETTINGS, ...s, profileInstructions: s.profileInstructions ?? {} })).catch(() => {}).finally(() => setReady(true));
    refreshHistory();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  async function refreshHistory() { await searchHistory(historyQueryRef.current); }

  function newChat() {
    if (streamingRef.current) return;
    currentChatId.current = crypto.randomUUID();
    completedTurns.current = [];
    setPastTurns([]); setAnswer(''); setQuestion(''); setActivePreset(null); setFirstTokenSeconds(null); setErr(null); setCopied(false);
    void clearCaptures();
    openPanel('assist');
  }

  // ---- backend events ----
  React.useEffect(() => {
    const subs: { (): void }[] = [];
    let disposed = false;
    let registered = 0;
    const probeId = crypto.randomUUID();
    let probeReceived = false;
    let probeTimer: ReturnType<typeof setTimeout> | null = null;
    const register = (unlisten: () => void) => {
      if (disposed) { unlisten(); return; }
      subs.push(unlisten);
      registered += 1;
      if (registered === Object.keys(eventNames).length) {
        void commands.diagnosticsProbe(probeId).catch(listenerFailed);
        probeTimer = setTimeout(() => {
          if (!disposed && !probeReceived) {
            reportDiagnostic('listener_failed', probeId, eventNames.diagnosticsProbe);
            listenerFailed();
          }
        }, 3000);
      }
    };
    const listenerFailed = () => {
      if (!disposed) {
        setEventsReady(false);
        setErr('Could not connect to app events. Open Diagnostics, then restart Notely.');
      }
    };
    events.onClickthroughChanged((payload) => {
      setSettings((s) => ({ ...s, clickThrough: payload.enabled }));
      if (payload.enabled) setPanel('assist');
    }).then(register).catch(listenerFailed);
    events.onDiagnosticsProbe((payload) => {
      if (disposed || payload.requestId !== probeId) return;
      probeReceived = true;
      if (probeTimer) clearTimeout(probeTimer);
      setEventsReady(true);
      reportDiagnostic('listeners_ready', probeId, eventNames.diagnosticsProbe);
    }).then(register).catch(listenerFailed);
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
    }).then(register).catch(listenerFailed);
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
    }).then(register).catch(listenerFailed);
    events.onTranscriptStatus((s) => setLiveStatus(s.status)).then(register).catch(listenerFailed);
    events.onCaptureTaken((img) => setCaptures((c) => [...c, img].slice(-4))).then(register).catch(listenerFailed);
    events.onAssistChunk((c) => {
      if (c.requestId !== activeRequest.current) return;
      if (!firstTokenSeen.current) {
        firstTokenSeen.current = true;
        reportDiagnostic('first_text', c.requestId, undefined, Math.round(performance.now() - requestStarted.current));
        setFirstTokenSeconds((performance.now() - requestStarted.current) / 1000);
      }
      setAnswer((a) => a + c.text);
      setAsking(false);
    }).then(register).catch(listenerFailed);
    events.onAssistDone((d) => {
      if (d.requestId !== activeRequest.current) return;
      reportDiagnostic(d.error ? 'request_failed' : d.cancelled ? 'request_cancelled' : 'request_finished', d.requestId, undefined, Math.round(performance.now() - requestStarted.current));
      activeRequest.current = null;
      streamingRef.current = false;
      setStreaming(false);
      setAsking(false);
      setAskingElapsed(0);
      if (d.error) setErr(d.error);
      if (!d.error && !d.cancelled) {
        const chatId = currentChatId.current;
        if (d.historySaved === false) setErr('Your answer is ready, but could not be saved to History. Check available disk space and Diagnostics.');
        else void commands.chatsGet(chatId).then(turns => { if (currentChatId.current === chatId) completedTurns.current = turns; }).catch(() => {});
        void refreshHistory();
      }
    }).then(register).catch(listenerFailed);
    events.onError((e) => {
      // Audio/session errors must not reset an unrelated assistant request.
      setErr(e.message);
    }).then(register).catch(listenerFailed);
    events.onStealthChanged((p) => {
      setSettings((s) => ({ ...s, contentProtection: p.stealth }));
    }).then(register).catch(listenerFailed);
    events.onHotkeyFocusChat(() => {
      setPanel('assist');
      setTimeout(() => { if (!disposed) inputRef.current?.focus(); }, 80);
    }).then(register).catch(listenerFailed);
    events.onHotkeyAssist(() => {
      if (!streamingRef.current) void runAssistRef.current(null);
    }).then(register).catch(listenerFailed);
    events.onHotkeyPreset((p) => {
      if (!streamingRef.current) void runAssistRef.current(p.preset);
    }).then(register).catch(listenerFailed);
    events.onHotkeyRegion(() => {
      void beginRegionRef.current();
    }).then(register).catch(listenerFailed);

    // Automatically focus the input field so users can type immediately
    inputRef.current?.focus();
    const handleWinFocus = () => {
      inputRef.current?.focus();
    };
    window.addEventListener('focus', handleWinFocus);

    return () => {
      disposed = true;
      if (probeTimer) clearTimeout(probeTimer);
      subs.forEach((u) => u());
      window.removeEventListener('focus', handleWinFocus);
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  // Auto-clear transient errors after a few seconds, but keep quota/model errors visible.
  React.useEffect(() => {
    if (!err) return;
    if (['permission', 'capture', 'screen', 'not ready'].some((s) => err.toLowerCase().includes(s)) || err.toLowerCase().includes('quota') || err.toLowerCase().includes('limit') || err.toLowerCase().includes('key') || err.toLowerCase().includes('model')) return;
    const t = setTimeout(() => setErr(null), 6000);
    return () => clearTimeout(t);
  }, [err]);

  React.useEffect(() => {
    if (!streaming) return;
    const timer = setInterval(() => {
      const id = activeRequest.current;
      if (id && performance.now() - requestStarted.current > (requestHasImages.current ? 115000 : 80000)) {
        reportDiagnostic('watchdog_timeout', id, undefined, Math.round(performance.now() - requestStarted.current));
        activeRequest.current = null;
        streamingRef.current = false;
        setStreaming(false);
        setAsking(false);
        setErr('No completion received before the request deadline. Open Diagnostics to see the last request stage.');
        void commands.assistCancel(id).catch(() => {});
      }
    }, 1000);
    return () => clearInterval(timer);
  }, [streaming]);

  React.useEffect(() => {
    const onError = () => reportDiagnostic('javascript_error', activeRequest.current ?? undefined);
    const onRejection = () => reportDiagnostic('unhandled_rejection', activeRequest.current ?? undefined);
    window.addEventListener('error', onError);
    window.addEventListener('unhandledrejection', onRejection);
    return () => { window.removeEventListener('error', onError); window.removeEventListener('unhandledrejection', onRejection); };
  }, []);

  // ---- meeting ----
  async function toggleMeeting() {
    if (meetingBusy) return;
    setMeetingBusy(true);
    setErr(null);
    try {
      if (recording) {
        const res = await commands.meetingStop();
        setRecording(false);
        setLiveStatus('idle');
        await refreshHistory();
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
    } finally { setMeetingBusy(false); }
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
    const id = activeRequest.current;
    if (id) reportDiagnostic('request_cancelled', id, undefined, Math.round(performance.now() - requestStarted.current));
    activeRequest.current = null;
    streamingRef.current = false;
    setStreaming(false);
    setAsking(false);
    setErr(null);
    if (id) void commands.assistCancel(id).catch((e) => setErr(String(e)));
  }

  async function runAssist(preset: string | null, slot?: 'fast' | 'smart', questionOverride?: string) {
    if (streamingRef.current || preferencesBusy) return;
    if (!eventsReady) { setErr('App event listeners are not ready. Check Diagnostics or restart Notely.'); return; }
    const currentQ = preset ? '' : (questionOverride ?? question) || 'What should I focus on?';
    lastPrompt.current = { preset, question: currentQ };
    const id = crypto.randomUUID();
    activeRequest.current = id;
    reportDiagnostic('request_started', id);
    requestHasImages.current = preset === 'screen' || captures.length > 0;
    requestStarted.current = performance.now();
    firstTokenSeen.current = false;
    setFirstTokenSeconds(null);
    setErr(null);
    setPastTurns(completedTurns.current);
    setResponseLabel(`${PROVIDERS.find(p=>p.id === settingsRef.current.assistantProvider)?.name} · ${PROFILES.find(p=>p.id === settingsRef.current.instructionProfile)?.name}`);
    setAnswer('');
    setStreaming(true);
    setAsking(true);
    setAskingElapsed(0);
    streamingRef.current = true;
    setActivePreset(preset);
    setPanel('assist');
    const s = settingsRef.current;
    try {
      await commands.assistRun({
        requestId: id,
        conversationId: currentChatId.current,
        question: currentQ,
        preset,
        includeScreenshot: preset === 'screen' || captures.length > 0,
        profileId: s.instructionProfile,
        customInstructions: s.customInstructions || null,
        slot: slot ?? (smartMode ? 'smart' : 'fast'),
      });
      reportDiagnostic('request_accepted', id, undefined, Math.round(performance.now() - requestStarted.current));
      if (!preset) setQuestion('');
    } catch (e) {
      reportDiagnostic('request_failed', id, undefined, Math.round(performance.now() - requestStarted.current));
      if (activeRequest.current !== id) return;
      activeRequest.current = null;
      streamingRef.current = false;
      setStreaming(false);
      setAsking(false);
      setErr(String(e));
    }
  }

  async function changeProvider(provider: Provider) {
    if (preferencesBusy || streamingRef.current) return;
    setPreferencesBusy(true);
    try {
      const next = { ...settingsRef.current, assistantProvider: provider };
      await commands.settingsSet(next); setSettings(next);
    } catch (e) { setErr(String(e)); }
    finally { setPreferencesBusy(false); }
  }
  async function changeProfile(profile: string) {
    if (preferencesBusy || streamingRef.current) return;
    setPreferencesBusy(true);
    try {
      const next = { ...settingsRef.current, instructionProfile: profile };
      await commands.settingsSet(next); setSettings(next);
    } catch (e) { setErr(String(e)); }
    finally { setPreferencesBusy(false); }
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
      await commands.captureRegion(rect.x, rect.y, rect.w, rect.h);
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
    historyQueryRef.current = q;
    const generation = ++searchGeneration.current;
    setHistorySearching(true);
    try {
      const [meetings, savedChats] = await Promise.all([q.trim() ? commands.historySearch(q.trim()) : commands.historyList(100), commands.chatsList(q.trim())]);
      if (generation !== searchGeneration.current) return;
      setHistory(meetings); setChats(savedChats);
      if (!q.trim()) setRecentChats(savedChats);
    } catch (e) { if (generation === searchGeneration.current) setErr(String(e)); }
    finally { if (generation === searchGeneration.current) setHistorySearching(false); }
  }
  async function selectHistory(entry: HistoryEntry) {
    const generation = ++selectionGeneration.current;
    setHistoryLoading(true);
    try {
      if (entry.kind === 'chat') {
        const turns = await commands.chatsGet(entry.chat.id);
        if (generation === selectionGeneration.current) setSelected({ kind: 'chat', summary: entry.chat, turns });
      } else {
        const row = await commands.historyGet(entry.meeting.id);
        if (row && generation === selectionGeneration.current) setSelected({ kind:'meeting', row, transcript:parseTranscript(row.transcriptJson), notes:parseNotes(row.notesJson) });
      }
    } catch (e) { setErr(String(e)); }
    finally { if (generation === selectionGeneration.current) setHistoryLoading(false); }
  }
  async function deleteHistory(entry: HistoryEntry) {
    try {
      if (entry.kind === 'chat') {
        await commands.chatsDelete(entry.chat.id);
        if (currentChatId.current === entry.chat.id) { newChat(); openPanel('history'); }
      } else { await commands.historyDelete(entry.meeting.id); }
      setSelected(null); await refreshHistory();
      if (historyQueryRef.current.trim()) setRecentChats(await commands.chatsList());
    } catch (e) { setErr(String(e)); throw e; }
  }
  function continueChat(selection: Extract<HistorySelection, {kind:'chat'}>) {
    if (streamingRef.current) return;
    const last = selection.turns[selection.turns.length - 1];
    if (!last) return;
    currentChatId.current = selection.summary.id;
    completedTurns.current = selection.turns;
    setPastTurns(selection.turns.slice(0,-1)); setAnswer(last.answer); setQuestion('');
    setActivePreset(null); lastPrompt.current = { preset:null, question:last.question };
    setResponseLabel(`${PROVIDERS.find(p=>p.id === last.provider)?.name} · ${PROFILES.find(p=>p.id === last.profileId)?.name}`);
    void changeProfile(last.profileId);
    setFirstTokenSeconds(null); setErr(null); void clearCaptures(); openPanel('assist');
  }

  // ---- window drag (any non-interactive area) ----
  function maybeDrag(e: React.MouseEvent) {
    if (e.button !== 0 || selector) return;
    if ((e.target as HTMLElement).closest('button,input,textarea,a,select,label,summary,details,[role=button],[data-nodrag]')) return;
    void getCurrentWindow().startDragging().catch(() => {});
  }

  if (!ready) return <div className="app-loading">Getting Notely ready…</div>;

  const glass = { background: `rgba(27,30,32,${selector ? 1 : settings.overlayOpacity})` };
  const providerName = PROVIDERS.find((p) => p.id === settings.assistantProvider)?.name;
  const activeProfile = PROFILES.find(p=>p.id === settings.instructionProfile) ?? PROFILES[0];
  const historyEntries: HistoryEntry[] = [...chats.map(chat => ({kind:'chat' as const,key:`chat:${chat.id}`,chat})), ...history.map(meeting => ({kind:'meeting' as const,key:`meeting:${meeting.id}`,meeting}))].sort((a,b)=>(b.kind === 'chat' ? b.chat.updatedAt : b.meeting.startedAt) - (a.kind === 'chat' ? a.chat.updatedAt : a.meeting.startedAt));
  const presetLabel = activePreset ? PRESETS.find((p) => p.id === activePreset)?.label : null;
  const navItems: { id: Panel; label: string; icon: string }[] = [
    { id: 'assist', label: 'Companion', icon: 'chat' },
    { id: 'transcript', label: 'Transcript', icon: 'wave' },
    { id: 'notes', label: 'Notes', icon: 'notes' },
    { id: 'history', label: 'History', icon: 'history' },
  ];

  return (
    <div className="app-frame">
      <main className={`companion-window ${settings.contentProtection ? 'protected-pointer' : ''}`} style={glass}>
        <header className="window-header" onMouseDown={maybeDrag}>
          <div className="brand"><span className="brand-mark"><Icon name="notebook" size={19} /></span><span>notely<span className="brand-caption">Your everyday companion</span></span></div>
          <div className="window-controls">
            <button aria-label={settings.contentProtection ? 'Protection requested' : 'Protection off'} className={`privacy-control ${settings.contentProtection ? 'requested' : ''}`} onClick={() => void toggleStealth()}
              title={settings.contentProtection ? 'Capture protection requested. Visibility depends on macOS and the sharing app; test before sharing.' : 'Request screen-capture protection. Compatibility depends on macOS and the sharing app.'}>
              <Icon name="shield" size={14} /><span>{settings.contentProtection ? 'Protection requested' : 'Protection off'}</span>
            </button>
            <WindowResize disabled={!!selector} onError={setErr} />
            <button className={`icon-button ${panel === 'settings' ? 'selected' : ''}`} aria-label="Settings" title="Settings" onClick={() => openPanel(panel === 'settings' ? 'assist' : 'settings')}><Icon name="settings" /></button>
            <button className="icon-button" aria-label="Hide Notely" title="Hide Notely · ⌘⇧H" onClick={() => void commands.overlayHide()}><Icon name="hide" /></button>
          </div>
        </header>

        {settings.clickThrough && <div className="input-mode-notice" role="status"><Icon name="screen" size={14} /><span>Mouse clicks pass through to the app underneath.</span><kbd>{shortcutLabel(settings.hotkeys.toggleClickThrough || DEFAULT_HOTKEYS.toggleClickThrough)}</kbd><span>to interact with Notely again</span></div>}
        <nav className="navigation" aria-label="Main navigation">
          <div className="navigation-tabs">{navItems.map((item) => <button key={item.id} className={`nav-tab ${panel === item.id ? 'active' : ''}`} aria-current={panel === item.id ? 'page' : undefined} onClick={() => openPanel(item.id)}><Icon name={item.icon} size={15} /><span>{item.label}</span></button>)}</div>
          <button className={`meeting-button ${recording ? 'is-recording' : ''}`} disabled={meetingBusy} onClick={() => void toggleMeeting()} title={recording ? 'Stop meeting and save session' : 'Start meeting transcription with Gemini'}><span className="status-dot" /><span>{meetingBusy ? 'One moment…' : recording ? (liveStatus === 'reconnecting' ? 'Reconnecting' : 'End meeting') : 'Start meeting'}</span></button>
        </nav>

        <SharingControls enabled={settings.contentProtection} hotkeys={settings.hotkeys} />
        {panel === 'assist' ? <>
          <div className="assistant-content" data-nodrag>
            {!answer && !asking && !err && pastTurns.length === 0 && <div className="welcome-view">
              <div className="welcome-heading"><div><p className="eyebrow">A little clarity, whenever you need it</p><h1>What’s on your mind?</h1><p className="welcome-description">Think it through, find the words, or get a fresh pair of eyes.</p></div></div>
              <div className="suggestion-grid">{PRESETS.map((p) => <button key={p.id} className="suggestion-card" disabled={!eventsReady || streaming} onClick={() => void runAssist(p.id)}><span className={`suggestion-icon ${p.id}`}><Icon name={PRESET_ICON[p.id]} size={18} /></span><span><strong>{p.label}</strong><small>{PRESET_DETAIL[p.id]}</small></span><span className="suggestion-arrow">↗</span></button>)}</div>
              <div className="home-profile"><span className="suggestion-icon"><Icon name={activeProfile.icon} size={17} /></span><div><strong>Your {activeProfile.name.toLowerCase()} companion</strong><p>{activeProfile.description}</p><small>{settings.profileInstructions[activeProfile.id]?.trim() ? 'Your personal instructions are included' : 'Add your context to make this profile more useful'}</small></div><button onClick={()=>openPanel('settings')}>Personalize ↗</button></div>
              <div className="home-context"><span className="status-dot ready" />{recording ? `Meeting connected · ${lines.filter(l=>l.isFinal).length} transcript lines` : 'AI chats are saved locally on this Mac'}<span>{captures.length ? `${captures.length} screen capture${captures.length === 1 ? '' : 's'} attached` : `${activeProfile.name} profile selected`}</span></div>
              {recentChats.length > 0 && <div className="home-recent"><div className="home-section-title"><span>Pick up where you left off</span><button onClick={()=>{setSelected(null);openPanel('history');}}>All history ↗</button></div>{recentChats.slice(0,2).map(chat=><button className="recent-chat" key={chat.id} disabled={streaming || historyLoading} onClick={()=>{openPanel('history');void selectHistory({kind:'chat',key:`chat:${chat.id}`,chat});}}><Icon name="chat" size={14} /><span>{chat.title}</span><small>{chat.turnCount} {chat.turnCount === 1 ? 'exchange' : 'exchanges'}</small><span>↗</span></button>)}</div>}

            </div>}

            {pastTurns.length > 0 && <div className="past-turns">{pastTurns.map(turn=><article className="past-turn" key={turn.requestId}><p className="user-question">{turn.question}</p><div className="turn-attribution"><Icon name="notebook" size={13} />Notely · {PROFILES.find(p=>p.id === turn.profileId)?.name}</div><ResponseView text={turn.answer} streaming={false} /></article>)}</div>}
            {asking && !answer && <div className="thinking-state" role="status"><span className="thinking-mark"><Icon name="notebook" size={24} /></span><h2>{requestHasImages.current ? 'Taking a closer look…' : 'Thinking it through…'}</h2><p>{providerName} · {askingElapsed.toFixed(1)}s</p><button className="text-button" onClick={cancelAssist}>Cancel request</button></div>}

            {answer && <section className="answer-section" aria-label="Assistant response">
              <div className="answer-toolbar"><span className="answer-identity"><span className="answer-mark"><Icon name="notebook" size={14} /></span>Notely<span className="answer-provider">{responseLabel || providerName}{streaming ? ' · Writing…' : ''}</span></span><div className="answer-tools"><button className="icon-button" onClick={() => void copyAnswer()} aria-label={copied ? 'Answer copied' : 'Copy answer'} title={copied ? 'Copied' : 'Copy answer'}><Icon name={copied ? 'check' : 'copy'} size={15} /></button><button className="icon-button" disabled={streaming} onClick={newChat} aria-label="New question" title="New question"><Icon name="plus" size={17} /></button></div></div>
              <div className="request-label">{presetLabel || lastPrompt.current.question}</div>
              <ResponseView text={answer} streaming={streaming} />
            </section>}
          </div>

          <div className="composer-area" data-nodrag>
            {(answer || asking) && <div className="quick-actions">{PRESETS.map((p) => <button key={p.id} disabled={streaming || !eventsReady} onClick={() => void runAssist(p.id)}><Icon name={PRESET_ICON[p.id]} size={13} />{p.label}</button>)}</div>}
            {captures.length > 0 && <CapturesBar captures={captures} onClear={() => void clearCaptures()} />}
            <div className="composer-context"><label htmlFor="chat-profile"><Icon name={activeProfile.icon} size={14} />Tailored for</label><select id="chat-profile" aria-label="Assistant profile" value={settings.instructionProfile} disabled={streaming || preferencesBusy} onChange={e=>void changeProfile(e.target.value)}>{PROFILES.map(profile=><option key={profile.id} value={profile.id}>{profile.name}</option>)}</select>{settings.profileInstructions[activeProfile.id]?.trim() && <span className="personalized-badge">Personalized</span>}<button onClick={()=>openPanel('settings')} title="Edit instructions for this profile">Customize</button>{(answer || pastTurns.length > 0) && <button className="new-chat-button" disabled={streaming} onClick={newChat}>New chat <Icon name="plus" size={12} /></button>}</div>
            <div className="composer">
              <textarea ref={inputRef} rows={2} value={question} aria-label="Your question" onChange={(e) => setQuestion(e.target.value)} onFocus={() => { commands.overlayFocus().catch(() => {}); }}
                onKeyDown={(e) => { if (e.key === 'Enter' && !e.shiftKey && !e.nativeEvent.isComposing) { e.preventDefault(); if (!streaming) void runAssist(null); } }}
                placeholder={recording ? 'Ask about your meeting, or anything else…' : 'Ask anything, I’m here to help…'} />
              <div className="composer-toolbar">
                <select aria-label="AI provider" value={settings.assistantProvider} disabled={streaming || preferencesBusy} onChange={(e) => void changeProvider(e.target.value as Provider)}>{PROVIDERS.map((p) => <option key={p.id} value={p.id}>{p.name}</option>)}</select>
                <span className="toolbar-divider" />
                <button className={`mode-button ${smartMode ? 'smart' : ''}`} onClick={() => setSmartMode((v) => !v)} disabled={streaming} title={smartMode ? 'Smart: use your configured smart model; responses may take longer' : 'Fast: quicker responses'}><Icon name={smartMode ? 'layers' : 'timer'} size={13} />{smartMode ? 'Smart' : 'Fast'}</button>
                <button className="icon-button capture-button" disabled={streaming} onClick={(e) => { e.stopPropagation(); void beginRegionSelect(); }} title="Capture a screen region · ⌘⇧A" aria-label="Capture a screen region"><Icon name="region" size={16} /></button>
                <div className="composer-spacer" />
                {streaming ? <button className="stop-button" onClick={cancelAssist} aria-label="Stop response" title="Stop response"><span /></button> : <button className="send-button" disabled={!eventsReady || preferencesBusy} onClick={() => void runAssist(null)} title="Send · Enter" aria-label="Send question"><Icon name="arrow" size={18} /></button>}
              </div>
            </div>
            <footer className="composer-footer"><span><span className={`status-dot ${eventsReady ? 'ready' : ''}`} />{eventsReady ? (recording ? 'Meeting context connected' : 'Ready when you are') : 'Connecting…'}</span><span>{firstTokenSeconds !== null ? `First text in ${firstTokenSeconds.toFixed(1)}s` : 'Enter to send · ⇧ Enter for a new line'}</span></footer>
          </div>
        </> : <div className="secondary-panel">
          <div className="panel-heading"><div><p className="eyebrow">YOUR WORKSPACE</p><h1>{panel === 'diagnostics' ? 'Diagnostics' : panel === 'settings' ? 'Make yourself at home' : panel.charAt(0).toUpperCase() + panel.slice(1)}</h1></div><button className="back-button" onClick={() => openPanel('assist')}>Back to companion <span>esc</span></button></div>
          {panel === 'transcript' && <TranscriptView lines={lines} recording={recording} />}
          {panel === 'notes' && <NotesPanel notes={notes} generating={notesGenerating} disabled={recording || lines.filter((l) => l.isFinal).length === 0} onGenerate={() => void generateNotes()} />}
          {panel === 'history' && <HistoryPanel entries={historyEntries} selected={selected} query={historyQuery} searching={historySearching} loading={historyLoading} busy={streaming} onQuery={q=>{setHistoryQuery(q);void searchHistory(q);}} onSelect={entry=>void selectHistory(entry)} onDelete={deleteHistory} onRefresh={()=>void refreshHistory()} onContinue={continueChat} onBack={()=>setSelected(null)} />}
          {panel === 'diagnostics' && <DiagnosticsPanel />}
          {panel === 'settings' && <SettingsPanel settings={settings} onClose={() => openPanel('assist')} onSave={async (s) => { const interactive = { ...s, clickThrough: false }; await commands.settingsSet(interactive); setSettings(interactive); await commands.registerHotkeys(s.hotkeys); }} />}
          {panel === 'settings' && <button className="diagnostics-link" onClick={() => openPanel('diagnostics')}>Troubleshooting? Open diagnostics <span>↗</span></button>}
        </div>}

        {err && <div role="alert" className="error-notice"><div><strong>Let’s get this sorted</strong><p>{err.replace(/^Error:\s*/, '')}</p></div><div className="error-actions"><button onClick={() => openPanel('diagnostics')}>View logs</button><button onClick={() => openPanel('settings')}>Settings</button><button className="icon-button" onClick={() => setErr(null)} aria-label="Dismiss error"><Icon name="close" size={14} /></button></div></div>}
      </main>
      {selector && <RegionSelector sel={selector} onDone={(r) => void finishRegionSelect(r)} />}
    </div>
  );
}
