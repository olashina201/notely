import * as React from 'react';
import { commands } from '../api';
import { PROVIDERS, type AppSettings, type ModelInfo, type Provider } from '../types';

const fieldClass = 'flex-1 min-w-0 bg-white/10 border border-white/12 rounded-lg px-2 py-1.5 text-[12px] outline-none';
const buttonClass = 'px-2.5 py-1 rounded-lg bg-white/10 hover:bg-white/10 border border-white/12 text-[12px] disabled:opacity-50';

export function CredentialEditor({ provider, model, onModels, testConnection }: {
  provider: Provider; model: string; onModels?: (models: ModelInfo[]) => void; testConnection?: () => Promise<string>;
}) {
  const [hasKey, setHasKey] = React.useState(false);
  const [input, setInput] = React.useState('');
  const [busy, setBusy] = React.useState(false);
  const [message, setMessage] = React.useState('');
  const name = PROVIDERS.find((p) => p.id === provider)!.name;

  React.useEffect(() => {
    let active = true;
    commands.keyHas(provider).then((v) => { if (active) setHasKey(v); })
      .catch((e) => { if (active) setMessage(String(e)); });
    return () => { active = false; };
  }, [provider]);

  async function perform(action: 'save' | 'test' | 'remove' | 'models') {
    setBusy(true);
    setMessage('');
    try {
      if (action === 'save') {
        await commands.keySet(provider, input.trim());
        setInput('');
        setHasKey(true);
        setMessage(`${name} key saved to OS Keychain.`);
      } else if (action === 'remove') {
        await commands.keyClear(provider);
        setHasKey(false);
        onModels?.([]);
        setMessage(`${name} key removed.`);
      } else if (action === 'test') {
        setMessage('Testing connection…');
        setMessage(await (testConnection ? testConnection() : commands.keyTest(provider, model)));
      } else {
        const models = await commands.modelsList(provider);
        onModels?.(models);
        setMessage(`Loaded ${models.length} models. Choose a model that supports text and images.`);
      }
    } catch (e) {
      setMessage(String(e));
    } finally { setBusy(false); }
  }

  return (
    <div className="space-y-1.5">
      <div className="flex items-center gap-2 text-white/65">
        <span className={`w-2 h-2 rounded-full ${hasKey ? 'bg-emerald-400' : 'bg-amber-400'}`} />
        {name} API key {hasKey ? 'available' : 'not set'}
      </div>
      <div className="flex gap-1.5">
        <input aria-label={`${name} API key`} type="password" autoComplete="off" value={input}
          onChange={(e) => setInput(e.target.value)} placeholder={`Paste ${name} API key`}
          onKeyDown={(e) => { if (e.key === 'Enter' && input.trim() && !busy) { e.preventDefault(); void perform('save'); } }}
          className={fieldClass} />
        <button disabled={busy || !input.trim()} onClick={() => void perform('save')} className={buttonClass}>Save key</button>
      </div>
      <div className="flex flex-wrap gap-1.5">
        <button disabled={busy || !hasKey || !model.trim()} onClick={() => void perform('test')} className={buttonClass}>Test connection</button>
        {onModels && <button disabled={busy || !hasKey} onClick={() => void perform('models')} className={buttonClass}>Load models</button>}
        <button disabled={busy || !hasKey} onClick={() => void perform('remove')} className={buttonClass}>Remove key</button>
      </div>
      <div className="text-[11px] text-white/65">Your operating system may ask to unlock a saved key once. Notely reuses that key in memory for this session. A ChatGPT or Claude subscription alone does not supply API access.</div>
      {message && <div role="status" className="text-[11.5px] text-white/90 break-words">{message}</div>}
    </div>
  );
}

export function ProviderFields({ settings, onChange }: { settings: AppSettings; onChange: (s: AppSettings) => void }) {
  const [models, setModels] = React.useState<ModelInfo[]>([]);
  const provider = settings.assistantProvider;
  const fastKey = provider === 'gemini' ? 'fastModel' : provider === 'openai' ? 'openaiFastModel' : 'claudeFastModel';
  const smartKey = provider === 'gemini' ? 'smartModel' : provider === 'openai' ? 'openaiSmartModel' : 'claudeSmartModel';
  const available = models.filter((m) => provider !== 'gemini' || m.methods.includes('generateContent'));

  return (
    <div className="space-y-2">
      <CredentialEditor provider={provider} model={settings[fastKey]} onModels={setModels} />
      {([['Fast model', fastKey], ['Smart / notes model', smartKey]] as const).map(([label, key]) => (
        <label key={key} className="flex items-center gap-2">
          <span className="text-white/65 w-32 shrink-0">{label}</span>
          <input aria-label={label} value={settings[key]} list={`models-${provider}`} spellCheck={false}
            onChange={(e) => onChange({ ...settings, [key]: e.target.value })} className={`${fieldClass} font-mono`} />
        </label>
      ))}
      <datalist id={`models-${provider}`}>{available.map((m) => <option key={m.id} value={m.id}>{m.displayName}</option>)}</datalist>
      {provider === 'gemini' && <>
        <label className="flex items-center gap-2">
          <span className="text-white/65 w-32">Smart thinking</span>
          <select value={settings.thinkingLevel} onChange={(e) => onChange({ ...settings, thinkingLevel: e.target.value })}
            className={`${fieldClass} [&>option]:text-black`}>
            <option value="off">Off</option><option value="low">Low</option><option value="high">High</option>
          </select>
        </label>
        <label className="flex items-center gap-2 text-white/65">
          <input type="checkbox" checked={settings.searchGrounding} onChange={(e) => onChange({ ...settings, searchGrounding: e.target.checked })} />
          Google Search grounding (can increase response time)
        </label>
      </>}
    </div>
  );
}
