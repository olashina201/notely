import * as React from 'react';
import ReactMarkdown from 'react-markdown';
import rehypeHighlight from 'rehype-highlight';
import type { CapturedImage, TranscriptLine } from '../types';

export function Hotkey({ children }: { children: React.ReactNode }) {
  return (
    <kbd className="px-1.5 py-0.5 mx-0.5 rounded bg-white/10 border border-white/15 text-[11px] font-mono text-white/80">
      {children}
    </kbd>
  );
}

export function ResponseView({ text, streaming }: { text: string; streaming: boolean }) {
  if (!text) {
    return (
      <div className="text-[13px] leading-relaxed text-white/40 px-1 py-3 text-center">
        Press Ctrl/Cmd+Enter for an answer based on the conversation and your screen.
      </div>
    );
  }
  return (
    <div className="response-body text-[13px] leading-relaxed text-white/90 px-1 py-1">
      <ReactMarkdown rehypePlugins={[rehypeHighlight]}>{text}</ReactMarkdown>
      {streaming && <span className="inline-block w-2 h-4 ml-1 align-middle bg-white/70 animate-pulse" />}
    </div>
  );
}

export function TranscriptView({ lines, recording }: { lines: TranscriptLine[]; recording: boolean }) {
  const ref = React.useRef<HTMLDivElement>(null);
  React.useEffect(() => {
    ref.current?.scrollTo({ top: ref.current.scrollHeight });
  }, [lines]);
  return (
    <div ref={ref} className="flex-1 overflow-y-auto px-3 py-2 space-y-1.5 min-h-0">
      {lines.length === 0 && (
        <div className="text-[12px] text-white/35 text-center py-4">
          {recording ? 'Listening... speak or play meeting audio.' : 'Start the meeting to see the live transcript here.'}
        </div>
      )}
      {lines.map((l) => (
        <div key={l.id} className="text-[12.5px] leading-snug">
          <span className={l.speaker === 'me' ? 'text-sky-300 font-semibold' : 'text-emerald-300 font-semibold'}>
            {l.speaker === 'me' ? 'Me' : 'Others'}
          </span>
          <span className="text-white/25">{' \u00b7 '}</span>
          <span className={l.isFinal ? 'text-white/90' : 'text-white/50 italic'}>{l.text}</span>
        </div>
      ))}
    </div>
  );
}

export function CapturesBar({ captures, onClear }: { captures: CapturedImage[]; onClear: () => void }) {
  if (captures.length === 0) return null;
  return (
    <div className="flex items-center gap-2 px-3 py-1.5 border-t border-white/10">
      <span className="text-[11px] text-white/50">Screen context ({captures.length})</span>
      <div className="flex gap-1.5 flex-1 overflow-x-auto">
        {captures.map((c) => (
          <img
            key={c.takenAt}
            src={'data:' + c.mimeType + ';base64,' + c.dataBase64}
            className="h-10 rounded border border-white/15 object-cover"
            alt="screen context"
          />
        ))}
      </div>
      <button onClick={onClear} className="text-[11px] text-white/50 hover:text-white/90 shrink-0">
        Clear
      </button>
    </div>
  );
}
