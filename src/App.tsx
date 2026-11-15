import * as React from 'react';
import { getCurrentWindow } from '@tauri-apps/api/window';
import { commands, events } from './api';
import { HistoryPanel, NotesPanel, SettingsPanel } from './components/panels';
import { RegionSelector, type SelRect } from './components/RegionSelector';
import { CapturesBar, TranscriptView } from './components/views';
import { ResponseView } from './components/ResponseView';
import {
  DEFAULT_HOTKEYS,
  PRESETS,
  PROFILES,
  PROVIDERS,
  type AppSettings,
  type CapturedImage,
  type MeetingRow,
  type NotesJson,
  type Provider,
  type SelectorBegin,
  type TranscriptLine,
} from './types';

if (typeof window !== 'undefined' && !(window as any).__NOTELY_LOG_HOOKED__) {
  (window as any).__NOTELY_LOG_HOOKED__ = true;
  const origLog = console.log;
  console.log = (...args: any[]) => {
    origLog(...args);
    commands.feLog(args.map((a) => (typeof a === 'object' ? JSON.stringify(a) : String(a))).join(' ')).catch(() => {});
  };
  const origErr = console.error;
  console.error = (...args: any[]) => {
    origErr(...args);
    commands.feLog('[ERROR] ' + args.map((a) => (typeof a === 'object' ? JSON.stringify(a) : String(a))).join(' ')).catch(() => {});
  };
  window.addEventListener('error', (e) => {
    commands.feLog(`[UNHANDLED] ${e.message} at ${e.filename}:${e.lineno}`).catch(() => {});
  });
}

type Panel = 'assist' | 'transcript' | 'notes' | 'history' | 'settings';

const DEFAULT_SETTINGS: AppSettings = {
  assistantProvider: 'gemini',
  openaiFastModel: 'gpt-4.1-mini',
  openaiSmartModel: 'gpt-4.1',
  claudeFastModel: 'claude-haiku-4-5-20251001',
  claudeSmartModel: 'claude-sonnet-4-6',
  transcriptionProvider: 'openai',
  openaiTranscriptionModel: 'gpt-live-transcribe',
  liveModel: 'gemini-2.5-flash-native-audio-preview-12-2025',
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

  const [notes, setNotes] = React.useState<NotesJson | null>(null);
  const [notesGenerating, setNotesGenerating] = React.useState(false);
  const [selector, setSelector] = React.useState<SelectorBegin | null>(null);

  const currentChatId = React.useRef<string>(crypto.randomUUID());
  const activeRequestId = React.useRef<string | null>(null);
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

  // Load initial settings
  React.useEffect(() => {
    commands
      .settingsGet()
      .then((s) => {
        setSettings(s);
        setReady(true);
      })
      .catch((e) => {
        console.error('settingsGet failed:', e);
        setReady(true);
      });
  }, []);

  // Event listeners
  React.useEffect(() => {
    const unsubs: (() => void)[] = [];

    // Streaming chunks from multi-provider backend
    events.onAssistChunk((c) => {
      if (activeRequestId.current && c.requestId === activeRequestId.current) {
        setAnswer((prev) => prev + c.text);
        setStreaming(true);
        streamingRef.current = true;
      }
    }).then((u) => unsubs.push(u));

    // Streaming done from multi-provider backend
    events.onAssistDone((d) => {
      if (activeRequestId.current && d.requestId === activeRequestId.current) {
        setStreaming(false);
        setAsking(false);
        streamingRef.current = false;
        activeRequestId.current = null;
        if (d.error && !d.cancelled) {
          setErr(d.error);
        }
      }
    }).then((u) => unsubs.push(u));

    // Transcription events
    events.onTranscriptPartial((l) => {
      setLines((prev) => {
        const i = prev.findIndex((x) => x.id === l.id);
        if (i >= 0) {
          const next = [...prev];
          next[i] = l;
          return next;
        }
        return [...prev, l];
      });
    }).then((u) => unsubs.push(u));

    events.onTranscriptFinal((l) => {
      setLines((prev) => {
        const i = prev.findIndex((x) => x.id === l.id);
        if (i >= 0) {
          const next = [...prev];
          next[i] = l;
          return next;
        }
        return [...prev, l];
      });
    }).then((u) => unsubs.push(u));

    events.onTranscriptStatus((s) => {
      setLiveStatus(s.status);
    }).then((u) => unsubs.push(u));

    // Screen captures
    events.onCaptureTaken((img) => {
      setCaptures((prev) => [...prev, img]);
    }).then((u) => unsubs.push(u));

    // Stealth / protection changes
    events.onStealthChanged((payload) => {
      setSettings((s) => ({ ...s, contentProtection: payload.stealth }));
    }).then((u) => unsubs.push(u));

    // Global hotkeys
    events.onHotkeyFocusChat(() => {
      commands.overlayFocus().catch(() => {});
      inputRef.current?.focus();
    }).then((u) => unsubs.push(u));

    events.onHotkeyAssist(() => {
      void runAssist(null);
    }).then((u) => unsubs.push(u));

    events.onHotkeyRegion(() => {
      void beginRegionSelect();
    }).then((u) => unsubs.push(u));

    events.onHotkeyPreset((p) => {
      void runAssist(p.preset);
    }).then((u) => unsubs.push(u));

    return () => {
      unsubs.forEach((u) => u());
    };
  }, []);

  // Sync tray state
  React.useEffect(() => {
    if (!ready) return;
    commands.trayUpdate({
      ready,
      streaming,
      meetingBusy: recording,
      selecting: !!selector,
    }).catch(() => {});
  }, [ready, streaming, recording, selector]);

  // ---- assist runner ----
  async function runAssist(preset: string | null = null, slotOverride?: 'fast' | 'smart') {
    if (asking && !streamingRef.current) return;
    setErr(null);

    const s = settingsRef.current;
    const isScreenPreset = preset === 'screen';
    const hasCaptures = captures.length > 0;
    const includeScreenshot = isScreenPreset || (s.autoAttachScreenshot && (hasCaptures || isScreenPreset));
    const includeMeeting = recording || lines.length > 0;
    const slot = slotOverride ?? (smartMode ? 'smart' : 'fast');

    const id = crypto.randomUUID();
    activeRequestId.current = id;

    setAnswer('');
    setAsking(true);
    setStreaming(false);
    streamingRef.current = false;
    setActivePreset(preset);
    setPanel('assist');

    try {
      await commands.assistRun({
        requestId: id,
        conversationId: currentChatId.current,
        question: question.trim(),
        preset: preset ?? undefined,
        includeScreenshot,
        includeMeeting,
        profileId: s.instructionProfile,
        customInstructions: s.customInstructions || null,
        slot,
      });
      if (!preset) setQuestion('');
    } catch (e) {
      setAsking(false);
      setStreaming(false);
      streamingRef.current = false;
      activeRequestId.current = null;
      setErr(String(e));
    }
  }

  function cancelAssist() {
    if (activeRequestId.current) {
      commands.assistCancel(activeRequestId.current).catch(() => {});
    }
    setAsking(false);
    setStreaming(false);
    streamingRef.current = false;
    activeRequestId.current = null;
  }

  // ---- stealth toggle ----
  async function toggleStealth() {
    try {
      const next = await commands.overlayToggleStealth();
      setSettings((s) => ({ ...s, contentProtection: next }));
    } catch (e) {
      setErr(String(e));
    }
  }

  // ---- meeting toggle ----
  async function toggleMeeting() {
    setErr(null);
    if (recording) {
      try {
        await commands.meetingStop();
        setRecording(false);
        setLiveStatus('idle');
      } catch (e) {
        setErr(String(e));
      }
    } else {
      try {
        setLines([]);
        setNotes(null);
        await commands.meetingStart();
        setRecording(true);
        setLiveStatus('connecting');
      } catch (e) {
        setErr(String(e));
      }
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
      await commands.captureRegion(rect.x, rect.y, rect.w, rect.h);
    } catch (e) {
      setErr(String(e));
    }
  }

  async function clearCaptures() {
    await commands.captureClear().catch(() => {});
    setCaptures([]);
  }

  // ---- notes ----
  async function generateNotes() {
    setErr(null);
    setNotesGenerating(true);
    try {
      const n = await commands.notesGenerate();
      setNotes(n as unknown as NotesJson);
    } catch (e) {
      setErr(String(e));
    } finally {
      setNotesGenerating(false);
    }
  }

  // ---- provider & profile switcher ----
  async function updateProvider(provider: Provider) {
    const next = { ...settings, assistantProvider: provider };
    setSettings(next);
    await commands.settingsSet(next).catch(() => {});
  }

  async function updateProfile(profileId: string) {
    const next = { ...settings, instructionProfile: profileId };
    setSettings(next);
    await commands.settingsSet(next).catch(() => {});
  }

  // ---- window drag (any non-interactive area) ----
  function maybeDrag(e: React.MouseEvent) {
    if (e.button !== 0 || selector) return;
    if ((e.target as HTMLElement).closest('button,input,textarea,a,select,[data-nodrag],.select-text')) return;
    getCurrentWindow().startDragging().catch((err) => {
      console.error('[App] startDragging error:', err);
    });
  }

  if (!ready) return null;

  const opacity = selector ? 1 : settings.overlayOpacity;
  const glass = {
    background: `rgba(16,18,24,${opacity})`,
    backdropFilter: 'blur(20px)',
    WebkitBackdropFilter: 'blur(20px)',
  };
  const presetLabel = activePreset ? PRESETS.find((p) => p.id === activePreset)?.label ?? activePreset : null;
  const activeProfile = PROFILES.find((p) => p.id === settings.instructionProfile) ?? PROFILES[0];
  const activeProvider = PROVIDERS.find((p) => p.id === settings.assistantProvider) ?? PROVIDERS[0];

  return (
    <div
      className="h-full flex flex-col items-center pt-2 pb-3 px-4 select-none"
      data-tauri-drag-region="deep"
      onMouseDown={maybeDrag}
      style={{ cursor: 'default' }}
    >
      {/* Detached top pill: stealth controls */}
      <div
        className="flex items-center gap-1 pl-1.5 pr-1.5 py-1 rounded-full border border-white/15 shadow-2xl cursor-grab active:cursor-grabbing"
        data-tauri-drag-region="deep"
        style={glass}
      >
        <div
          className={`w-7 h-7 rounded-full flex items-center justify-center text-[15px] ${recording ? 'bg-red-500/25' : 'bg-white/10'}`}
          title={recording ? (liveStatus === 'reconnecting' ? 'Reconnecting…' : 'Recording') : 'Idle'}
        >
          {recording ? <span className="w-2 h-2 rounded-full bg-red-400 animate-pulse" /> : <span className="text-white/80">✦</span>}
        </div>
        <button
          onClick={() => void toggleStealth()}
          title={
            settings.contentProtection
              ? 'Stealth Mode is ACTIVE (hidden from screen share & Dock). Click to deactivate.'
              : 'Stealth Mode is OFF (visible in screen share & Dock). Click to activate.'
          }
          className={`flex items-center gap-1.5 px-3 py-1 rounded-full text-[12px] font-medium transition-all ${
            settings.contentProtection
              ? 'bg-emerald-500/20 border border-emerald-400/35 text-emerald-300 hover:bg-emerald-500/30'
              : 'bg-amber-500/15 border border-amber-400/30 text-amber-200 hover:bg-amber-500/25'
          }`}
        >
          <span className="text-[12px]">{settings.contentProtection ? '🥷' : '👀'}</span>
          <span>{settings.contentProtection ? 'Stealth' : 'Visible'}</span>
        </button>
        <button
          onClick={() => void commands.overlayHide()}
          title="Hide overlay (global hotkey brings it back)"
          className="flex items-center gap-1 px-3 py-1 rounded-full text-[13px] text-white/85 hover:bg-white/10"
        >
          <span className="text-white/50 text-[11px]">∨</span> Hide
        </button>
        <button
          onClick={() => void toggleMeeting()}
          title={recording ? 'Stop meeting' : 'Start meeting'}
          className={`w-7 h-7 rounded-full flex items-center justify-center ${recording ? 'bg-red-500/80 hover:bg-red-500' : 'bg-white/10 hover:bg-white/20'}`}
        >
          {recording ? (
            <span className="w-2.5 h-2.5 rounded-[3px] bg-white" />
          ) : (
            <span className="w-2.5 h-2.5 rounded-full bg-white/85" />
          )}
        </button>
      </div>

      {/* Main glass card */}
      <div
        className="w-full max-w-[760px] mt-2 rounded-2xl border border-white/12 shadow-2xl flex flex-col min-h-0 overflow-hidden"
        style={glass}
      >
        {/* Subtle drag handle bar at the top of the main card */}
        <div
          data-tauri-drag-region="true"
          className="w-full flex items-center justify-center pt-2 pb-0.5 cursor-grab active:cursor-grabbing group"
          title="Drag to move Notely"
        >
          <div className="w-10 h-1 rounded-full bg-white/20 group-hover:bg-white/45 transition" />
        </div>

        {panel === 'assist' && (
          <>
            {/* Active preset pill */}
            {presetLabel && (
              <div className="flex justify-end px-4 pt-3">
                <span className="px-3 py-1 rounded-full bg-blue-600/90 text-white text-[13px] font-medium shadow">
                  {presetLabel}
                </span>
              </div>
            )}

            {/* Answer area */}
            <div className="px-5 pt-3 pb-2 max-h-[380px] overflow-y-auto">
              {!answer ? (
                <div className="text-[13.5px] leading-relaxed text-white/55 text-center py-5">
                  {asking ? (
                    <span className="flex items-center justify-center gap-2 text-white/80">
                      <span className="w-2 h-2 rounded-full bg-blue-400 animate-pulse" />
                      Thinking… ({askingElapsed.toFixed(1)}s)
                      <button
                        onClick={cancelAssist}
                        className="text-[11px] text-white/75 hover:text-white bg-white/10 hover:bg-white/20 px-2 py-0.5 rounded-full ml-1 transition"
                      >
                        Cancel
                      </button>
                    </span>
                  ) : err ? (
                    <div className="flex flex-col items-center justify-center gap-1.5 py-1 text-center select-text">
                      <span className="text-amber-400 text-sm font-medium">
                        ⚠️ {err.toLowerCase().includes('quota') || err.toLowerCase().includes('limit') ? 'Model Quota Limit' : 'Assist Error'}
                      </span>
                      <span className="text-[12.5px] text-white/80 max-w-lg leading-relaxed">
                        {err.replace(/^Error:\s*/, '')}
                      </span>
                      <div className="flex items-center gap-2 mt-2">
                        <button
                          onClick={() => {
                            setErr(null);
                            void runAssist(null, 'fast');
                          }}
                          className="px-3 py-1 rounded-lg bg-blue-600/80 hover:bg-blue-600 text-white text-[12px] font-medium shadow"
                        >
                          ⚡ Try Fast Model
                        </button>
                        <button
                          onClick={() => setPanel('settings')}
                          className="px-3 py-1 rounded-lg bg-white/10 hover:bg-white/20 text-white/90 text-[12px]"
                        >
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
                <ResponseView
                  text={answer}
                  streaming={streaming}
                  onClear={() => {
                    setActivePreset(null);
                    setAnswer('');
                  }}
                />
              )}
            </div>

            {/* Quick action pills */}
            <div className="flex items-center justify-center gap-0.5 px-3 py-1.5 text-[13px] text-white/70 flex-wrap">
              <button
                onClick={() => {
                  setActivePreset(null);
                  setAnswer('');
                }}
                className="flex items-center gap-1 px-2 py-1 rounded-lg hover:bg-white/10 hover:text-white/90"
                title="Clear answer"
              >
                <span>✨</span> Assist
              </button>
              {PRESETS.map((p) => (
                <span key={p.id} className="flex items-center">
                  <span className="text-white/20">·</span>
                  <button
                    onClick={() => void runAssist(p.id)}
                    disabled={asking}
                    className={`flex items-center gap-1 px-2 py-1 rounded-lg hover:bg-white/10 hover:text-white/90 disabled:opacity-50 ${
                      activePreset === p.id ? 'text-white/95' : ''
                    }`}
                  >
                    <span>{PRESET_ICON[p.id] ?? '•'}</span> {p.label}
                  </button>
                </span>
              ))}
            </div>

            {/* Live transcript (collapsible) + captures */}
            {showTranscript && <TranscriptView lines={lines} recording={recording} />}
            {captures.length > 0 && <CapturesBar captures={captures} onClear={() => void clearCaptures()} />}

            {/* Input row */}
            <div
              className="px-4 pb-4 pt-1"
              onClick={() => {
                commands.overlayFocus().catch(() => {});
                inputRef.current?.focus();
              }}
            >
              <div className="rounded-2xl border border-white/12 bg-white/[0.06] px-3.5 pt-2.5 pb-2">
                <input
                  ref={inputRef}
                  value={question}
                  onChange={(e) => setQuestion(e.target.value)}
                  onFocus={() => {
                    commands.overlayFocus().catch(() => {});
                  }}
                  onKeyDown={(e) => {
                    if (e.key === 'Enter') {
                      e.preventDefault();
                      void runAssist(null);
                    }
                  }}
                  placeholder="Ask anything… (Press Enter or ⌘↵ to send)"
                  className="w-full bg-transparent outline-none text-[13.5px] text-white/90 placeholder:text-white/40"
                />

                <div className="flex items-center gap-1.5 mt-2 flex-wrap">
                  {/* Smart / Fast mode */}
                  <button
                    onClick={() => setSmartMode((v) => !v)}
                    title={smartMode ? 'Smart model (slower, stronger)' : 'Fast model'}
                    className={`flex items-center gap-1 px-2.5 py-1 rounded-full text-[12px] border ${
                      smartMode
                        ? 'bg-blue-600/80 border-blue-500/50 text-white'
                        : 'bg-white/5 border-white/10 text-white/60 hover:text-white/85'
                    }`}
                  >
                    <span>⚡</span> Smart
                  </button>

                  {/* Provider switcher */}
                  <select
                    value={settings.assistantProvider}
                    onChange={(e) => void updateProvider(e.target.value as Provider)}
                    className="bg-white/10 hover:bg-white/15 text-white/80 text-[11.5px] px-2 py-1 rounded-full border border-white/10 outline-none cursor-pointer [&>option]:text-black"
                    title="Choose AI provider"
                  >
                    {PROVIDERS.map((p) => (
                      <option key={p.id} value={p.id}>
                        {p.name}
                      </option>
                    ))}
                  </select>

                  {/* Profile switcher */}
                  <select
                    value={settings.instructionProfile}
                    onChange={(e) => void updateProfile(e.target.value)}
                    className="bg-white/10 hover:bg-white/15 text-white/80 text-[11.5px] px-2 py-1 rounded-full border border-white/10 outline-none cursor-pointer [&>option]:text-black"
                    title="Choose assistant profile"
                  >
                    {PROFILES.map((p) => (
                      <option key={p.id} value={p.id}>
                        {p.name}
                      </option>
                    ))}
                  </select>

                  {/* ... Menu */}
                  <div className="relative">
                    <button
                      onClick={() => setMenuOpen((v) => !v)}
                      title="More options"
                      className="px-2 py-1 rounded-full text-[12px] bg-white/5 hover:bg-white/10 border border-white/10 text-white/60 hover:text-white"
                    >
                      •••
                    </button>
                    {menuOpen && (
                      <div
                        className="absolute left-0 bottom-full mb-1.5 w-44 rounded-xl border border-white/15 bg-neutral-900/95 backdrop-blur-xl shadow-2xl py-1 text-[12px] text-white/80 z-20"
                        onMouseLeave={() => setMenuOpen(false)}
                      >
                        <button
                          onClick={() => {
                            setMenuOpen(false);
                            setPanel('settings');
                          }}
                          className="w-full text-left px-3 py-1.5 hover:bg-white/10"
                        >
                          ⚙️ Settings
                        </button>
                        <button
                          onClick={() => {
                            setMenuOpen(false);
                            setPanel('history');
                          }}
                          className="w-full text-left px-3 py-1.5 hover:bg-white/10"
                        >
                          🕒 History & Chats
                        </button>
                        <button
                          onClick={() => {
                            setMenuOpen(false);
                            setPanel('notes');
                          }}
                          className="w-full text-left px-3 py-1.5 hover:bg-white/10"
                        >
                          📝 Meeting Notes
                        </button>
                        <button
                          onClick={() => {
                            setMenuOpen(false);
                            setShowTranscript((v) => !v);
                          }}
                          className="w-full text-left px-3 py-1.5 hover:bg-white/10"
                        >
                          🎙️ {showTranscript ? 'Hide Transcript' : 'Show Transcript'}
                        </button>
                        <div className="border-t border-white/10 my-1" />
                        <button
                          onClick={() => {
                            setMenuOpen(false);
                            currentChatId.current = crypto.randomUUID();
                            setAnswer('');
                            setActivePreset(null);
                          }}
                          className="w-full text-left px-3 py-1.5 hover:bg-white/10 text-sky-300"
                        >
                          + New Chat
                        </button>
                      </div>
                    )}
                  </div>

                  {/* Region capture button */}
                  <button
                    onClick={() => void beginRegionSelect()}
                    title="Select screen region"
                    className="p-1 rounded-full text-[13px] bg-white/5 hover:bg-white/10 border border-white/10 text-white/60 hover:text-white"
                  >
                    ⌖
                  </button>

                  <div className="flex-1" />

                  {captures.length > 0 && (
                    <span className="text-[11px] text-white/35">
                      {captures.length} shot{captures.length > 1 ? 's' : ''}
                    </span>
                  )}

                  {/* Send or Stop button */}
                  {streaming ? (
                    <button
                      onClick={cancelAssist}
                      title="Stop response"
                      className="w-8 h-8 rounded-full bg-red-600 hover:bg-red-500 flex items-center justify-center text-white shadow"
                    >
                      <span className="w-2.5 h-2.5 rounded-[2px] bg-white" />
                    </button>
                  ) : (
                    <button
                      onClick={() => void runAssist(null)}
                      disabled={asking}
                      title="Ask (⌘/Ctrl+Enter)"
                      className="w-8 h-8 rounded-full bg-blue-600 hover:bg-blue-500 disabled:opacity-50 flex items-center justify-center text-white shadow"
                    >
                      <span className="text-[13px] ml-0.5">▶</span>
                    </button>
                  )}
                </div>
              </div>
            </div>
          </>
        )}

        {/* Secondary panels render inside the same card */}
        {panel !== 'assist' && (
          <div className="flex flex-col min-h-0 max-h-[460px]">
            <div className="flex items-center gap-2 px-4 pt-3 pb-1 border-b border-white/10">
              <button
                onClick={() => setPanel('assist')}
                className="text-white/60 hover:text-white text-[13px] flex items-center gap-1 font-medium"
              >
                ← Assist
              </button>
              <span className="text-[13px] text-white/80 capitalize">
                {panel === 'transcript' ? 'Transcript' : panel === 'history' ? 'History & Chats' : panel}
              </span>
            </div>

            {panel === 'transcript' && <TranscriptView lines={lines} recording={recording} />}

            {panel === 'notes' && (
              <NotesPanel
                notes={notes}
                generating={notesGenerating}
                disabled={lines.filter((l) => l.isFinal).length === 0}
                onGenerate={() => void generateNotes()}
              />
            )}

            {panel === 'history' && (
              <HistoryPanel
                onContinueChat={(chatId, turns) => {
                  currentChatId.current = chatId;
                  const lastTurn = turns[turns.length - 1];
                  if (lastTurn) {
                    setAnswer(lastTurn.answer);
                  }
                  setPanel('assist');
                }}
              />
            )}

            {panel === 'settings' && (
              <SettingsPanel
                settings={settings}
                onClose={() => setPanel('assist')}
                onSave={async (s) => {
                  const interactive = { ...s, clickThrough: false };
                  await commands.settingsSet(interactive);
                  setSettings(interactive);
                  await commands.registerHotkeys(s.hotkeys);
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
                <button
                  onClick={() => setPanel('settings')}
                  className="px-2 py-0.5 rounded bg-white/10 hover:bg-white/20 text-white text-[11px]"
                >
                  Open Settings
                </button>
              )}
              <button
                onClick={() => setErr(null)}
                className="px-1.5 py-0.5 rounded text-white/50 hover:text-white text-[11px]"
                title="Dismiss"
              >
                ✕
              </button>
            </div>
          </div>
        )}
      </div>

      {/* Region selector overlays everything while active */}
      {selector && <RegionSelector sel={selector} onDone={(r) => void finishRegionSelect(r)} />}
    </div>
  );
}
