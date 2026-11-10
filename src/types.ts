// Shared frontend types mirroring the Rust backend (state.rs / db.rs / gemini.rs).
export type Speaker = 'me' | 'other';

export interface TranscriptLine {
  id: string;
  speaker: string;
  text: string;
  isFinal: boolean;
  startedAt: number;
}

export interface CapturedImage {
  dataBase64: string;
  mimeType: string;
  width: number;
  height: number;
  takenAt: number;
}

export interface AssistRequest {
  question: string;
  preset?: string | null;
  includeScreenshot: boolean;
  profileId: string;
  customInstructions?: string | null;
  slot: 'fast' | 'smart';
}

export interface ModelInfo {
  id: string;
  displayName: string;
  methods: string[];
}

export interface MonitorInfo {
  name: string;
  x: number;
  y: number;
  width: number;
  height: number;
  scaleFactor: number;
  isPrimary: boolean;
}

export interface SelectorBegin {
  monitors: MonitorInfo[];
  saved: { x: number; y: number; w: number; h: number };
  monitor: { x: number; y: number; w: number; h: number };
}

export interface MeetingRow {
  id: number;
  title: string;
  profileId: string;
  startedAt: number;
  endedAt: number | null;
  transcriptJson: string;
  notesJson: string | null;
}

export interface NotesJson {
  summary: string;
  decisions: string[];
  actionItems: { text: string; owner?: string }[];
  followUpEmail: string;
}

export interface AppSettings {
  liveModel: string;
  fastModel: string;
  smartModel: string;
  language: string;
  overlayOpacity: number;
  contentProtection: boolean;
  clickThrough: boolean;
  saveCaptures: boolean;
  thinkingLevel: string;
  searchGrounding: boolean;
  autoAttachScreenshot: boolean;
  instructionProfile: string;
  customInstructions: string;
  hotkeys: Record<string, string>;
}

export const PROFILES = [
  { id: 'interview', name: 'Interview' },
  { id: 'sales', name: 'Sales call' },
  { id: 'lecture', name: 'Lecture' },
  { id: 'coding', name: 'Coding' },
] as const;

export const PRESETS = [
  { id: 'say', label: 'What should I say?' },
  { id: 'followup', label: 'Follow-up questions' },
  { id: 'recap', label: 'Recap' },
  { id: 'screen', label: 'Analyze screen' },
] as const;

export const DEFAULT_HOTKEYS: Record<string, string> = {
  assist: 'CommandOrControl+Enter',
  captureFull: 'CommandOrControl+Shift+S',
  captureRegion: 'CommandOrControl+Shift+A',
  panicHide: 'CommandOrControl+Shift+H',
  toggleClickThrough: 'CommandOrControl+Shift+T',
  say: 'CommandOrControl+Shift+1',
  followup: 'CommandOrControl+Shift+2',
  recap: 'CommandOrControl+Shift+3',
  screen: 'CommandOrControl+Shift+4',
};

export const HOTKEY_ACTIONS = [
  { id: 'assist', label: 'Assist (ask with transcript + screen)' },
  { id: 'captureFull', label: 'Silent full screenshot' },
  { id: 'captureRegion', label: 'Region screenshot (selector)' },
  { id: 'panicHide', label: 'Panic hide overlay' },
  { id: 'toggleClickThrough', label: 'Toggle click-through' },
  { id: 'say', label: 'Preset: What should I say?' },
  { id: 'followup', label: 'Preset: Follow-up questions' },
  { id: 'recap', label: 'Preset: Recap' },
  { id: 'screen', label: 'Preset: Analyze screen' },
];
