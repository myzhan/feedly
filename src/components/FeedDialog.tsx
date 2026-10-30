import { useEffect, useState } from 'react';
import Modal from './Modal';
import type { FeedNode } from '../lib/types';

export interface FeedDialogData {
  kind: 'feed' | 'folder';
  id?: number; // present when editing an existing node
  title: string;
  feedUrl?: string;
  parentId: number | null;
}

interface Props {
  open: boolean;
  folders: FeedNode[];
  initial: Partial<FeedDialogData> | null;
  busy?: boolean;
  error?: string | null;
  onClose: () => void;
  onSubmit: (data: FeedDialogData) => void;
}

export default function FeedDialog({
  open,
  folders,
  initial,
  busy,
  error,
  onClose,
  onSubmit,
}: Props) {
  const [kind, setKind] = useState<'feed' | 'folder'>('feed');
  const [title, setTitle] = useState('');
  const [url, setUrl] = useState('');
  const [parentId, setParentId] = useState<number | null>(null);

  useEffect(() => {
    if (!open) return;
    setKind(initial?.kind ?? 'feed');
    setTitle(initial?.title ?? '');
    setUrl(initial?.feedUrl ?? '');
    setParentId(initial?.parentId ?? null);
  }, [open, initial]);

  const editing = initial?.id != null;
  // A folder (new or edit) and an edited feed require a title; a brand-new
  // feed only needs a URL (the name is auto-fetched from the feed when blank).
  const needsTitle = kind === 'folder' || editing;
  const canSubmit = needsTitle
    ? title.trim().length > 0
    : url.trim().length > 0;

  const submit = () => {
    if (!canSubmit) return;
    onSubmit({
      kind,
      id: initial?.id,
      title: title.trim(),
      feedUrl: kind === 'feed' ? url.trim() : undefined,
      parentId,
    });
  };

  return (
    <Modal open={open} onClose={onClose} title={editing ? '编辑订阅' : '添加订阅'}>
      <div className="space-y-4">
        {!editing && (
          <div className="flex gap-2">
            {(['feed', 'folder'] as const).map((k) => (
              <button
                key={k}
                onClick={() => setKind(k)}
                className={`flex-1 rounded-lg border px-3 py-2 text-sm font-medium transition ${
                  kind === k
                    ? 'border-indigo-500 bg-indigo-50 text-indigo-700 dark:bg-indigo-500/15 dark:text-indigo-300'
                    : 'border-slate-200 text-slate-600 hover:bg-slate-50 dark:border-slate-700 dark:text-slate-300 dark:hover:bg-slate-700/50'
                }`}
              >
                {k === 'feed' ? 'RSS 订阅源' : '文件夹'}
              </button>
            ))}
          </div>
        )}

        {kind === 'feed' && !editing && (
          <label className="block">
            <span className="mb-1 block text-sm font-medium text-slate-600 dark:text-slate-300">
              订阅地址 (Feed URL)
            </span>
            <input
              autoFocus
              value={url}
              onChange={(e) => setUrl(e.target.value)}
              onKeyDown={(e) => e.key === 'Enter' && submit()}
              placeholder="https://example.com/feed.xml"
              className="w-full rounded-lg border border-slate-300 px-3 py-2 text-sm outline-none focus:border-indigo-500 dark:border-slate-600 dark:bg-slate-900/50"
            />
          </label>
        )}

        <label className="block">
          <span className="mb-1 block text-sm font-medium text-slate-600 dark:text-slate-300">
            名称
          </span>
          <input
            value={title}
            onChange={(e) => setTitle(e.target.value)}
            onKeyDown={(e) => e.key === 'Enter' && submit()}
            placeholder={kind === 'feed' ? '订阅名称（留空则自动获取）' : '文件夹名称'}
            className="w-full rounded-lg border border-slate-300 px-3 py-2 text-sm outline-none focus:border-indigo-500 dark:border-slate-600 dark:bg-slate-900/50"
          />
        </label>

        <label className="block">
          <span className="mb-1 block text-sm font-medium text-slate-600 dark:text-slate-300">
            所在文件夹
          </span>
          <select
            value={parentId ?? ''}
            onChange={(e) => setParentId(e.target.value ? Number(e.target.value) : null)}
            className="w-full rounded-lg border border-slate-300 px-3 py-2 text-sm outline-none focus:border-indigo-500 dark:border-slate-600 dark:bg-slate-900/50"
          >
            <option value="">根目录</option>
            {folders.map((f) => (
              <option key={f.id} value={f.id} disabled={f.id === initial?.id}>
                {f.title}
              </option>
            ))}
          </select>
        </label>

        {error && (
          <p className="rounded-lg bg-red-50 px-3 py-2 text-sm text-red-600 dark:bg-red-500/10 dark:text-red-400">
            {error}
          </p>
        )}

        <div className="flex justify-end gap-2 pt-1">
          <button
            onClick={onClose}
            className="rounded-lg px-4 py-2 text-sm font-medium text-slate-600 hover:bg-slate-100 dark:text-slate-300 dark:hover:bg-slate-700"
          >
            取消
          </button>
          <button
            onClick={submit}
            disabled={!canSubmit || busy}
            className="rounded-lg bg-indigo-600 px-4 py-2 text-sm font-medium text-white hover:bg-indigo-500 disabled:opacity-50"
          >
            {busy ? '处理中…' : '保存'}
          </button>
        </div>
      </div>
    </Modal>
  );
}
