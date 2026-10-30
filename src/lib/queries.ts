import {
  keepPreviousData,
  useInfiniteQuery,
  useMutation,
  useQuery,
  useQueryClient,
  type QueryClient,
} from '@tanstack/react-query';
import { api } from './api';
import type {
  Article,
  ArticleFilter,
  ArticlePage,
  ArticleStatus,
  ArticleSummary,
  Settings,
} from './types';

export const PAGE_SIZE = 100;

export const keys = {
  feeds: ['feeds'] as const,
  articles: (filter: Omit<ArticleFilter, 'limit' | 'offset'>) =>
    ['articles', filter] as const,
  article: (id: number) => ['article', id] as const,
  settings: ['settings'] as const,
};

export function useFeeds() {
  return useQuery({ queryKey: keys.feeds, queryFn: api.listFeeds });
}

export function useSettings() {
  return useQuery({ queryKey: keys.settings, queryFn: api.getSettings });
}

/// Infinite list of article summaries for the given scope/status/search.
export function useArticles(params: {
  feedId: number | null;
  folderId: number | null;
  status: ArticleStatus;
  query: string;
}) {
  const scope = {
    feedId: params.feedId,
    folderId: params.folderId,
    status: params.status,
    query: params.query || null,
  };
  return useInfiniteQuery({
    queryKey: keys.articles(scope),
    initialPageParam: 0,
    placeholderData: keepPreviousData,
    queryFn: ({ pageParam }) =>
      api.listArticles({ ...scope, limit: PAGE_SIZE, offset: pageParam }),
    getNextPageParam: (last, allPages) => {
      const loaded = allPages.reduce((n, p) => n + p.items.length, 0);
      return loaded < last.total ? loaded : undefined;
    },
    select: (data) => {
      const items = data.pages.flatMap((p) => p.items);
      return { items, total: data.pages[0]?.total ?? 0 };
    },
  });
}

export function useArticle(id: number | null) {
  return useQuery({
    queryKey: keys.article(id ?? -1),
    queryFn: () => api.getArticle(id!),
    enabled: id != null,
  });
}

/// Shared invalidation used after any state-mutating action or backend event.
export function useInvalidate() {
  const qc = useQueryClient();
  return (opts?: { feeds?: boolean; articles?: boolean }) => {
    const wantFeeds = opts ? opts.feeds : true;
    const wantArticles = opts ? opts.articles : true;
    if (wantFeeds) qc.invalidateQueries({ queryKey: keys.feeds });
    if (wantArticles) qc.invalidateQueries({ queryKey: ['articles'] });
    qc.invalidateQueries({ queryKey: ['article'] });
  };
}

/// Optimistically patches summary fields across every cached article page and
/// the open full-article query, so the reading loop stays responsive without a
/// full list refetch (which would reset infinite-scroll state).
function patchArticles(
  qc: QueryClient,
  ids: number[],
  patch: Partial<ArticleSummary>,
) {
  const idSet = new Set(ids);
  qc.setQueriesData({ queryKey: ['articles'] }, (old) => {
    if (!old) return old;
    const data = old as { pages: ArticlePage[]; pageParams: unknown[] };
    return {
      ...data,
      pages: data.pages.map((p) => ({
        ...p,
        items: p.items.map((a) => (idSet.has(a.id) ? { ...a, ...patch } : a)),
      })),
    };
  });
}

export function useMarkRead() {
  const qc = useQueryClient();
  const invalidate = useInvalidate();
  return useMutation({
    mutationFn: (ids: number[]) => api.markRead(ids),
    onSuccess: (_d, ids) => {
      patchArticles(qc, ids, { isRead: true });
      invalidate({ feeds: true, articles: false });
    },
  });
}

export function useMarkUnread() {
  const qc = useQueryClient();
  const invalidate = useInvalidate();
  return useMutation({
    mutationFn: (ids: number[]) => api.markUnread(ids),
    onSuccess: (_d, ids) => {
      patchArticles(qc, ids, { isRead: false });
      invalidate({ feeds: true, articles: false });
    },
  });
}

export function useSetStarred() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: ({ id, starred }: { id: number; starred: boolean }) =>
      api.setStarred(id, starred),
    onSuccess: (_d, { id, starred }) => {
      patchArticles(qc, [id], { isStarred: starred });
      qc.setQueryData(keys.article(id), (old: Article | undefined) =>
        old ? { ...old, isStarred: starred } : old,
      );
    },
  });
}

export function useMarkAllRead() {
  const invalidate = useInvalidate();
  return useMutation({
    mutationFn: ({ feedId, folderId }: { feedId: number | null; folderId: number | null }) =>
      api.markAllRead(feedId, folderId),
    onSuccess: () => invalidate(),
  });
}

export function useFeedMutation() {
  const invalidate = useInvalidate();
  const qc = useQueryClient();
  const after = () => {
    invalidate();
    qc.invalidateQueries({ queryKey: keys.feeds });
  };
  return {
    createFeed: useMutation({
      mutationFn: ({ url, folderId }: { url: string; folderId: number | null }) =>
        api.createFeed(url, folderId),
      onSuccess: after,
    }),
    createFolder: useMutation({
      mutationFn: ({ title, parentId }: { title: string; parentId: number | null }) =>
        api.createFolder(title, parentId),
      onSuccess: after,
    }),
    updateFeed: useMutation({
      mutationFn: ({
        id,
        title,
        folderId,
        sortOrder,
      }: {
        id: number;
        title: string;
        folderId: number | null;
        sortOrder: number;
      }) => api.updateFeed(id, title, folderId, sortOrder),
      onSuccess: after,
    }),
    deleteFeed: useMutation({
      mutationFn: (id: number) => api.deleteFeed(id),
      onSuccess: after,
    }),
  };
}

export function useRefresh() {
  const invalidate = useInvalidate();
  return {
    refreshFeed: useMutation({
      mutationFn: (id: number) => api.refreshFeed(id),
      onSuccess: () => invalidate(),
    }),
    refreshAll: useMutation({
      mutationFn: () => api.refreshAll(),
    }),
  };
}

export function useUpdateSettings() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (input: Partial<Settings>) => api.updateSettings(input),
    onSuccess: (data) => qc.setQueryData(keys.settings, data),
  });
}
