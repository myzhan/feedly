import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import type {
  RefreshDone,
  RefreshProgress,
  RefreshStarted,
} from './types';

export interface RefreshHandlers {
  onStarted?: (p: RefreshStarted) => void;
  onProgress?: (p: RefreshProgress) => void;
  onDone?: (p: RefreshDone) => void;
  onArticleAdded?: (p: { feedId: number }) => void;
}

/// Subscribes to all refresh-related backend events. Returns a single cleanup
/// function that unlistens everything once all listeners are attached.
export async function subscribeRefresh(
  handlers: RefreshHandlers,
): Promise<UnlistenFn> {
  const unlisteners = await Promise.all([
    handlers.onStarted
      ? listen<RefreshStarted>('refresh-started', (e) => handlers.onStarted!(e.payload))
      : Promise.resolve<UnlistenFn>(() => {}),
    handlers.onProgress
      ? listen<RefreshProgress>('refresh-progress', (e) => handlers.onProgress!(e.payload))
      : Promise.resolve<UnlistenFn>(() => {}),
    handlers.onDone
      ? listen<RefreshDone>('refresh-done', (e) => handlers.onDone!(e.payload))
      : Promise.resolve<UnlistenFn>(() => {}),
    handlers.onArticleAdded
      ? listen<{ feedId: number }>('article-added', (e) => handlers.onArticleAdded!(e.payload))
      : Promise.resolve<UnlistenFn>(() => {}),
  ]);
  return () => unlisteners.forEach((fn) => fn());
}

export interface FulltextHandlers {
  onReady: (p: { articleId: number }) => void;
}

/// Subscribes to the backend `fulltext-ready` event, fired when a background
/// extraction persists a richer body for an article.
export async function subscribeFulltext(
  handlers: FulltextHandlers,
): Promise<UnlistenFn> {
  const unlisten = await listen<{ articleId: number }>('fulltext-ready', (e) =>
    handlers.onReady(e.payload),
  );
  return unlisten;
}
