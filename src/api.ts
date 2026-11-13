// Typed bridge to Rust. Keys are passed for storage only and are never returned.
import { invoke } from '@tauri-apps/api/core';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import eventNames from './events.json';
import type { AppSettings, AssistRequest, ChatSummary, ChatTurn, CapturedImage, DiagnosticsSnapshot, MeetingRow, ModelInfo, MonitorInfo, Provider, SelectorBegin, TranscriptLine } from './types';

export function reportDiagnostic(stage: string, requestId?: string, event?: string, elapsedMs?: number) {
  void invoke('diagnostics_frontend', { stage, requestId: requestId ?? null, event: event ?? null, elapsedMs: elapsedMs ?? null })
    .catch(() => console.warn('[Notely] Could not write diagnostic metadata.'));
}

function listenEvent<T>(name: string, handler: (event: { payload: T }) => void): Promise<UnlistenFn> {
  return listen<T>(name, handler).catch((error) => {
    reportDiagnostic('listener_failed', undefined, name);
    throw error;
  });
}

export const commands = {
  diagnosticsRecent: () => invoke<DiagnosticsSnapshot>('diagnostics_recent'),
  diagnosticsClear: () => invoke<void>('diagnostics_clear'),
  diagnosticsProbe: (requestId: string) => invoke<void>('diagnostics_probe', { requestId }),
  screenPermissionGet: () => invoke<{ granted: boolean; platform: string }>('screen_permission_get'),
  screenPermissionRequest: () => invoke<{ granted: boolean; platform: string }>('screen_permission_request'),
  screenPermissionOpenSettings: () => invoke<void>('screen_permission_open_settings'),
  settingsGet: () => invoke<AppSettings>('settings_get'),
  settingsSet: (s: AppSettings) => invoke<void>('settings_set', { s }),
  keyHas: (provider: Provider) => invoke<boolean>('key_has', { provider }),
  keySet: (provider: Provider, key: string) => invoke<void>('key_set', { provider, key }),
  keyClear: (provider: Provider) => invoke<void>('key_clear', { provider }),
  keyTest: (provider: Provider, model: string) => invoke<string>('key_test', { provider, model }),
  modelsList: (provider: Provider) => invoke<ModelInfo[]>('models_list', { provider }),
  captureFull: (monitorIndex?: number) => invoke<CapturedImage>('capture_full_cmd', { monitorIndex: monitorIndex ?? null }),
  captureActive: () => invoke<CapturedImage>('capture_active_cmd'),
  captureRegion: (x: number, y: number, w: number, h: number) =>
    invoke<CapturedImage>('capture_region_cmd', { x, y, w, h }),
  captureClear: () => invoke<void>('capture_clear'),
  monitorsList: () => invoke<MonitorInfo[]>('monitors_list'),
  selectorBegin: () => invoke<SelectorBegin>('selector_begin'),
  selectorEnd: (saved?: { x: number; y: number; w: number; h: number } | null) =>
    invoke<void>('selector_end', { saved: saved ?? null }),
  meetingStart: () => invoke<void>('meeting_start'),
  meetingStop: () => invoke<{ saved?: boolean; empty?: boolean; lines?: number }>('meeting_stop'),
  meetingTranscriptAdd: (line: TranscriptLine) => invoke<void>('meeting_transcript_add', { line }),
  assistRun: (req: AssistRequest) => invoke<void>('assist_run', { req }),
  assistCancel: (requestId: string) => invoke<void>('assist_cancel', { requestId }),
  notesGenerate: () => invoke<Record<string, unknown>>('notes_generate'),
  chatsList: (query = '') => invoke<ChatSummary[]>('chats_list', { query }),
  chatsGet: (id: string) => invoke<ChatTurn[]>('chats_get', { id }),
  chatsDelete: (id: string) => invoke<void>('chats_delete', { id }),
  historyList: (limit?: number) => invoke<MeetingRow[]>('history_list', { limit: limit ?? null }),
  historySearch: (q: string) => invoke<MeetingRow[]>('history_search', { q }),
  historyGet: (id: number) => invoke<MeetingRow | null>('history_get', { id }),
  historyDelete: (id: number) => invoke<void>('history_delete', { id }),
  overlayHide: () => invoke<void>('overlay_hide'),
  overlayShow: () => invoke<void>('overlay_show'),
  overlayInteractive: () => invoke<void>('overlay_interactive'),
  overlayToggleClickthrough: () => invoke<boolean>('overlay_toggle_clickthrough'),
  overlayToggleStealth: () => invoke<boolean>('overlay_toggle_stealth'),
  overlaySetStealth: (enabled: boolean) => invoke<boolean>('overlay_set_stealth', { enabled }),
  overlayFocus: () => invoke<void>('overlay_focus'),
  registerHotkeys: (hotkeys: Record<string, string>) => invoke<void>('register_hotkeys_cmd', { hotkeys }),
};

export const events = {
  onClickthroughChanged: (cb: (p: { enabled: boolean }) => void) => listenEvent<{ enabled: boolean }>(eventNames.clickthroughChanged, (e) => cb(e.payload)),
  onDiagnosticsProbe: (cb: (p: { requestId: string }) => void) => listenEvent<{ requestId: string }>(eventNames.diagnosticsProbe, (e) => cb(e.payload)),
  onTranscriptPartial: (cb: (line: TranscriptLine) => void) => listenEvent<TranscriptLine>(eventNames.transcriptPartial, (e) => cb(e.payload)),
  onTranscriptFinal: (cb: (line: TranscriptLine) => void) => listenEvent<TranscriptLine>(eventNames.transcriptFinal, (e) => cb(e.payload)),
  onTranscriptStatus: (cb: (s: { status: string }) => void) =>
    listenEvent<{ status: string }>(eventNames.transcriptStatus, (e) => cb(e.payload)),
  onCaptureTaken: (cb: (img: CapturedImage) => void) => listenEvent<CapturedImage>(eventNames.captureTaken, (e) => cb(e.payload)),
  onAssistChunk: (cb: (c: { requestId: string; text: string }) => void) => listenEvent<{ requestId: string; text: string }>(eventNames.assistChunk, (e) => cb(e.payload)),
  onAssistDone: (cb: (d: { requestId: string; cancelled?: boolean; error?: string; historySaved?: boolean }) => void) =>
    listenEvent<{ requestId: string; cancelled?: boolean; error?: string; historySaved?: boolean }>(eventNames.assistDone, (e) => cb(e.payload)),
  onError: (cb: (e: { message: string }) => void) => listenEvent<{ message: string }>(eventNames.error, (e) => cb(e.payload)),
  onStealthChanged: (cb: (payload: { stealth: boolean }) => void) =>
    listenEvent<{ stealth: boolean }>(eventNames.stealthChanged, (e) => cb(e.payload)),
  onHotkeyFocusChat: (cb: () => void) => listenEvent(eventNames.hotkeyFocusChat, () => cb()),
  onHotkeyAssist: (cb: () => void) => listenEvent(eventNames.hotkeyAssist, () => cb()),
  onHotkeyRegion: (cb: () => void) => listenEvent(eventNames.hotkeyRegion, () => cb()),
  onHotkeyPreset: (cb: (p: { preset: string }) => void) => listenEvent<{ preset: string }>(eventNames.hotkeyPreset, (e) => cb(e.payload)),
};

export type { UnlistenFn };
