import { useEffect, useRef } from 'react';
import type { ArticleSummary } from '../lib/types';
import { relativeTime } from '../lib/utils';
import { IconFulltext, IconStar } from './Icons';

// Deterministic accent colour per feed so the same source always looks the
// same, making cross-feed reposts of one article easy to tell apart.
const SOURCE_COLORS = [
  '#ef4444',
  '#f97316',
  '#eab308',
  '#22c55e',
  '#06b6d4',
  '#3b82f6',
  '#8b5cf6',
  '#ec4899',
];
function sourceColor(feedId: number): string {
  return SOURCE_COLORS[Math.abs(feedId) % SOURCE_COLORS.length];
}

interface Props {
  items: ArticleSummary[];
  total: number;
  loading: boolean;
  selectedId: number | null;
  hasMore: boolean;
  fetchingMore: boolean;
  showSource: boolean;
  onSelect: (a: ArticleSummary) => void;
  onToggleStar: (a: ArticleSummary) => void;
  onLoadMore: () => void;
}

function ListItem({
  a,
  active,
  showSource,
  onSelect,
  onToggleStar,
}: {
  a: ArticleSummary;
  active: boolean;
  showSource: boolean;
  onSelect: (a: ArticleSummary) => void;
  onToggleStar: (a: ArticleSummary) => void;
}) {
  return (
    <div
      id={`art-${a.id}`}
      onClick={() => onSelect(a)}
      className={`list-row cursor-pointer border-b border-slate-100 px-4 py-3 dark:border-slate-800 ${
        active
          ? 'bg-indigo-50 dark:bg-indigo-500/10'
          : 'hover:bg-slate-50 dark:hover:bg-slate-800/60'
      }`}
    >
      <div className="flex items-start gap-2">
        {!a.isRead && (
          <span className="mt-1.5 h-2 w-2 shrink-0 rounded-full bg-indigo-500" />
        )}
        <div className={`min-w-0 flex-1 ${a.isRead ? '' : 'font-medium'}`}>
          {showSource && (
            <div className="mb-1 flex items-center gap-1.5">
              <span
                className="h-2 w-2 shrink-0 rounded-full"
                style={{ backgroundColor: sourceColor(a.feedId) }}
              />
              <span className="truncate text-[11px] font-medium tracking-wide text-slate-500 dark:text-slate-400">
                {a.feedTitle}
              </span>
            </div>
          )}
          <p
            className={`truncate text-sm ${
              a.isRead
                ? 'text-slate-600 dark:text-slate-400'
                : 'text-slate-900 dark:text-slate-100'
            }`}
          >
            {a.title || '(无标题)'}
          </p>
          <p className="mt-0.5 flex items-center gap-1.5 text-xs text-slate-400">
            {!showSource && (
              <>
                <span className="truncate">{a.feedTitle}</span>
                <span>·</span>
              </>
            )}
            <span className="shrink-0">{relativeTime(a.publishedAt)}</span>
          </p>
          {a.plainText && (
            <p className="mt-1 line-clamp-2 text-xs text-slate-500 dark:text-slate-400">
              {a.plainText}
            </p>
          )}
        </div>
        <span
          title={a.fulltextReady ? '全文已就绪' : '全文未就绪（点开先显示摘要）'}
          className={`mt-1 shrink-0 ${
            a.fulltextReady
              ? 'text-emerald-500'
              : 'text-slate-300 dark:text-slate-600'
          }`}
        >
          <IconFulltext width={16} height={16} />
        </span>
        <button
          onClick={(e) => {
            e.stopPropagation();
            onToggleStar(a);
          }}
          className="shrink-0 rounded p-1 text-slate-300 hover:text-amber-500"
          aria-label="星标"
        >
          <IconStar
            width={16}
            height={16}
            className={a.isStarred ? 'fill-amber-400 text-amber-400' : ''}
          />
        </button>
      </div>
    </div>
  );
}

export default function ArticleList({
  items,
  total,
  loading,
  selectedId,
  hasMore,
  fetchingMore,
  showSource,
  onSelect,
  onToggleStar,
  onLoadMore,
}: Props) {
  const sentinel = useRef<HTMLDivElement>(null);

  useEffect(() => {
    const el = sentinel.current;
    if (!el) return;
    const io = new IntersectionObserver((entries) => {
      if (entries[0].isIntersecting && hasMore && !fetchingMore) onLoadMore();
    });
    io.observe(el);
    return () => io.disconnect();
  }, [hasMore, fetchingMore, onLoadMore]);

  if (loading) {
    return (
      <div className="flex-1 overflow-y-auto p-4 text-sm text-slate-400">加载中…</div>
    );
  }

  if (items.length === 0) {
    return (
      <div className="flex flex-1 items-center justify-center p-6 text-center text-sm text-slate-400">
        没有符合条件的文章
      </div>
    );
  }

  return (
    <div className="flex min-h-0 w-full flex-1 flex-col overflow-y-auto">
      {items.map((a) => (
        <ListItem
          key={a.id}
          a={a}
          active={selectedId === a.id}
          showSource={showSource}
          onSelect={onSelect}
          onToggleStar={onToggleStar}
        />
      ))}
      <div ref={sentinel} className="p-3 text-center text-xs text-slate-400">
        {fetchingMore ? '加载中…' : hasMore ? `还有 ${total - items.length} 篇` : '— fin —'}
      </div>
    </div>
  );
}
