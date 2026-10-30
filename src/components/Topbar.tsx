import { useEffect, useState } from 'react';
import type { ArticleStatus } from '../lib/types';
import { IconCheckAll, IconRefresh, IconSearch, IconSettings, IconX } from './Icons';

interface Props {
  title: string;
  status: ArticleStatus;
  onStatus: (s: ArticleStatus) => void;
  query: string;
  onQuery: (q: string) => void;
  busy: boolean;
  progress: { done: number; total: number } | null;
  onRefresh: () => void;
  onMarkAllRead: () => void;
  onImportOpml: () => void;
  onExportOpml: () => void;
  onOpenSettings: () => void;
}

const STATUSES: { key: ArticleStatus; label: string }[] = [
  { key: 'all', label: '全部' },
  { key: 'unread', label: '未读' },
  { key: 'starred', label: '星标' },
];

export default function Topbar({
  title,
  status,
  onStatus,
  query,
  onQuery,
  busy,
  progress,
  onRefresh,
  onMarkAllRead,
  onImportOpml,
  onExportOpml,
  onOpenSettings,
}: Props) {
  const [text, setText] = useState(query);
  useEffect(() => setText(query), [query]);
  useEffect(() => {
    if (text === query) return;
    const t = setTimeout(() => onQuery(text), 250);
    return () => clearTimeout(t);
  }, [text, query, onQuery]);

  return (
    <header className="flex h-14 shrink-0 items-center gap-3 border-b border-slate-200 px-4 dark:border-slate-700">
      <h1 className="max-w-[22%] truncate text-sm font-semibold text-slate-800 dark:text-slate-100">
        {title}
      </h1>

      <div className="flex rounded-lg border border-slate-200 p-0.5 dark:border-slate-700">
        {STATUSES.map((s) => (
          <button
            key={s.key}
            onClick={() => onStatus(s.key)}
            className={`rounded-md px-2.5 py-1 text-xs font-medium transition ${
              status === s.key
                ? 'bg-indigo-600 text-white'
                : 'text-slate-500 hover:text-slate-700 dark:hover:text-slate-300'
            }`}
          >
            {s.label}
          </button>
        ))}
      </div>

      <div className="relative ml-auto w-64 max-w-full">
        <IconSearch
          width={15}
          height={15}
          className="pointer-events-none absolute left-2.5 top-1/2 -translate-y-1/2 text-slate-400"
        />
        <input
          id="feedly-search"
          value={text}
          onChange={(e) => setText(e.target.value)}
          placeholder="搜索文章…"
          className="w-full rounded-lg border border-slate-200 bg-white py-1.5 pl-8 pr-7 text-sm outline-none focus:border-indigo-500 dark:border-slate-700 dark:bg-slate-800"
        />
        {text && (
          <button
            onClick={() => setText('')}
            className="absolute right-1.5 top-1/2 -translate-y-1/2 text-slate-400 hover:text-slate-600"
            aria-label="清除搜索"
          >
            <IconX width={14} height={14} />
          </button>
        )}
      </div>

      <div className="flex items-center gap-1">
        {progress && (
          <span className="text-xs tabular-nums text-slate-400">
            {progress.done}/{progress.total}
          </span>
        )}
        <button
          onClick={onRefresh}
          disabled={busy}
          title="刷新全部 (r)"
          className="rounded-lg p-2 text-slate-500 hover:bg-slate-100 disabled:opacity-40 dark:hover:bg-slate-700"
        >
          <IconRefresh width={17} height={17} className={busy ? 'animate-spin' : ''} />
        </button>
        <button
          onClick={onMarkAllRead}
          title="全部标记已读 (shift+a)"
          className="rounded-lg p-2 text-slate-500 hover:bg-slate-100 dark:hover:bg-slate-700"
        >
          <IconCheckAll width={17} height={17} />
        </button>
        <button
          onClick={onImportOpml}
          title="导入 OPML"
          className="hidden rounded-lg p-2 text-slate-500 hover:bg-slate-100 sm:block dark:hover:bg-slate-700"
        >
          <span className="text-xs font-semibold">导入</span>
        </button>
        <button
          onClick={onExportOpml}
          title="导出 OPML"
          className="hidden rounded-lg p-2 text-slate-500 hover:bg-slate-100 sm:block dark:hover:bg-slate-700"
        >
          <span className="text-xs font-semibold">导出</span>
        </button>
        <button
          onClick={onOpenSettings}
          title="设置 (,)"
          className="rounded-lg p-2 text-slate-500 hover:bg-slate-100 dark:hover:bg-slate-700"
        >
          <IconSettings width={17} height={17} />
        </button>
      </div>
    </header>
  );
}
