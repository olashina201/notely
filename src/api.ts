// Typed bridge to the Rust backend: Tauri commands + events. The API key never reaches this layer.
import { invoke } from '@tauri-apps/api/core';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import type { AppSettings, AssistRequest, CapturedImage, MeetingRow, ModelInfo, MonitorInfo, SelectorBegin, TranscriptLine } from './types';

export const commands = {
  settingsGet: () => invoke<AppSettings>('settings_get'),
  settingsSet: (s: AppSettings) => invoke<void>('settings_set', { s }),
  keyHas: () => invoke<boolean>('key_has'),
  keySet: (key: string) => invoke<void>('key_set', { key }),
  keyClear: () => invoke<void>('key_clear'),
  keyTest: () => invoke<string>('key_test'),
  modelsList: () => invoke<ModelInfo[]>('models_list'),
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
  assistCancel: () => invoke<void>('assist_cancel'),
  notesGenerate: () => invoke<Record<string, unknown>>('notes_generate'),
  historyList: (limit?: number) => invoke<MeetingRow[]>('history_list', { limit: limit ?? null }),
  historySearch: (q: string) => invoke<MeetingRow[]>('history_search', { q }),
  historyGet: (id: number) => invoke<MeetingRow | null>('history_get', { id }),
  historyDelete: (id: number) => invoke<void>('history_delete', { id }),
  overlayHide: () => invoke<void>('overlay_hide'),
  overlayShow: () => invoke<void>('overlay_show'),
  overlayToggleClickthrough: () => invoke<boolean>('overlay_toggle_clickthrough'),
  overlayToggleStealth: () => invoke<boolean>('overlay_toggle_stealth'),
  overlaySetStealth: (enabled: boolean) => invoke<boolean>('overlay_set_stealth', { enabled }),
  overlayFocus: () => invoke<void>('overlay_focus'),
  registerHotkeys: (hotkeys: Record<string, string>) => invoke<void>('register_hotkeys_cmd', { hotkeys }),
};

export const events = {
  onTranscriptPartial: (cb: (line: TranscriptLine) => void) => listen<TranscriptLine>('transcript.partial', (e) => cb(e.payload)),
  onTranscriptFinal: (cb: (line: TranscriptLine) => void) => listen<TranscriptLine>('transcript.final', (e) => cb(e.payload)),
  onTranscriptStatus: (cb: (s: { status: string }) => void) =>
    listen<{ status: string }>('transcript.status', (e) => cb(e.payload)),
  onCaptureTaken: (cb: (img: CapturedImage) => void) => listen<CapturedImage>('capture.taken', (e) => cb(e.payload)),
  onAssistChunk: (cb: (c: { text: string }) => void) => listen<{ text: string }>('assist.chunk', (e) => cb(e.payload)),
  onAssistDone: (cb: (d: { cancelled?: boolean; error?: string }) => void) =>
    listen<{ cancelled?: boolean; error?: string }>('assist.done', (e) => cb(e.payload)),
  onError: (cb: (e: { message: string }) => void) => listen<{ message: string }>('error', (e) => cb(e.payload)),
  onStealthChanged: (cb: (payload: { stealth: boolean }) => void) =>
    listen<{ stealth: boolean }>('stealth.changed', (e) => cb(e.payload)),
  onHotkeyAssist: (cb: () => void) => listen('hotkey.assist', () => cb()),
  onHotkeyRegion: (cb: () => void) => listen('hotkey.capture_region', () => cb()),
  onHotkeyPreset: (cb: (p: { preset: string }) => void) => listen<{ preset: string }>('hotkey.preset', (e) => cb(e.payload)),
};

export type { UnlistenFn };
