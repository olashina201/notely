import * as React from 'react';
import { Icon } from './Icon';
import { ResponseView } from './views';
import type { ChatSummary, ChatTurn, HistoryEntry, MeetingRow, NotesJson, TranscriptLine } from '../types';
import { PROFILES, PROVIDERS } from '../types';

export type HistorySelection = { kind: 'chat'; summary: ChatSummary; turns: ChatTurn[] } | { kind: 'meeting'; row: MeetingRow; transcript: TranscriptLine[]; notes: NotesJson | null };
const profileName = (id: string) => PROFILES.find(p => p.id === id)?.name ?? id;
export function HistoryPanel({ entries, selected, query, searching, loading, onQuery, onSelect, onDelete, onRefresh, onContinue, onBack, busy, notesGenerating, onGenerateNotes }: {
  notesGenerating: boolean; onGenerateNotes: (id:number)=>void;
  entries: HistoryEntry[]; selected: HistorySelection | null; query: string; searching: boolean; loading: boolean; busy: boolean;
  onQuery: (q: string) => void; onSelect: (entry: HistoryEntry) => void; onDelete: (entry: HistoryEntry) => Promise<void>;
  onRefresh: () => void; onContinue: (selection: Extract<HistorySelection, {kind:'chat'}>) => void; onBack: () => void;
}) {
  const [filter, setFilter] = React.useState<'all' | 'chat' | 'meeting'>('all');
  const [pendingDelete, setPendingDelete] = React.useState<string | null>(null);
  const [deleting, setDeleting] = React.useState(false);
  const visible = entries.filter(e => filter === 'all' || e.kind === filter);
  if (selected) return <div className="history-detail">
    <div className="history-detail-toolbar"><button className="text-button" onClick={onBack}>← All history</button>{selected.kind === 'meeting' && <button className="primary-small" disabled={busy || notesGenerating || selected.transcript.length === 0} onClick={()=>onGenerateNotes(selected.row.id)}>{notesGenerating ? 'Generating notes…' : selected.notes ? 'Regenerate notes' : 'Generate notes'}</button>}{selected.kind === 'chat' && <button className="primary-small" disabled={busy} onClick={() => onContinue(selected)}>Continue this chat <Icon name="arrow" size={13} /></button>}</div>
    <div className="history-detail-scroll">
      <p className="eyebrow">{selected.kind === 'chat' ? 'SAVED CHAT' : 'SAVED MEETING'}</p>
      <h2>{selected.kind === 'chat' ? selected.summary.title : selected.row.title}</h2>
      <p className="history-meta">{new Date(selected.kind === 'chat' ? selected.summary.updatedAt : selected.row.startedAt).toLocaleString()} · {profileName(selected.kind === 'chat' ? selected.summary.profileId : selected.row.profileId)}</p>
      {selected.kind === 'chat' ? selected.turns.map(turn => <article className="saved-turn" key={turn.requestId}><p className="user-question">{turn.question}</p><div className="turn-attribution"><Icon name="notebook" size={13} />Notely · {PROVIDERS.find(p=>p.id === turn.provider)?.name ?? turn.provider} · {profileName(turn.profileId)}</div><ResponseView text={turn.answer} streaming={false} /></article>) : <>
        {selected.notes && <div className="saved-notes"><h3>Summary</h3><ResponseView text={selected.notes.summary} streaming={false} />{selected.notes.decisions.length > 0 && <><h3>Decisions</h3><ul>{selected.notes.decisions.map((text,i)=><li key={i}>{text}</li>)}</ul></>}{selected.notes.actionItems.length > 0 && <><h3>Action items</h3><ul>{selected.notes.actionItems.map((item,i)=><li key={i}>{item.text}{item.owner ? ` · ${item.owner}` : ''}</li>)}</ul></>}{selected.notes.followUpEmail && <><h3>Follow-up draft</h3><p className="selectable-text">{selected.notes.followUpEmail}</p></>}</div>}
        <h3>Transcript · {selected.transcript.length} lines</h3>{selected.transcript.map(line=><p className="saved-transcript" key={line.id}><strong>{line.speaker === 'me' ? 'You' : 'Others'}</strong> {line.text}</p>)}
      </>}
    </div>
  </div>;
  return <div className="history-browser">
    <div className="history-search"><input aria-label="Search history" placeholder="Search chats, answers, meeting notes…" value={query} onChange={e=>onQuery(e.target.value)} /><button className="icon-button" aria-label="Refresh history" title="Refresh history" onClick={onRefresh}><Icon name="history" size={16} /></button></div>
    <div className="history-filters">{(['all','chat','meeting'] as const).map(kind=><button key={kind} aria-pressed={filter === kind} className={filter === kind ? 'active' : ''} onClick={()=>{setFilter(kind);setPendingDelete(null);}}>{kind === 'all' ? 'Everything' : kind === 'chat' ? 'AI chats' : 'Meetings'}</button>)}<span>{searching ? 'Searching…' : `${visible.length} saved`}</span></div>
    <div className="history-list" aria-busy={searching || loading}>
      {!searching && visible.length === 0 && <div className="empty-state"><span className="empty-state-icon"><Icon name="history" size={24} /></span><h2>{query ? 'No matches just yet' : 'A place to pick things back up'}</h2><p>{query ? 'Try another phrase from a question, answer, or meeting.' : 'Completed AI chats and recorded meetings are saved on this Mac. Screenshots are not stored in chat history.'}</p></div>}
      {visible.map(entry=>{const item = entry.kind === 'chat' ? entry.chat : entry.meeting; return <div className="history-row" key={entry.key}>
        <button className="history-row-main" disabled={loading} onClick={()=>onSelect(entry)}><span className="recent-icon"><Icon name={entry.kind === 'chat' ? 'chat' : 'wave'} size={17} /></span><span><strong>{item.title || 'Untitled meeting'}</strong><small>{entry.kind === 'chat' ? `${entry.chat.turnCount} ${entry.chat.turnCount === 1 ? 'exchange' : 'exchanges'}` : entry.meeting.endedAt ? 'Meeting' : 'Unfinished meeting'} · {profileName(item.profileId)} · {new Date(entry.kind === 'chat' ? entry.chat.updatedAt : entry.meeting.startedAt).toLocaleDateString(undefined,{month:'short',day:'numeric',hour:'2-digit',minute:'2-digit'})}{entry.kind === 'meeting' && entry.meeting.notesJson ? ' · Notes ready' : ''}</small></span></button>
        {pendingDelete === entry.key ? <div className="delete-confirm"><span>Delete permanently?</span><button disabled={deleting} onClick={()=>{setDeleting(true);void onDelete(entry).then(()=>setPendingDelete(null)).catch(()=>{}).finally(()=>setDeleting(false));}}>Delete</button><button disabled={deleting} onClick={()=>setPendingDelete(null)}>Keep</button></div> : <button className="icon-button delete-history" disabled={busy || deleting} aria-label={`Delete ${item.title || 'meeting'}`} title="Delete from history" onClick={()=>setPendingDelete(entry.key)}><Icon name="close" size={14} /></button>}
      </div>;})}
    </div>
    <p className="history-storage-note">Saved locally on this Mac · Chat history includes questions and answers.</p>
  </div>;
}
