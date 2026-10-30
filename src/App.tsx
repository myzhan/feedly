import { lazy, Suspense, useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { useQueryClient } from '@tanstack/react-query';
import Sidebar from './components/Sidebar';
import Topbar from './components/Topbar';
import ArticleList from './components/ArticleList';
import ArticleView from './components/ArticleView';
import Modal from './components/Modal';
import type { FeedDialogData } from './components/FeedDialog';
// On-demand dialogs: code-split out of the startup chunk so the first paint
// only parses the three reading panes.
const FeedDialog = lazy(() => import('./components/FeedDialog'));
const SettingsDialog = lazy(() => import('./components/SettingsDialog'));
const ShortcutsHelp = lazy(() => import('./components/ShortcutsHelp'));
const OpmlDialog = lazy(() => import('./components/OpmlDialog'));
import { api } from './lib/api';
import { subscribeFulltext, subscribeRefresh } from './lib/events';
import type { ArticleStatus, ArticleSummary, FeedNode } from './lib/types';
import { collectFeeds, flattenFeeds, openExternal } from './lib/utils';
import {
  keys,
  useArticle,
  useArticles,
  useFeedMutation,
  useFeeds,
  useInvalidate,
  useMarkAllRead,
  useMarkRead,
  useMarkUnread,
  useRefresh,
  useSetStarred,
  useSettings,
  useUpdateSettings,
} from './lib/queries';

interface Selection {
  kind: 'all' | 'feed' | 'folder';
  id: number | null;
  title: string;
}

export default function App() {
  const qc = useQueryClient();
  const { data: tree = [] } = useFeeds();
  const { data: settings } = useSettings();

  const [selection, setSelection] = useState<Selection>({ kind: 'all', id: null, title: '全部文章' });
  const [status, setStatus] = useState<ArticleStatus>('all');
  const [query, setQuery] = useState('');
  const [articleId, setArticleId] = useState<number | null>(null);

  const feedId = selection.kind === 'feed' ? selection.id : null;
  const folderId = selection.kind === 'folder' ? selection.id : null;

  const articlesQ = useArticles({ feedId, folderId, status, query });
  const articleQ = useArticle(articleId);
  const items = articlesQ.data?.items ?? [];
  const total = articlesQ.data?.total ?? 0;

  const invalidate = useInvalidate();
  const markRead = useMarkRead();
  const markUnread = useMarkUnread();
  const setStarred = useSetStarred();
  const markAllRead = useMarkAllRead();
  const feedMut = useFeedMutation();
  const refresh = useRefresh();
  const updateSettings = useUpdateSettings();

  // ---- refresh progress driven by backend events -------------------------
  const [progress, setProgress] = useState<{ running: boolean; done: number; total: number }>(
    { running: false, done: 0, total: 0 },
  );
  const invalidateRef = useRef(invalidate);
  invalidateRef.current = invalidate;

  // Ids already handed to the background full-text prefetch, so the list effect
  // fires once per article rather than on every render.
  const prefetched = useRef<Set<number>>(new Set());

  // When a background full-text extraction lands, refetch the affected article
  // (so the reader swaps in the richer body in place) and the article list (so
  // its `fulltextReady` flag flips and we stop re-prefetching / opening it).
  useEffect(() => {
    let unlisten: (() => void) | undefined;
    let cancelled = false;
    subscribeFulltext({
      onReady: ({ articleId }) => {
        qc.invalidateQueries({ queryKey: keys.article(articleId) });
        qc.invalidateQueries({ queryKey: ['articles'] });
      },
    }).then((fn) => {
      if (cancelled) fn();
      else unlisten = fn;
    });
    return () => {
      cancelled = true;
      unlisten?.();
    };
  }, [qc]);

  useEffect(() => {
    let unlisten: (() => void) | undefined;
    let cancelled = false;
    subscribeRefresh({
      onStarted: ({ total }) => setProgress({ running: true, done: 0, total }),
      onProgress: () => setProgress((p) => ({ ...p, done: p.done + 1 })),
      onDone: () => {
        setProgress({ running: false, done: 0, total: 0 });
        invalidateRef.current();
      },
      onArticleAdded: () => invalidateRef.current({ feeds: true, articles: true }),
    }).then((fn) => {
      if (cancelled) fn();
      else unlisten = fn;
    });
    return () => {
      cancelled = true;
      unlisten?.();
    };
  }, []);

  // ---- theme --------------------------------------------------------------
  const theme = settings?.theme ?? 'system';
  useEffect(() => {
    const mq = window.matchMedia('(prefers-color-scheme: dark)');
    const apply = () => {
      const dark = theme === 'dark' || (theme === 'system' && mq.matches);
      document.documentElement.classList.toggle('dark', dark);
    };
    try {
      localStorage.setItem('feedly-theme', theme);
    } catch {
      /* ignore */
    }
    apply();
    if (theme === 'system') {
      mq.addEventListener('change', apply);
      return () => mq.removeEventListener('change', apply);
    }
  }, [theme]);

  // ---- derived lists ------------------------------------------------------
  const allNodes = useMemo(() => flattenFeeds(tree), [tree]);
  const folders = useMemo(() => allNodes.filter((n) => n.kind === 'folder'), [allNodes]);
  const feedList = useMemo(() => collectFeeds(tree), [tree]);
  const totalUnread = useMemo(
    () => tree.reduce((s, n) => s + n.unreadCount, 0),
    [tree],
  );
  const findNode = useCallback((id: number) => allNodes.find((n) => n.id === id), [allNodes]);

  // ---- actions ------------------------------------------------------------
  const selectNode = useCallback((node: FeedNode) => {
    setSelection({ kind: node.kind, id: node.id, title: node.title });
    setArticleId(null);
  }, []);
  const selectAll = useCallback(() => {
    setSelection({ kind: 'all', id: null, title: '全部文章' });
    setArticleId(null);
  }, []);
  const selectArticle = useCallback(
    (a: ArticleSummary) => {
      setArticleId(a.id);
      if (!a.isRead) markRead.mutate([a.id]);
    },
    [markRead],
  );

  const toggleStar = useCallback(
    (a: { id: number; isStarred: boolean }) => {
      setStarred.mutate({ id: a.id, starred: !a.isStarred });
    },
    [setStarred],
  );

  const doMarkAllRead = useCallback(() => {
    markAllRead.mutate({ feedId, folderId });
  }, [markAllRead, feedId, folderId]);

  const doRefreshAll = useCallback(() => {
    refresh.refreshAll.mutate();
  }, [refresh]);

  // ---- dialogs ------------------------------------------------------------
  const [feedDialog, setFeedDialog] = useState<{ open: boolean; initial: Partial<FeedDialogData> | null }>(
    { open: false, initial: null },
  );
  const [dialogBusy, setDialogBusy] = useState(false);
  const [dialogError, setDialogError] = useState<string | null>(null);

  const openAdd = useCallback(
    (parentId: number | null) =>
      setFeedDialog({ open: true, initial: { kind: 'feed', title: '', parentId } }),
    [],
  );
  const openAddFolder = useCallback(
    (parentId: number | null) =>
      setFeedDialog({ open: true, initial: { kind: 'folder', title: '', parentId } }),
    [],
  );
  const openEdit = useCallback(
    (node: FeedNode) =>
      setFeedDialog({
        open: true,
        initial: {
          kind: node.kind,
          id: node.id,
          title: node.title,
          feedUrl: node.feedUrl ?? undefined,
          parentId: node.parentId,
        },
      }),
    [],
  );

  const submitDialog = useCallback(
    async (data: FeedDialogData) => {
      setDialogBusy(true);
      setDialogError(null);
      try {
        if (data.id != null) {
          const node = findNode(data.id);
          await feedMut.updateFeed.mutateAsync({
            id: data.id,
            title: data.title,
            folderId: data.parentId,
            sortOrder: node?.sortOrder ?? 0,
          });
        } else if (data.kind === 'folder') {
          await feedMut.createFolder.mutateAsync({ title: data.title, parentId: data.parentId });
        } else {
          const node = await feedMut.createFeed.mutateAsync({
            url: data.feedUrl!.trim(),
            folderId: data.parentId,
          });
          // create_feed auto-derives the name from feed metadata; honour a
          // user-supplied override by renaming right after.
          if (data.title && data.title !== node.title) {
            await feedMut.updateFeed.mutateAsync({
              id: node.id,
              title: data.title,
              folderId: node.parentId,
              sortOrder: node.sortOrder,
            });
          }
        }
        setFeedDialog({ open: false, initial: null });
      } catch (e) {
        setDialogError(String(e));
      } finally {
        setDialogBusy(false);
      }
    },
    [feedMut, findNode],
  );

  const [confirmNode, setConfirmNode] = useState<FeedNode | null>(null);
  const doDelete = useCallback(async () => {
    if (!confirmNode) return;
    await feedMut.deleteFeed.mutateAsync(confirmNode.id);
    if (selection.id === confirmNode.id) selectAll();
    setConfirmNode(null);
  }, [confirmNode, feedMut, selection.id, selectAll]);

  const [settingsOpen, setSettingsOpen] = useState(false);
  const [shortcutsOpen, setShortcutsOpen] = useState(false);
  const [opmlMode, setOpmlMode] = useState<'import' | 'export' | null>(null);
  const [exportContent, setExportContent] = useState('');

  const openExport = useCallback(async () => {
    const xml = await api.exportOpml();
    setExportContent(xml);
    setOpmlMode('export');
  }, []);
  const doImport = useCallback(
    async (xml: string) => {
      const report = await api.importOpml(xml);
      invalidate();
      return report;
    },
    [invalidate],
  );

  // ---- background full-text prefetch --------------------------------------
  // Warm the whole loaded list before the user clicks in. We key on the id set
  // (not `items`, whose array identity changes every render) and track requested
  // ids in a ref, so each article is handed to the backend exactly once per
  // session. Only articles not yet readable in-app are requested; landed bodies
  // flip `fulltextReady` via the fulltext-ready event.
  const idsKey = items.map((a) => a.id).join(',');
  useEffect(() => {
    const batch: number[] = [];
    for (const a of items) {
      if (!a.fulltextReady && a.link && !prefetched.current.has(a.id)) {
        prefetched.current.add(a.id);
        batch.push(a.id);
      }
    }
    if (batch.length > 0) void api.prefetchFulltext(batch).catch(() => {});
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [idsKey]);

  // ---- keyboard shortcuts -------------------------------------------------
  const current = useMemo(() => items.find((a) => a.id === articleId) ?? null, [items, articleId]);

  const moveArticle = useCallback(
    (dir: number) => {
      if (items.length === 0) return;
      const idx = items.findIndex((a) => a.id === articleId);
      const base = idx === -1 ? (dir > 0 ? -1 : 0) : idx;
      const next = Math.min(items.length - 1, Math.max(0, base + dir));
      selectArticle(items[next]);
      document.getElementById(`art-${items[next].id}`)?.scrollIntoView({ block: 'nearest' });
    },
    [items, articleId, selectArticle],
  );

  const moveFeed = useCallback(
    (dir: number) => {
      const n = feedList.length;
      if (n === 0) return;
      const idx = feedList.findIndex((f) => f.id === selection.id);
      for (let step = 1; step <= n; step++) {
        const j = (((idx + dir * step) % n) + n) % n;
        if (feedList[j].unreadCount > 0) {
          selectNode(feedList[j]);
          return;
        }
      }
      // no unread found: just move by one
      const j = (((idx + dir) % n) + n) % n;
      selectNode(feedList[j]);
    },
    [feedList, selection.id, selectNode],
  );

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      const t = e.target as HTMLElement | null;
      const typing =
        !!t &&
        (t.tagName === 'INPUT' || t.tagName === 'TEXTAREA' || t.tagName === 'SELECT' || t.isContentEditable);
      const dialogOpen =
        feedDialog.open || settingsOpen || shortcutsOpen || confirmNode != null || opmlMode != null;

      if (e.key === '?' && !typing) {
        setShortcutsOpen((v) => !v);
        e.preventDefault();
        return;
      }
      if (typing || dialogOpen) return;

      switch (e.key) {
        case 'j':
        case 'ArrowDown':
          moveArticle(1);
          e.preventDefault();
          break;
        case 'k':
        case 'ArrowUp':
          moveArticle(-1);
          e.preventDefault();
          break;
        case 'n':
          moveFeed(1);
          break;
        case 'p':
          moveFeed(-1);
          break;
        case 'Enter':
          if (articleQ.data?.link) openExternal(articleQ.data.link);
          break;
        case 's':
          if (current) toggleStar(current);
          break;
        case 'u':
          if (current) {
            if (current.isRead) markUnread.mutate([current.id]);
            else markRead.mutate([current.id]);
          }
          break;
        case 'm':
          doMarkAllRead();
          break;
        case 'r':
          doRefreshAll();
          break;
        case '/':
          document.getElementById('feedly-search')?.focus();
          e.preventDefault();
          break;
        case 'c':
          openAdd(feedId ?? folderId);
          break;
        case ',':
          setSettingsOpen(true);
          break;
        default:
          break;
      }
    };
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  }, [
    feedDialog.open,
    settingsOpen,
    shortcutsOpen,
    confirmNode,
    opmlMode,
    moveArticle,
    moveFeed,
    articleQ.data,
    current,
    toggleStar,
    markRead,
    markUnread,
    doMarkAllRead,
    doRefreshAll,
    openAdd,
    feedId,
    folderId,
  ]);

  // ---- render -------------------------------------------------------------
  return (
    <div className="flex h-full w-full overflow-hidden bg-canvas text-slate-900 dark:text-slate-100">
      <Sidebar
        tree={tree}
        selectedId={selection.id}
        totalUnread={totalUnread}
        onSelectAll={selectAll}
        onSelect={selectNode}
        onAdd={openAdd}
        onAddFolder={openAddFolder}
        onEdit={openEdit}
        onDelete={setConfirmNode}
      />

      <main className="flex min-w-0 flex-1 flex-col">
        <Topbar
          title={selection.title}
          status={status}
          onStatus={setStatus}
          query={query}
          onQuery={setQuery}
          busy={progress.running}
          progress={progress.running ? { done: progress.done, total: progress.total } : null}
          onRefresh={doRefreshAll}
          onMarkAllRead={doMarkAllRead}
          onImportOpml={() => setOpmlMode('import')}
          onExportOpml={() => void openExport()}
          onOpenSettings={() => setSettingsOpen(true)}
        />

        <div className="flex min-h-0 flex-1">
          <div className="flex min-h-0 w-[360px] shrink-0 flex-col border-r border-slate-200 dark:border-slate-700">
            <ArticleList
              items={items}
              total={total}
              loading={articlesQ.isLoading}
              selectedId={articleId}
              hasMore={!!articlesQ.hasNextPage}
              fetchingMore={articlesQ.isFetchingNextPage}
              showSource={selection.kind !== 'feed'}
              onSelect={selectArticle}
              onToggleStar={toggleStar}
              onLoadMore={() => {
                if (!articlesQ.isFetching) void articlesQ.fetchNextPage();
              }}
            />
          </div>
          <ArticleView
            article={articleQ.data ?? null}
            loading={articleQ.isLoading}
            onToggleStar={toggleStar}
          />
        </div>
      </main>

      <Suspense fallback={null}>
      <FeedDialog
        open={feedDialog.open}
        folders={folders}
        initial={feedDialog.initial}
        busy={dialogBusy}
        error={dialogError}
        onClose={() => setFeedDialog({ open: false, initial: null })}
        onSubmit={(d) => void submitDialog(d)}
      />

      <SettingsDialog
        open={settingsOpen}
        settings={settings}
        onClose={() => setSettingsOpen(false)}
        onSave={(patch) => {
          updateSettings.mutate(patch);
          setSettingsOpen(false);
        }}
      />

      <ShortcutsHelp open={shortcutsOpen} onClose={() => setShortcutsOpen(false)} />

      <OpmlDialog
        mode={opmlMode}
        exportContent={exportContent}
        onClose={() => setOpmlMode(null)}
        onImport={doImport}
      />
      </Suspense>

      <Modal
        open={confirmNode != null}
        title="删除确认"
        onClose={() => setConfirmNode(null)}
        width="max-w-sm"
      >
        <p className="text-sm text-slate-600 dark:text-slate-300">
          确定要删除 <span className="font-semibold">{confirmNode?.title}</span>
          {confirmNode?.kind === 'folder' ? ' 及其中的所有订阅' : ' 及其文章'}？此操作无法撤销。
        </p>
        <div className="mt-4 flex justify-end gap-2">
          <button
            onClick={() => setConfirmNode(null)}
            className="rounded-lg px-4 py-2 text-sm font-medium text-slate-600 hover:bg-slate-100 dark:text-slate-300 dark:hover:bg-slate-700"
          >
            取消
          </button>
          <button
            onClick={() => void doDelete()}
            className="rounded-lg bg-red-600 px-4 py-2 text-sm font-medium text-white hover:bg-red-500"
          >
            删除
          </button>
        </div>
      </Modal>
    </div>
  );
}
