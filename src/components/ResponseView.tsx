import * as React from 'react';
import ReactMarkdown from 'react-markdown';
import rehypeHighlight from 'rehype-highlight';
import remarkGfm from 'remark-gfm';

interface ResponseViewProps {
  text: string;
  streaming: boolean;
  onClear?: () => void;
}

function extractText(node: React.ReactNode): string {
  if (typeof node === 'string') return node;
  if (typeof node === 'number') return String(node);
  if (!node) return '';
  if (Array.isArray(node)) return node.map(extractText).join('');
  if (React.isValidElement(node) && node.props && (node.props as any).children) {
    return extractText((node.props as any).children);
  }
  return '';
}

function CodeBlock({ className, children }: { className?: string; children?: React.ReactNode }) {
  const [copied, setCopied] = React.useState(false);
  const match = /language-(\w+)/.exec(className || '');
  const lang = match ? match[1] : '';
  const codeContent = extractText(children).replace(/\n$/, '');

  const handleCopy = (e: React.MouseEvent) => {
    e.stopPropagation();
    navigator.clipboard.writeText(codeContent).then(() => {
      setCopied(true);
      setTimeout(() => setCopied(false), 2000);
    });
  };

  return (
    <div className="relative group my-2.5 rounded-xl border border-white/15 bg-black/65 overflow-hidden shadow-lg" data-nodrag>
      <div className="flex items-center justify-between px-3.5 py-1.5 bg-white/5 border-b border-white/10 text-[11px] font-mono text-white/50 select-none">
        <span className="uppercase tracking-wider font-semibold text-white/70">{lang || 'code'}</span>
        <button
          onClick={handleCopy}
          className="flex items-center gap-1 px-2 py-0.5 rounded hover:bg-white/10 text-white/70 hover:text-white transition text-[11px]"
          title="Copy code"
        >
          {copied ? (
            <>
              <span className="text-emerald-400">✓</span>
              <span className="text-emerald-300">Copied</span>
            </>
          ) : (
            <>
              <span>📋</span>
              <span>Copy</span>
            </>
          )}
        </button>
      </div>
      <pre className="p-3.5 overflow-x-auto text-[13px] leading-relaxed font-mono select-text">
        <code className={className}>{children}</code>
      </pre>
    </div>
  );
}

export function ResponseView({ text, streaming, onClear }: ResponseViewProps) {
  const [copiedAll, setCopiedAll] = React.useState(false);

  const handleCopyAll = (e: React.MouseEvent) => {
    e.stopPropagation();
    navigator.clipboard.writeText(text).then(() => {
      setCopiedAll(true);
      setTimeout(() => setCopiedAll(false), 2000);
    });
  };

  if (!text) return null;

  return (
    <div className="relative flex flex-col w-full">
      {/* Top action toolbar */}
      <div className="flex items-center justify-between pb-1.5 mb-1.5 border-b border-white/10 text-[11.5px] text-white/60 select-none">
        <span className="flex items-center gap-1.5 font-medium text-white/75">
          <span className="w-1.5 h-1.5 rounded-full bg-blue-400" />
          <span>Response</span>
          {streaming && (
            <span className="text-[10.5px] px-1.5 py-0.2 rounded-full bg-blue-500/20 text-blue-300 border border-blue-400/30 animate-pulse">
              Streaming…
            </span>
          )}
        </span>
        <div className="flex items-center gap-1.5">
          <button
            onClick={handleCopyAll}
            className="flex items-center gap-1 px-2 py-0.5 rounded-md hover:bg-white/10 text-white/70 hover:text-white transition"
            title="Copy full response"
          >
            {copiedAll ? (
              <>
                <span className="text-emerald-400">✓</span>
                <span className="text-emerald-300">Copied!</span>
              </>
            ) : (
              <>
                <span>📋</span>
                <span>Copy</span>
              </>
            )}
          </button>
          {onClear && !streaming && (
            <button
              onClick={onClear}
              className="px-2 py-0.5 rounded-md hover:bg-white/10 text-white/40 hover:text-white/80 transition"
              title="Clear answer"
            >
              Clear
            </button>
          )}
        </div>
      </div>

      {/* Formatted Markdown Content */}
      <div className="response-body text-[14px] leading-relaxed text-white/90 select-text overflow-x-hidden">
        <ReactMarkdown
          remarkPlugins={[remarkGfm]}
          rehypePlugins={[rehypeHighlight]}
          components={{
            pre(props: any) {
              const codeEl = React.isValidElement(props.children) ? props.children : null;
              const className = codeEl ? (codeEl.props as any).className : '';
              const content = codeEl ? (codeEl.props as any).children : props.children;
              return <CodeBlock className={className}>{content}</CodeBlock>;
            },
            code({ className, children, ...props }: any) {
              return (
                <code
                  className="px-1.5 py-0.5 mx-0.5 rounded bg-white/12 text-sky-200 font-mono text-[12.5px] border border-white/10 font-medium select-text"
                  {...props}
                >
                  {children}
                </code>
              );
            },
            ul({ children }) {
              return <ul className="list-disc pl-5 my-2 space-y-1 text-white/90 leading-relaxed">{children}</ul>;
            },
            ol({ children }) {
              return <ol className="list-decimal pl-5 my-2 space-y-1 text-white/90 leading-relaxed">{children}</ol>;
            },
            li({ children }) {
              return <li className="leading-relaxed marker:text-blue-400/80 pl-0.5">{children}</li>;
            },
            p({ children }) {
              return <p className="my-1.5 leading-relaxed text-white/90">{children}</p>;
            },
            h1({ children }) {
              return <h1 className="text-[16.5px] font-bold text-white mt-2.5 mb-1 pb-1 border-b border-white/10">{children}</h1>;
            },
            h2({ children }) {
              return <h2 className="text-[15px] font-semibold text-white mt-2 mb-1">{children}</h2>;
            },
            h3({ children }) {
              return <h3 className="text-[13.5px] font-semibold text-sky-200 mt-1.5 mb-0.5">{children}</h3>;
            },
            strong({ children }) {
              return <strong className="font-semibold text-white">{children}</strong>;
            },
            em({ children }) {
              return <em className="italic text-white/85">{children}</em>;
            },
            blockquote({ children }) {
              return (
                <blockquote className="border-l-2 border-blue-400/80 pl-3 my-2 text-white/80 italic bg-white/5 py-1 rounded-r-lg">
                  {children}
                </blockquote>
              );
            },
            table({ children }) {
              return (
                <div className="overflow-x-auto my-2 rounded-lg border border-white/15">
                  <table className="min-w-full text-[12.5px] text-left divide-y divide-white/10">{children}</table>
                </div>
              );
            },
            th({ children }) {
              return <th className="px-3 py-1.5 bg-white/10 font-medium text-white/90">{children}</th>;
            },
            td({ children }) {
              return <td className="px-3 py-1.5 border-t border-white/10 text-white/85">{children}</td>;
            },
            a({ href, children }) {
              return (
                <a href={href} target="_blank" rel="noreferrer" className="text-sky-300 hover:text-sky-200 underline underline-offset-2">
                  {children}
                </a>
              );
            },
            hr() {
              return <hr className="my-2.5 border-white/10" />;
            }
          }}
        >
          {text}
        </ReactMarkdown>
        {streaming && (
          <span className="inline-block w-2 h-4 ml-1 align-middle bg-blue-400/90 animate-pulse rounded-sm" />
        )}
      </div>
    </div>
  );
}
