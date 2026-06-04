import { BookOpen, Bot, Check, Copy, Download, Search, FileCode } from "lucide-react";
import { useQuery } from "@tanstack/react-query";
import { useMemo, useState } from "react";

import { getAgentSkill, listAgentSkills } from "../api/agentSkills";
import { EmptyState } from "../components/EmptyState";
import { InlineError } from "../components/InlineError";
import { Badge } from "../components/ui/badge";
import { Button } from "../components/ui/button";
import { Input } from "../components/ui/input";
import { Skeleton } from "../components/ui/skeleton";
import { cn } from "../lib/utils";

export function AgentSkillsView() {
  const [searchQuery, setSearchQuery] = useState("");
  const [selectedAssistant, setSelectedAssistant] = useState<"all" | "gemini" | "claude">("all");
  const [selectedSkill, setSelectedSkill] = useState<{ assistant: "gemini" | "claude"; name: string } | null>(null);
  const [copied, setCopied] = useState(false);

  // Fetch the list of skills
  const skillsQuery = useQuery({
    queryKey: ["agent-skills"],
    queryFn: listAgentSkills,
  });

  // Fetch content for the selected skill
  const skillContentQuery = useQuery({
    queryKey: ["agent-skills", selectedSkill?.assistant, selectedSkill?.name],
    queryFn: () => getAgentSkill(selectedSkill!.assistant, selectedSkill!.name),
    enabled: selectedSkill !== null,
  });

  // Filter skills based on search query and assistant tab
  const filteredSkills = useMemo(() => {
    const list = skillsQuery.data ?? [];
    return list.filter((skill) => {
      const matchAssistant = selectedAssistant === "all" || skill.assistant === selectedAssistant;
      const matchSearch =
        skill.title.toLowerCase().includes(searchQuery.toLowerCase()) ||
        skill.description.toLowerCase().includes(searchQuery.toLowerCase()) ||
        skill.name.toLowerCase().includes(searchQuery.toLowerCase());
      return matchAssistant && matchSearch;
    });
  }, [skillsQuery.data, selectedAssistant, searchQuery]);

  // Lookup selected skill metadata (for title/description)
  const selectedSkillMeta = useMemo(() => {
    if (!selectedSkill || !skillsQuery.data) return null;
    return skillsQuery.data.find(
      (s) => s.name === selectedSkill.name && s.assistant === selectedSkill.assistant
    );
  }, [selectedSkill, skillsQuery.data]);

  // Copy skill content to clipboard
  const handleCopy = async () => {
    if (!skillContentQuery.data?.content) return;
    try {
      await navigator.clipboard.writeText(skillContentQuery.data.content);
      setCopied(true);
      setTimeout(() => setCopied(false), 2000);
    } catch (err) {
      console.error("Failed to copy content", err);
    }
  };

  // Download skill file
  const handleDownload = () => {
    if (!skillContentQuery.data) return;
    const { filename, content } = skillContentQuery.data;
    const blob = new Blob([content], { type: "text/markdown;charset=utf-8;" });
    const url = URL.createObjectURL(blob);
    const link = document.createElement("a");
    link.href = url;
    link.setAttribute("download", filename);
    document.body.appendChild(link);
    link.click();
    document.body.removeChild(link);
    URL.revokeObjectURL(url);
  };

  return (
    <div className="mx-auto grid max-w-7xl gap-5">
      <header className="flex flex-col gap-4 sm:flex-row sm:items-center sm:justify-between">
        <div>
          <p className="text-sm font-medium text-accent-strong">Agent skills library</p>
          <h1 className="mt-1 text-2xl font-semibold tracking-normal text-ink font-sans">Agent Skills</h1>
        </div>
      </header>

      {skillsQuery.isError && <InlineError message="Failed to load agent skills library." />}

      <div className="grid gap-6 lg:grid-cols-[340px_1fr] items-start">
        {/* Sidebar: List of skills */}
        <aside className="rounded-lg border border-line bg-white p-4 shadow-sm flex flex-col gap-4">
          <div className="relative">
            <Search className="absolute left-3 top-1/2 h-4 w-4 -translate-y-1/2 text-ink/40" aria-hidden="true" />
            <Input
              type="text"
              placeholder="Search skills..."
              value={searchQuery}
              onChange={(e) => setSearchQuery(e.target.value)}
              className="pl-9 bg-soft/30 border-line focus:border-accent text-sm"
            />
          </div>

          {/* Assistant Selector Tabs */}
          <div className="flex rounded-md bg-soft p-1 text-sm">
            <button
              onClick={() => setSelectedAssistant("all")}
              className={cn(
                "flex-1 rounded py-1.5 text-center font-medium transition-colors",
                selectedAssistant === "all"
                  ? "bg-white text-ink shadow-sm"
                  : "text-ink/60 hover:text-ink"
              )}
            >
              All
            </button>
            <button
              onClick={() => setSelectedAssistant("gemini")}
              className={cn(
                "flex-1 rounded py-1.5 text-center font-medium transition-colors",
                selectedAssistant === "gemini"
                  ? "bg-white text-ink shadow-sm"
                  : "text-ink/60 hover:text-ink"
              )}
            >
              Gemini
            </button>
            <button
              onClick={() => setSelectedAssistant("claude")}
              className={cn(
                "flex-1 rounded py-1.5 text-center font-medium transition-colors",
                selectedAssistant === "claude"
                  ? "bg-white text-ink shadow-sm"
                  : "text-ink/60 hover:text-ink"
              )}
            >
              Claude
            </button>
          </div>

          {/* Skills List */}
          <div className="flex flex-col gap-2 max-h-[500px] overflow-y-auto thin-scrollbar pr-1">
            {skillsQuery.isLoading && (
              <div className="flex flex-col gap-3">
                <Skeleton className="h-16 w-full" />
                <Skeleton className="h-16 w-full" />
                <Skeleton className="h-16 w-full" />
              </div>
            )}

            {!skillsQuery.isLoading && filteredSkills.length === 0 && (
              <div className="text-center py-6 text-sm text-ink/50">
                No matching agent skills found.
              </div>
            )}

            {!skillsQuery.isLoading &&
              filteredSkills.map((skill) => {
                const isSelected =
                  selectedSkill?.name === skill.name &&
                  selectedSkill?.assistant === skill.assistant;
                return (
                  <button
                    key={`${skill.assistant}-${skill.name}`}
                    onClick={() =>
                      setSelectedSkill({ assistant: skill.assistant, name: skill.name })
                    }
                    className={cn(
                      "w-full text-left rounded-lg border p-3.5 transition-all duration-200 hover:border-accent/40 flex flex-col gap-1.5",
                      isSelected
                        ? "border-accent bg-accent/5 ring-1 ring-accent"
                        : "border-line bg-white hover:bg-soft/40"
                    )}
                  >
                    <div className="flex items-start justify-between gap-2">
                      <span className="font-semibold text-sm text-ink group-hover:text-accent-strong">
                        {skill.title}
                      </span>
                      <Badge
                        variant={skill.assistant === "gemini" ? "purple" : "rust"}
                        className="shrink-0 text-[10px] py-0 px-1.5 font-medium"
                      >
                        {skill.assistant === "gemini" ? "Gemini" : "Claude"}
                      </Badge>
                    </div>
                    <p className="text-xs text-ink/65 line-clamp-2 leading-relaxed">
                      {skill.description}
                    </p>
                  </button>
                );
              })}
          </div>
        </aside>

        {/* Main Panel: Preview Markdown */}
        <section className="rounded-lg border border-line bg-white shadow-sm min-h-[500px] flex flex-col overflow-hidden">
          {!selectedSkill ? (
            <div className="flex-1 flex items-center justify-center p-8">
              <EmptyState
                title="Select a skill to preview"
                message="Choose an agent skill from the library on the left to read setup details, download, or copy instructions for your AI agent."
              />
            </div>
          ) : (
            <div className="flex-1 flex flex-col">
              {/* Header Bar */}
              <div className="border-b border-line bg-soft/20 px-5 py-4 flex flex-wrap items-center justify-between gap-3">
                <div className="flex items-center gap-2">
                  <FileCode className="h-5 w-5 text-accent" />
                  <div>
                    <h2 className="font-semibold text-base text-ink leading-none">
                      {selectedSkillMeta?.title || selectedSkill.name}
                    </h2>
                    <span className="text-xs text-ink/50 font-mono mt-1 block">
                      {selectedSkill.assistant === "gemini" ? ".gemini" : ".claude"}/skills/
                      {selectedSkill.name}.md
                    </span>
                  </div>
                </div>

                <div className="flex items-center gap-2">
                  <Button
                    variant="secondary"
                    size="sm"
                    onClick={handleCopy}
                    disabled={skillContentQuery.isLoading || skillContentQuery.isError}
                  >
                    {copied ? (
                      <>
                        <Check className="h-3.5 w-3.5 text-green-600" />
                        Copied!
                      </>
                    ) : (
                      <>
                        <Copy className="h-3.5 w-3.5" />
                        Copy Code
                      </>
                    )}
                  </Button>
                  <Button
                    variant="default"
                    size="sm"
                    onClick={handleDownload}
                    disabled={skillContentQuery.isLoading || skillContentQuery.isError}
                  >
                    <Download className="h-3.5 w-3.5" />
                    Download File
                  </Button>
                </div>
              </div>

              {/* Usage notice */}
              <div className="bg-blue-50/50 border-b border-blue-100/50 px-5 py-3 flex gap-3 items-start text-xs text-blue-800">
                <Bot className="h-4 w-4 shrink-0 text-blue-500 mt-0.5" />
                <div>
                  <span className="font-semibold">How to use this skill:</span> Place this markdown file in your local{" "}
                  <code className="bg-blue-100/50 px-1 py-0.5 rounded font-mono text-[11px] text-blue-900 font-semibold">
                    .{selectedSkill.assistant}/skills/
                  </code>{" "}
                  directory inside your active coding workspace. Compatible AI agents will scan this directory to learn new capabilities.
                </div>
              </div>

              {/* Document Container */}
              <div className="flex-1 p-6 overflow-y-auto thin-scrollbar max-h-[600px]">
                {skillContentQuery.isLoading && (
                  <div className="space-y-4">
                    <Skeleton className="h-8 w-3/4" />
                    <Skeleton className="h-4 w-full" />
                    <Skeleton className="h-4 w-5/6" />
                    <Skeleton className="h-32 w-full" />
                  </div>
                )}

                {skillContentQuery.isError && (
                  <InlineError message="Failed to load the selected agent skill content." />
                )}

                {!skillContentQuery.isLoading && skillContentQuery.data && (
                  <div className="markdown-body select-text">
                    <MarkdownRenderer content={skillContentQuery.data.content} />
                  </div>
                )}
              </div>
            </div>
          )}
        </section>
      </div>
    </div>
  );
}

interface MarkdownRendererProps {
  content: string;
}

function MarkdownRenderer({ content }: MarkdownRendererProps) {
  const lines = content.split("\n");
  const elements: React.ReactNode[] = [];
  let inCodeBlock = false;
  let codeBlockLanguage = "";
  let codeBlockLines: string[] = [];

  for (let i = 0; i < lines.length; i++) {
    const line = lines[i];
    if (line === undefined) continue;

    if (line.trim().startsWith("```")) {
      if (inCodeBlock) {
        // End of code block
        const codeText = codeBlockLines.join("\n");
        elements.push(
          <div key={`code-${i}`} className="my-4 rounded-lg overflow-hidden border border-line bg-zinc-900">
            <div className="flex items-center justify-between bg-zinc-800/80 px-4 py-1.5 text-[11px] font-mono text-zinc-400 border-b border-zinc-700/50">
              <span className="uppercase tracking-wider">{codeBlockLanguage || "code"}</span>
              <button
                type="button"
                onClick={() => navigator.clipboard.writeText(codeText)}
                className="hover:text-white transition-colors flex items-center gap-1"
              >
                Copy
              </button>
            </div>
            <pre className="overflow-x-auto p-4 font-mono text-xs leading-relaxed text-zinc-100 thin-scrollbar">
              <code>{codeText}</code>
            </pre>
          </div>
        );
        codeBlockLines = [];
        inCodeBlock = false;
      } else {
        // Start of code block
        inCodeBlock = true;
        codeBlockLanguage = line.trim().substring(3).trim();
      }
      continue;
    }

    if (inCodeBlock) {
      codeBlockLines.push(line);
      continue;
    }

    // Parse headers
    if (line.startsWith("# ")) {
      elements.push(
        <h1 key={`h1-${i}`} className="mt-6 mb-4 text-2xl font-bold tracking-tight text-ink border-b border-line pb-2 first:mt-0 font-sans">
          {parseInlineMarkdown(line.substring(2))}
        </h1>
      );
    } else if (line.startsWith("## ")) {
      elements.push(
        <h2 key={`h2-${i}`} className="mt-6 mb-3 text-lg font-semibold tracking-tight text-ink border-b border-line/40 pb-1 font-sans">
          {parseInlineMarkdown(line.substring(3))}
        </h2>
      );
    } else if (line.startsWith("### ")) {
      elements.push(
        <h3 key={`h3-${i}`} className="mt-4 mb-2 text-sm font-semibold tracking-tight text-ink font-sans">
          {parseInlineMarkdown(line.substring(4))}
        </h3>
      );
    } else if (line.trim().startsWith("- ") || line.trim().startsWith("* ")) {
      const cleanLine = line.trim().substring(2);
      elements.push(
        <li key={`li-${i}`} className="ml-5 list-disc text-sm text-ink/80 my-1.5 leading-relaxed">
          {parseInlineMarkdown(cleanLine)}
        </li>
      );
    } else if (/^\d+\.\s/.test(line.trim())) {
      const match = line.trim().match(/^(\d+)\.\s(.*)/);
      if (match) {
        elements.push(
          <li key={`li-${i}`} className="ml-5 list-decimal text-sm text-ink/80 my-1.5 leading-relaxed">
            {parseInlineMarkdown(match[2] ?? "")}
          </li>
        );
      }
    } else if (line.trim() === "") {
      elements.push(<div key={`space-${i}`} className="h-2" />);
    } else {
      elements.push(
        <p key={`p-${i}`} className="text-sm text-ink/80 leading-relaxed my-2 font-sans">
          {parseInlineMarkdown(line)}
        </p>
      );
    }
  }

  return <div className="space-y-1">{elements}</div>;
}

function parseInlineMarkdown(text: string): React.ReactNode[] {
  const parts: React.ReactNode[] = [];
  let remaining = text;
  let keyIdx = 0;

  while (remaining.length > 0) {
    const boldIndex = remaining.indexOf("**");
    const codeIndex = remaining.indexOf("`");

    if (boldIndex === -1 && codeIndex === -1) {
      parts.push(<span key={keyIdx++}>{remaining}</span>);
      break;
    }

    if (boldIndex !== -1 && (codeIndex === -1 || boldIndex < codeIndex)) {
      if (boldIndex > 0) {
        parts.push(<span key={keyIdx++}>{remaining.substring(0, boldIndex)}</span>);
      }
      const closingBoldIndex = remaining.indexOf("**", boldIndex + 2);
      if (closingBoldIndex !== -1) {
        const boldText = remaining.substring(boldIndex + 2, closingBoldIndex);
        parts.push(<strong key={keyIdx++} className="font-semibold text-ink">{boldText}</strong>);
        remaining = remaining.substring(closingBoldIndex + 2);
      } else {
        parts.push(<span key={keyIdx++}>{remaining.substring(boldIndex)}</span>);
        break;
      }
    } else {
      if (codeIndex > 0) {
        parts.push(<span key={keyIdx++}>{remaining.substring(0, codeIndex)}</span>);
      }
      const closingCodeIndex = remaining.indexOf("`", codeIndex + 1);
      if (closingCodeIndex !== -1) {
        const codeText = remaining.substring(codeIndex + 1, closingCodeIndex);
        parts.push(<code key={keyIdx++} className="px-1.5 py-0.5 rounded bg-soft font-mono text-[13px] text-accent-strong border border-line/60 font-semibold">{codeText}</code>);
        remaining = remaining.substring(closingCodeIndex + 1);
      } else {
        parts.push(<span key={keyIdx++}>{remaining.substring(codeIndex)}</span>);
        break;
      }
    }
  }

  return parts;
}
