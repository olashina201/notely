// Shared frontend types mirroring the Rust backend (state.rs / db.rs / gemini.rs).
export type Speaker = 'me' | 'other';
export type Provider = 'gemini' | 'openai' | 'claude';
export const PROVIDERS: { id: Provider; name: string }[] = [
  { id: 'gemini', name: 'Gemini' },
  { id: 'openai', name: 'OpenAI' },
  { id: 'claude', name: 'Claude' },
];

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
  requestId: string;
  conversationId?: string;
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

export interface DiagnosticEntry {
  timestamp: string;
  level: string;
  stage: string;
  requestId: string | null;
  fields: Record<string, unknown>;
}

export interface DiagnosticsSnapshot {
  path: string;
  entries: DiagnosticEntry[];
  writeError: string | null;
}

export interface AppSettings {
  assistantProvider: Provider;
  openaiFastModel: string;
  openaiSmartModel: string;
  claudeFastModel: string;
  claudeSmartModel: string;
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
  profileInstructions: Record<string, string>;
  hotkeys: Record<string, string>;
}

export const PROFILES = [
  { id: 'general', name: 'Everyday', icon: 'notebook', description: 'A thoughtful partner for everyday work and decisions.', placeholder: 'Your role, goals, preferences, and how you like answers…' },
  { id: 'meeting', name: 'Meeting', icon: 'wave', description: 'Talking points, decisions, and clear next steps.', placeholder: 'Your team, project, meeting goals, and your role…' },
  { id: 'coding', name: 'Coding', icon: 'code', description: 'Practical solutions grounded in your stack.', placeholder: 'Languages, framework versions, architecture, coding conventions…' },
  { id: 'writing', name: 'Writing', icon: 'pen', description: 'Clear drafts in your voice, for your audience.', placeholder: 'Audience, tone, style, examples, and words to avoid…' },
  { id: 'interview', name: 'Interview', icon: 'chat', description: 'Clear, natural answers backed by your experience.', placeholder: 'Target role, your real experience, strengths, and interview context…' },
  { id: 'sales', name: 'Sales call', icon: 'briefcase', description: 'Better discovery, relevant value, and useful follow-ups.', placeholder: 'Your product, buyer, value proposition, and call objectives…' },
  { id: 'lecture', name: 'Learning', icon: 'notes', description: 'Understand ideas with explanations at your level.', placeholder: 'Subject, current level, learning goals, and preferred examples…' },
] as const;

export interface ChatSummary {
  id: string; title: string; profileId: string; provider: Provider; model: string;
  startedAt: number; updatedAt: number; turnCount: number;
}
export interface ChatTurn {
  requestId: string; question: string; answer: string; profileId: string;
  provider: Provider; model: string; createdAt: number;
}
export type HistoryEntry = { kind: 'meeting'; key: string; meeting: MeetingRow } | { kind: 'chat'; key: string; chat: ChatSummary };

export const PRESETS = [
  { id: 'say', label: 'What should I say?' },
  { id: 'followup', label: 'Follow-up questions' },
  { id: 'recap', label: 'Recap' },
  { id: 'screen', label: 'Analyze screen' },
] as const;

export const DEFAULT_HOTKEYS: Record<string, string> = {
  assist: 'CommandOrControl+Enter',
  focusChat: 'CommandOrControl+Shift+Space',
  toggleStealth: 'CommandOrControl+Shift+X',
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
  { id: 'focusChat', label: 'Focus chat without moving the pointer' },
  { id: 'assist', label: 'Assist (ask with transcript + screen)' },
  { id: 'toggleStealth', label: 'Toggle stealth mode' },
  { id: 'panicHide', label: 'Panic hide overlay' },
  { id: 'toggleClickThrough', label: 'Toggle click-through' },
  { id: 'captureRegion', label: 'Region screenshot (selector)' },
  { id: 'captureFull', label: 'Silent full screenshot' },
  { id: 'say', label: 'Preset: What should I say?' },
  { id: 'followup', label: 'Preset: Follow-up questions' },
  { id: 'recap', label: 'Preset: Recap' },
  { id: 'screen', label: 'Preset: Analyze screen' },
];
