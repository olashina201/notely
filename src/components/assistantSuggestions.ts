import { PRESETS, type AssistantAction } from '../types';

export interface SuggestedAction {
  id: string; label: string; description: string; icon: string;
  preset?: string; draft?: string; disabledReason?: string;
}
export interface ActionContext {
  recording: boolean; hasTranscript: boolean; meetingContext: boolean; hasChat: boolean; profile: string;
}
const starterDrafts: Record<string, [string,string,string][]> = {
  general: [['Think it through','Work through a decision or problem','Help me think through this situation: '],['Plan my next steps','Make a practical plan for your goal','Help me make a practical plan for: '],['Draft a project update','Turn your own notes into a clear update','Turn these notes into a clear project update:\n']],
  meeting: [['Prepare an agenda','Give your next meeting a clear purpose','Help me prepare a meeting agenda. The goal and attendees are:\n'],['Prepare talking points','Decide what you want to contribute','Help me prepare talking points for this meeting:\n'],['Draft a follow-up','Start from the notes you provide','Draft a follow-up message from these notes:\n']],
  interview: [['Practise a question','Prepare an answer using your real experience','Help me practise this interview question for the following role:\n'],['Shape my experience','Find a clear way to tell your story','Help me describe this real experience for an interview:\n'],['Prepare for an interview','Build a plan around the role','Help me prepare for an interview. The role and context are:\n']],
  coding: [['Debug an issue','Work from your error and relevant code','Help me debug this issue. Here is the error and relevant code:\n'],['Review my code','Check correctness and useful improvements','Review this code for correctness and suggest improvements:\n'],['Explain a concept','Work through a small, practical example','Explain this programming concept with an example: ']],
  writing: [['Draft a message','Write for a specific audience and purpose','Draft a message for this audience and purpose:\n'],['Improve a draft','Make your own writing clearer','Improve this draft while preserving my meaning:\n'],['Find the right tone','Express your point in your own voice','Help me express this point with the following tone:\n']],
  sales: [['Prepare for a call','Plan around your buyer and objective','Help me prepare for a sales call. The product, buyer, and objective are:\n'],['Clarify the value','Make your value proposition concrete','Help me improve this value proposition:\n'],['Work through an objection','Respond to a real concern','Help me respond to this customer objection:\n']],
  lecture: [['Explain a topic','Build an explanation at your level','Explain this topic at my current level:\n'],['Work through an example','Learn by doing something concrete','Walk me through an example of: '],['Check my understanding','Test the explanation you already have','Check my understanding and correct any mistakes:\n']],
};
export function actionUnavailable(action: AssistantAction, context: ActionContext): string | undefined {
  if (action.scope === 'live' && !context.recording) return 'This action needs a live meeting.';
  if (['live','meeting','review'].includes(action.scope) && (!context.meetingContext || !context.hasTranscript)) return 'Waiting for captured meeting speech.';
  if (action.scope === 'review' && context.recording) return 'End the meeting before drafting its follow-up.';
  if (action.scope === 'chat' && !context.hasChat) return 'Ask a question in this chat first.';
}
export function suggestedActions(context: ActionContext): { title: string; hint: string; primary: SuggestedAction[]; secondary: SuggestedAction[] } {
  const preset = (id: string): SuggestedAction => {
    const action = PRESETS.find(p=>p.id === id)!;
    return { id, label:action.label, description:action.description, icon:action.icon, preset:id, disabledReason:actionUnavailable(action,context) };
  };
  if (context.recording) return {
    title:'For the live meeting', hint:context.hasTranscript ? 'Use what has been captured to catch up, respond, or revisit a key point.' : 'Meeting help becomes available as speech is captured.',
    primary:['catchup','say','actions','revisit'].map(preset), secondary:['followup','recap','screen'].map(preset),
  };
  if (context.meetingContext && context.hasTranscript) return {
    title:'Review this meeting',hint:'Turn the captured discussion into useful decisions and next steps.',
    primary:['recap','actions','revisit','meeting_followup'].map(preset),secondary:[preset('screen')],
  };
  if (context.hasChat) return { title:'For this chat',hint:'Build on the conversation you already have.',primary:['chat_summary','chat_next','chat_explain','screen'].map(preset),secondary:[] };
  const starters=(starterDrafts[context.profile] ?? starterDrafts.general).map(([label,description,draft],i)=>({id:`starter-${i}`,label,description,icon:context.profile === 'coding' ? 'code' : context.profile === 'writing' ? 'pen' : 'notes',draft}));
  return { title:'A place to start',hint:'Choose a starting point, add your context, then send.',primary:[...starters,preset('screen')],secondary:[] };
}
