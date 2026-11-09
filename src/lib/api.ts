import { invoke } from '@tauri-apps/api/core';
import type {
  Article,
  ArticleFilter,
  ArticlePage,
  FeedNode,
  ImportReport,
  Settings,
} from './types';

// Rust command names are snake_case; argument keys are camelCase (Tauri default).
export const api = {
  listFeeds: () => invoke<FeedNode[]>('list_feeds'),

  createFolder: (title: string, parentId: number | null) =>
    invoke<FeedNode>('create_folder', { title, parentId }),

  createFeed: (url: string, folderId: number | null) =>
    invoke<FeedNode>('create_feed', { url, folderId }),

  updateFeed: (
    id: number,
    title: string,
    folderId: number | null,
    sortOrder: number,
  ) => invoke<FeedNode>('update_feed', { id, title, folderId, sortOrder }),

  deleteFeed: (id: number) => invoke<void>('delete_feed', { id }),

  listArticles: (filter: ArticleFilter) =>
    invoke<ArticlePage>('list_articles', { filter }),

  getArticle: (id: number) => invoke<Article>('get_article', { id }),

  markRead: (ids: number[]) => invoke<void>('mark_read', { ids }),
  markUnread: (ids: number[]) => invoke<void>('mark_unread', { ids }),

  setStarred: (id: number, starred: boolean) =>
    invoke<boolean>('set_starred', { id, starred }),

  markAllRead: (feedId: number | null, folderId: number | null) =>
    invoke<number>('mark_all_read', { feedId, folderId }),

  refreshFeed: (id: number) => invoke<number>('refresh_feed', { id }),
  refreshAll: () => invoke<void>('refresh_all'),

  exportOpml: () => invoke<string>('export_opml'),
  importOpml: (xml: string) => invoke<ImportReport>('import_opml', { xml }),

  getSettings: () => invoke<Settings>('get_settings'),
  updateSettings: (input: Partial<Settings>) =>
    invoke<Settings>('update_settings', { input }),

  // Queues background full-text extraction for the given article ids (the
  // loaded list). Returns immediately; ready bodies arrive via the
  // `fulltext-ready` event.
  prefetchFulltext: (ids: number[]) => invoke<void>('prefetch_fulltext', { ids }),

  // Ensures images are cached locally; returns the subset that succeeded with
  // their absolute local paths.
  cacheImages: (urls: string[]) =>
    invoke<{ url: string; path: string }[]>('cache_images', { urls }),

  // Renders an article's original page in a native child webview covering the
  // given rectangle (logical px within the main window), on top of the reader.
  openOriginalView: (url: string, rect: ViewRect) =>
    invoke<void>('open_original_view', { url, ...rect }),
  setOriginalViewBounds: (rect: ViewRect) =>
    invoke<void>('set_original_view_bounds', { ...rect }),
  closeOriginalView: () => invoke<void>('close_original_view'),
};

/// Logical-pixel rectangle for positioning the embedded original-page webview.
export interface ViewRect {
  x: number;
  y: number;
  width: number;
  height: number;
}
