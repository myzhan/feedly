// TypeScript mirrors of the Rust command payloads (camelCase via serde).

export type FeedKind = 'feed' | 'folder';

export interface FeedNode {
  id: number;
  parentId: number | null;
  kind: FeedKind;
  title: string;
  feedUrl: string | null;
  siteUrl: string | null;
  description: string | null;
  iconUrl: string | null;
  sortOrder: number;
  fetchError: string | null;
  fetchFailCount: number;
  lastFetchedAt: string | null;
  unreadCount: number;
  children: FeedNode[];
}

export interface ArticleSummary {
  id: number;
  feedId: number;
  feedTitle: string;
  title: string;
  link: string | null;
  author: string | null;
  publishedAt: string | null;
  plainText: string;
  isStarred: boolean;
  isRead: boolean;
  fulltextReady: boolean;
}

export interface Article extends Omit<ArticleSummary, 'plainText' | 'fulltextReady'> {
  contentHtml: string;
  fulltextHtml: string | null;
}

export interface ArticlePage {
  items: ArticleSummary[];
  total: number;
}

export type ArticleStatus = 'all' | 'unread' | 'starred';

export interface ArticleFilter {
  feedId?: number | null;
  folderId?: number | null;
  status: ArticleStatus;
  query?: string | null;
  limit: number;
  offset: number;
}

export interface Settings {
  refreshIntervalMinutes: number;
  theme: 'system' | 'light' | 'dark';
}

export interface ImportReport {
  feedsAdded: number;
  feedsSkipped: number;
  foldersCreated: number;
}

export interface RefreshStarted {
  total: number;
}
export interface RefreshProgress {
  feedId: number;
  newCount: number;
  notModified: boolean;
  error: string | null;
}
export interface RefreshDone {
  processed: number;
  total: number;
  newArticles: number;
  skipped: boolean;
}
