import * as React from 'react';
import { Icon } from './Icon';
import { DEFAULT_HOTKEYS } from '../types';

export function shortcutLabel(binding: string) {
  if (!/Mac|iPhone|iPad/.test(navigator.platform)) {
    return binding.replace(/CommandOrControl|Control/g, 'Ctrl').replace(/Command|Super/g, 'Win').replace(/Option/g, 'Alt');
  }
  return binding.replace(/CommandOrControl|Command|Super/g, '⌘').replace(/Control/g, '⌃').replace(/Alt|Option/g, '⌥').replace(/Shift/g, '⇧').replace(/\+/g, '');
}

export function SharingControls({ enabled, hotkeys }: { enabled: boolean; hotkeys: Record<string,string> }) {
  React.useEffect(() => {
    if (!enabled) return;
    const root = document.querySelector('.companion-window');
    if (!root) return;
    // Native WebKit title popups may be separate windows. Keep accessible labels,
    // but suppress these hover popups while content protection is requested.
    const titles = new Map<Element,string>();
    const suppress = (element: Element) => {
      const title = element.getAttribute('title');
      if (title) { titles.set(element,title); element.removeAttribute('title'); }
    };
    root.querySelectorAll('[title]').forEach(suppress);
    const observer = new MutationObserver(records => {
      for (const record of records) {
        if (record.type === 'attributes') suppress(record.target as Element);
        for (const node of record.addedNodes) if (node instanceof Element) { suppress(node); node.querySelectorAll('[title]').forEach(suppress); }
      }
    });
    observer.observe(root, { childList:true, subtree:true, attributes:true, attributeFilter:['title'] });
    return () => { observer.disconnect(); for (const [element,title] of titles) if (element.isConnected && !element.hasAttribute('title')) element.setAttribute('title',title); };
  }, [enabled]);
  if (!enabled) return null;
  const key = (id: string) => shortcutLabel(hotkeys[id] || DEFAULT_HOTKEYS[id]);
  return <details className="sharing-controls" data-nodrag>
    <summary><Icon name="shield" size={13} /><span>Sharing controls</span><span className="sharing-focus-hint">Focus chat <kbd>{key('focusChat')}</kbd></span><Icon name="hide" size={12} /></summary>
    <div className="sharing-shortcuts"><div><span>Focus chat & type</span><kbd>{key('focusChat')}</kbd></div><div><span>Send question</span><kbd>{key('assist')}</kbd></div><div><span>Analyze screen</span><kbd>{key('screen')}</kbd></div><div><span>Hide / show Notely</span><kbd>{key('panicHide')}</kbd></div><p>Use shortcuts to keep your pointer still. Pointer movement can remain visible in Meet and other sharing apps. Window protection does not hide the shared cursor.</p></div>
  </details>;
}
