import { type ComponentPropsWithoutRef, memo } from "react";
import ReactMarkdown from "react-markdown";
import { Prism as SyntaxHighlighter } from "react-syntax-highlighter";
import { oneDark } from "react-syntax-highlighter/dist/esm/styles/prism";
import remarkGfm from "remark-gfm";
import { cn } from "@/lib/utils";

interface MarkdownProps {
  content: string;
  className?: string;
}

function CodeBlock({
  inline,
  className,
  children,
  ...props
}: ComponentPropsWithoutRef<"code"> & { inline?: boolean }) {
  const match = /language-(\w+)/.exec(className || "");
  const language = match ? match[1] : "";
  const codeString = String(children).replace(/\n$/, "");

  if (!inline && (match || codeString.includes("\n"))) {
    return (
      <div className="relative group my-2">
        {language && (
          <div className="absolute right-2 top-2 text-[10px] text-[#565f89] uppercase font-mono">
            {language}
          </div>
        )}
        <SyntaxHighlighter
          // biome-ignore lint/suspicious/noExplicitAny: SyntaxHighlighter style prop typing is incompatible
          style={oneDark as any}
          language={language || "text"}
          PreTag="div"
          customStyle={{
            margin: 0,
            padding: "1rem",
            background: "#1a1b26",
            border: "1px solid #27293d",
            borderRadius: "0.375rem",
          }}
          {...props}
        >
          {codeString}
        </SyntaxHighlighter>
      </div>
    );
  }

  return (
    <code
      className={cn(
        "px-1.5 py-0.5 rounded bg-[#1a1b26] border border-[#27293d] text-[#7aa2f7] font-mono text-[0.9em]",
        className
      )}
      {...props}
    >
      {children}
    </code>
  );
}

export const Markdown = memo(function Markdown({ content, className }: MarkdownProps) {
  return (
    <div
      className={cn(
        "prose prose-invert prose-sm max-w-none break-words overflow-hidden",
        className
      )}
    >
      <ReactMarkdown
        remarkPlugins={[remarkGfm]}
        components={{
          code: CodeBlock,
          // Headings
          h1: ({ children }) => (
            <h1 className="text-xl font-bold text-[#c0caf5] mt-4 mb-2 first:mt-0">{children}</h1>
          ),
          h2: ({ children }) => (
            <h2 className="text-lg font-semibold text-[#c0caf5] mt-3 mb-2 first:mt-0">
              {children}
            </h2>
          ),
          h3: ({ children }) => (
            <h3 className="text-base font-semibold text-[#c0caf5] mt-3 mb-1 first:mt-0">
              {children}
            </h3>
          ),
          // Paragraphs
          p: ({ children }) => <p className="text-[#c0caf5] mb-2 last:mb-0">{children}</p>,
          // Lists
          ul: ({ children }) => (
            <ul className="list-disc list-inside text-[#c0caf5] mb-2 space-y-1">{children}</ul>
          ),
          ol: ({ children }) => (
            <ol className="list-decimal list-inside text-[#c0caf5] mb-2 space-y-1">{children}</ol>
          ),
          li: ({ children }) => <li className="text-[#c0caf5]">{children}</li>,
          // Links
          a: ({ href, children }) => (
            <a
              href={href}
              target="_blank"
              rel="noopener noreferrer"
              className="text-[#7aa2f7] hover:underline"
            >
              {children}
            </a>
          ),
          // Blockquotes
          blockquote: ({ children }) => (
            <blockquote className="border-l-2 border-[#bb9af7] pl-3 my-2 text-[#a9b1d6] italic">
              {children}
            </blockquote>
          ),
          // Horizontal rule
          hr: () => <hr className="my-4 border-[#27293d]" />,
          // Strong and emphasis
          strong: ({ children }) => (
            <strong className="font-bold text-[#c0caf5]">{children}</strong>
          ),
          em: ({ children }) => <em className="italic text-[#c0caf5]">{children}</em>,
          // Tables
          table: ({ children }) => (
            <div className="overflow-x-auto my-2">
              <table className="min-w-full border-collapse border border-[#27293d] text-sm">
                {children}
              </table>
            </div>
          ),
          thead: ({ children }) => <thead className="bg-[#1f2335]">{children}</thead>,
          tbody: ({ children }) => <tbody>{children}</tbody>,
          tr: ({ children }) => <tr className="border-b border-[#27293d]">{children}</tr>,
          th: ({ children }) => (
            <th className="px-3 py-2 text-left text-[#c0caf5] font-semibold border border-[#27293d]">
              {children}
            </th>
          ),
          td: ({ children }) => (
            <td className="px-3 py-2 text-[#a9b1d6] border border-[#27293d]">{children}</td>
          ),
        }}
      >
        {content}
      </ReactMarkdown>
    </div>
  );
});
