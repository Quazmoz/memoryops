import { ChevronDown, Loader2, MessageSquare, Minus, Send, ThumbsDown, ThumbsUp } from "lucide-react";
import { useMemo, useState, type FormEvent } from "react";

import type { FeedbackEntry, FeedbackRating } from "../api/types";
import { useMemoryFeedback, useSubmitFeedback } from "../hooks/use-memory";
import { formatRelativeTime } from "../lib/format";
import { cn } from "../lib/utils";
import { InlineError } from "./InlineError";
import { Badge } from "./ui/badge";
import { Button } from "./ui/button";
import { Card, CardContent, CardHeader, CardTitle } from "./ui/card";
import { Input } from "./ui/input";
import { Skeleton } from "./ui/skeleton";
import { HelpTooltip } from "./ui/tooltip";

type FeedbackPanelProps = {
  workspaceId: string;
  memoryId: string;
  initialQueryId?: string;
};

const RECENT_FEEDBACK_LIMIT = 5;
const MAX_COMMENT_CHARS = 500;
const recentFeedbackParams = { limit: RECENT_FEEDBACK_LIMIT, offset: 0 };

const ratingOptions: Array<{ value: FeedbackRating; label: string; icon: typeof ThumbsUp }> = [
  { value: 1, label: "Positive", icon: ThumbsUp },
  { value: 0, label: "Neutral", icon: Minus },
  { value: -1, label: "Negative", icon: ThumbsDown },
];

export function FeedbackPanel({ workspaceId, memoryId, initialQueryId = "" }: FeedbackPanelProps) {
  const feedbackQuery = useMemoryFeedback(workspaceId, memoryId, recentFeedbackParams);
  const submitFeedback = useSubmitFeedback(workspaceId);
  const [rating, setRating] = useState<FeedbackRating>(1);
  const [queryId, setQueryId] = useState(initialQueryId);
  const [comment, setComment] = useState("");
  const [commentExpanded, setCommentExpanded] = useState(false);
  const [recentExpanded, setRecentExpanded] = useState(false);
  const trimmedComment = comment.trim();
  const avgRating = feedbackQuery.data?.avg_rating ?? 0;
  const recentItems = feedbackQuery.data?.items ?? [];
  const canSubmit = !submitFeedback.isPending;

  function submit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    if (!canSubmit) {
      return;
    }

    submitFeedback.mutate({
      memoryId,
      request: {
        query_id: queryId.trim(),
        rating,
        ...(trimmedComment.length > 0 ? { comment: trimmedComment } : {}),
      },
    });
  }

  const avgBadge = useMemo(() => averageBadge(avgRating), [avgRating]);

  return (
    <Card data-testid="feedback-panel">
      <CardHeader>
        <div className="flex flex-wrap items-center justify-between gap-3">
          <CardTitle className="flex items-center gap-1.5">
            <span>Feedback</span>
            <HelpTooltip label="Feedback">Operator ratings and notes that can help tune whether this memory should be trusted, boosted, or deprioritized.</HelpTooltip>
          </CardTitle>
          <Badge variant={avgBadge.variant} className="gap-1">
            <avgBadge.Icon className="h-3.5 w-3.5" aria-hidden="true" />
            {avgBadge.label} {avgRating.toFixed(2)}
          </Badge>
        </div>
      </CardHeader>
      <CardContent className="grid gap-4">
        <form className="grid gap-4" onSubmit={submit}>
          <div className="grid gap-2">
            <span className="text-sm font-medium text-ink">Rating</span>
            <div className="grid grid-cols-3 gap-2">
              {ratingOptions.map((option) => {
                const Icon = option.icon;
                const selected = rating === option.value;

                return (
                  <Button
                    key={option.value}
                    type="button"
                    variant={selected ? "default" : "secondary"}
                    data-testid={`feedback-rating-${option.value}`}
                    aria-label={`${option.label} feedback`}
                    onClick={() => setRating(option.value)}
                    disabled={submitFeedback.isPending}
                  >
                    <Icon className="h-4 w-4" aria-hidden="true" />
                    {option.label}
                  </Button>
                );
              })}
            </div>
          </div>

          <label className="grid gap-2 text-sm font-medium text-ink" htmlFor="feedback-query-id">
            Query ID (optional)
            <Input id="feedback-query-id" data-testid="feedback-query-id" value={queryId} onChange={(event) => setQueryId(event.target.value)} />
          </label>

          <div className="grid gap-2">
            <Button type="button" variant="secondary" onClick={() => setCommentExpanded((value) => !value)} disabled={submitFeedback.isPending}>
              <MessageSquare className="h-4 w-4" aria-hidden="true" />
              Comment
              <ChevronDown className={cn("h-4 w-4 transition", commentExpanded && "rotate-180")} aria-hidden="true" />
            </Button>
            {commentExpanded ? (
              <label className="grid gap-1 text-sm text-ink/70" htmlFor="feedback-comment">
                <span className="flex justify-between text-xs font-medium uppercase text-ink/45">
                  <span>Comment</span>
                  <span>{comment.length}/{MAX_COMMENT_CHARS}</span>
                </span>
                <textarea
                  id="feedback-comment"
                  data-testid="feedback-comment"
                  maxLength={MAX_COMMENT_CHARS}
                  value={comment}
                  onChange={(event) => setComment(event.target.value)}
                  className="min-h-24 resize-y rounded-md border border-line bg-white px-3 py-2 text-sm leading-6 outline-none transition focus:border-accent focus:ring-2 focus:ring-accent/20 disabled:cursor-not-allowed disabled:opacity-50"
                  disabled={submitFeedback.isPending}
                />
              </label>
            ) : null}
          </div>

          {submitFeedback.isError ? <InlineError title="Feedback failed" message={errorMessage(submitFeedback.error)} /> : null}
          {submitFeedback.isSuccess ? <Badge variant="green">Feedback saved</Badge> : null}

          <Button type="submit" data-testid="feedback-submit" disabled={!canSubmit}>
            {submitFeedback.isPending ? <Loader2 className="h-4 w-4 animate-spin" aria-hidden="true" /> : <Send className="h-4 w-4" aria-hidden="true" />}
            Submit
          </Button>
        </form>

        <div className="border-t border-line pt-4">
          <button
            type="button"
            className="flex w-full items-center justify-between gap-3 text-left text-sm font-semibold text-ink"
            onClick={() => setRecentExpanded((value) => !value)}
          >
            <span>Recent feedback</span>
            <ChevronDown className={cn("h-4 w-4 transition", recentExpanded && "rotate-180")} aria-hidden="true" />
          </button>
          {recentExpanded ? <RecentFeedbackList items={recentItems} loading={feedbackQuery.isLoading} error={feedbackQuery.error} /> : null}
        </div>
      </CardContent>
    </Card>
  );
}

function RecentFeedbackList({ items, loading, error }: { items: FeedbackEntry[]; loading: boolean; error: Error | null }) {
  if (loading) {
    return (
      <div className="mt-3 grid gap-2">
        {Array.from({ length: 3 }).map((_, index) => (
          <Skeleton key={index} className="h-12 w-full" />
        ))}
      </div>
    );
  }

  if (error) {
    return <InlineError title="Feedback unavailable" message={errorMessage(error)} />;
  }

  if (items.length === 0) {
    return <p className="mt-3 text-sm text-ink/55">No feedback yet</p>;
  }

  return (
    <div className="mt-3 grid gap-2">
      {items.map((item) => (
        <div key={item.id} className="grid gap-1 rounded-md border border-line bg-soft p-3">
          <div className="flex flex-wrap items-center justify-between gap-2">
            <Badge variant={ratingBadge(item.rating).variant} className="gap-1">
              <RatingIcon rating={item.rating} />
              {ratingBadge(item.rating).label}
            </Badge>
            <span className="text-xs text-ink/55">
              {item.agent_id ?? item.user_id ?? "anonymous"} - {formatRelativeTime(item.occurred_at)}
            </span>
          </div>
          {item.comment ? <p className="text-sm leading-6 text-ink/75">{item.comment}</p> : null}
        </div>
      ))}
    </div>
  );
}

function RatingIcon({ rating }: { rating: FeedbackRating }) {
  if (rating > 0) {
    return <ThumbsUp className="h-3.5 w-3.5" aria-hidden="true" />;
  }
  if (rating < 0) {
    return <ThumbsDown className="h-3.5 w-3.5" aria-hidden="true" />;
  }
  return <Minus className="h-3.5 w-3.5" aria-hidden="true" />;
}

function averageBadge(value: number): { label: string; variant: "green" | "rust" | "gray"; Icon: typeof ThumbsUp } {
  if (value > 0.05) {
    return { label: "Positive", variant: "green", Icon: ThumbsUp };
  }
  if (value < -0.05) {
    return { label: "Negative", variant: "rust", Icon: ThumbsDown };
  }
  return { label: "Neutral", variant: "gray", Icon: Minus };
}

function ratingBadge(rating: FeedbackRating): { label: string; variant: "green" | "rust" | "gray" } {
  if (rating > 0) {
    return { label: "+1", variant: "green" };
  }
  if (rating < 0) {
    return { label: "-1", variant: "rust" };
  }
  return { label: "0", variant: "gray" };
}

function errorMessage(error: unknown): string {
  return error instanceof Error ? error.message : "Feedback could not be loaded.";
}
