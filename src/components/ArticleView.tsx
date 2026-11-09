import { useEffect, useMemo, useRef, useState } from 'react';
import type { MouseEvent } from 'react';
import { convertFileSrc } from '@tauri-apps/api/core';
import type { Article } from '../lib/types';
import { absolutizeUrls, linkifyBareUrls, openExternal } from '../lib/utils';
import { api } from '../lib/api';
import { IconChevron, IconExternal, IconStar } from './Icons';
import Lightbox from './Lightbox';

interface Props {
  article: Article | null;
  loading: boolean;
  onToggleStar: (a: Article) => void;
}

/// Bare host name for the embedded page's toolbar (drops `www.`).
function hostOf(url: string): string {
  try {
    return new URL(url).host.replace(/^www\./, '');
  } catch {
    return url;
  }
}

export default function ArticleView({ article, loading, onToggleStar }: Props) {
  // Prefer the background-extracted full text when present, else the feed body.
  const srcHtml = article?.fulltextHtml ?? article?.contentHtml ?? '';
  // Linkify bare-text URLs in the body so they become clickable, then resolve
  // feed-relative image/link URLs against the article's own page so they don't
  // 404 against the app origin. Hook stays above the early returns.
  const bodyHtml = useMemo(
    () => absolutizeUrls(linkifyBareUrls(srcHtml), article?.link),
    [srcHtml, article?.link],
  );
  const [lightbox, setLightbox] = useState<{ src: string; alt: string } | null>(
    null,
  );
  // URL shown in the native original-page webview over the body area; null
  // means the extracted/feed body is showing. Set by the header "原文" button or
  // by clicking a bare URL in the body.
  const [originalUrl, setOriginalUrl] = useState<string | null>(null);

  // After paint, download this article's images into the local cache and
  // rewrite their src to the asset protocol so repeat/offline reads are instant
  // and hotlink protection is bypassed. Unresolved images keep the remote URL.
  const bodyRef = useRef<HTMLDivElement>(null);
  // Host element the native original-page webview is positioned over.
  const hostRef = useRef<HTMLDivElement>(null);
  useEffect(() => {
    const el = bodyRef.current;
    if (!el) return;
    const imgs = Array.from(el.querySelectorAll('img'));
    const urls = imgs
      .map((i) => i.getAttribute('src') ?? '')
      .filter((u) => u.startsWith('http://') || u.startsWith('https://'));
    if (urls.length === 0) return;
    let cancelled = false;
    void api
      .cacheImages(urls)
      .then((refs) => {
        if (cancelled) return;
        const map = new Map(refs.map((r) => [r.url, r.path]));
        for (const img of imgs) {
          const local = map.get(img.getAttribute('src') ?? '');
          if (local) img.src = convertFileSrc(local);
        }
      })
      .catch(() => {});
    return () => {
      cancelled = true;
    };
  }, [bodyHtml]);

  // Show a URL in a native child webview overlaying the body area, keeping it
  // aligned as the pane/window resizes. The back button restores the body;
  // switching articles or unmounting destroys the webview.
  useEffect(() => {
    const link = originalUrl;
    const host = hostRef.current;
    if (!link || !host) return;

    let active = true;
    const r0 = host.getBoundingClientRect();
    void api
      .openOriginalView(link, {
        x: r0.left,
        y: r0.top,
        width: r0.width,
        height: r0.height,
      })
      .catch(() => {});

    const sync = () => {
      if (!active) return;
      const r = host.getBoundingClientRect();
      if (r.width < 1 || r.height < 1) return;
      void api
        .setOriginalViewBounds({ x: r.left, y: r.top, width: r.width, height: r.height })
        .catch(() => {});
    };
    const ro = new ResizeObserver(sync);
    ro.observe(host);
    window.addEventListener('resize', sync);

    return () => {
      active = false;
      ro.disconnect();
      window.removeEventListener('resize', sync);
      void api.closeOriginalView().catch(() => {});
    };
  }, [originalUrl]);

  // Clicking an image opens the in-app lightbox (never navigates). A bare URL
  // auto-linked in the body opens the in-app original view (with a back button);
  // any other link — including an image wrapped in <a> — goes to the system
  // browser so the webview never leaves the app.
  const onBodyClick = (e: MouseEvent<HTMLDivElement>) => {
    const el = e.target as HTMLElement;
    const img = el.closest('img');
    if (img) {
      e.preventDefault();
      const src = img.getAttribute('src');
      if (src) setLightbox({ src, alt: img.getAttribute('alt') ?? '' });
      return;
    }
    const a = el.closest('a');
    const href = a?.getAttribute('href');
    if (!href) return;
    e.preventDefault();
    if (href.startsWith('#')) return;
    if (a?.hasAttribute('data-inline-original')) setOriginalUrl(href);
    else openExternal(href);
  };

  if (loading) {
    return <div className="flex-1 p-6 text-sm text-slate-400">加载中…</div>;
  }
  if (!article) {
    return (
      <div className="flex flex-1 items-center justify-center p-8 text-center text-sm text-slate-400">
        选择一篇文章开始阅读
      </div>
    );
  }

  return (
    <div className="flex min-h-0 flex-1 flex-col">
      <div className="z-10 flex items-center gap-2 border-b border-slate-200 bg-canvas px-6 py-3 dark:border-slate-700">
        <h2 className="min-w-0 flex-1 truncate text-base font-semibold text-slate-800 dark:text-slate-100">
          {article.title}
        </h2>
        <button
          onClick={() => onToggleStar(article)}
          className="rounded-lg p-2 text-slate-400 hover:text-amber-500"
          title="星标 (s)"
        >
          <IconStar
            width={18}
            height={18}
            className={article.isStarred ? 'fill-amber-400 text-amber-400' : ''}
          />
        </button>
        {(article.link || originalUrl) &&
          (originalUrl ? (
            <button
              onClick={() => setOriginalUrl(null)}
              className="rounded-lg p-2 text-slate-500 hover:bg-slate-100 hover:text-indigo-600 dark:text-slate-300 dark:hover:bg-slate-700"
              title="返回正文"
            >
              <IconChevron width={18} height={18} className="rotate-180" />
            </button>
          ) : (
            <button
              onClick={() => setOriginalUrl(article.link)}
              className="rounded-lg p-2 text-slate-400 hover:bg-slate-100 hover:text-indigo-600 dark:hover:bg-slate-700"
              title="在应用内打开原文"
            >
              <IconExternal width={18} height={18} />
            </button>
          ))}
      </div>

      {originalUrl ? (
        <div className="min-h-0 flex-1 overflow-hidden bg-canvas p-3">
          <div className="flex h-full flex-col overflow-hidden rounded-xl border border-slate-200 bg-white shadow-sm dark:border-slate-700 dark:bg-slate-900">
            <div className="flex h-9 shrink-0 items-center gap-2 border-b border-slate-200 px-3 dark:border-slate-700">
              <span
                className="min-w-0 flex-1 truncate text-xs text-slate-400"
                title={originalUrl}
              >
                {hostOf(originalUrl)}
              </span>
              <button
                onClick={() => openExternal(originalUrl)}
                className="rounded-md p-1.5 text-slate-400 hover:bg-slate-100 hover:text-indigo-600 dark:hover:bg-slate-700"
                title="在浏览器打开"
              >
                <IconExternal width={15} height={15} />
              </button>
            </div>
            <div ref={hostRef} className="min-h-0 flex-1 bg-white dark:bg-slate-900" />
          </div>
        </div>
      ) : (
        <div className="min-h-0 flex-1 overflow-y-auto">
      <article className="mx-auto w-full max-w-3xl px-6 py-5">
        <div className="mb-4 flex flex-wrap items-center gap-x-2 gap-y-1 text-xs text-slate-400">
          <span>{article.feedTitle}</span>
          {article.author && (
            <>
              <span>·</span>
              <span>{article.author}</span>
            </>
          )}
          {article.publishedAt && (
            <>
              <span>·</span>
              <span>{new Date(article.publishedAt).toLocaleString()}</span>
            </>
          )}
        </div>
        <div
          ref={bodyRef}
          className="article-body"
          onClick={onBodyClick}
          dangerouslySetInnerHTML={{ __html: bodyHtml }}
        />
      </article>
        </div>
      )}

      {lightbox && (
        <Lightbox
          src={lightbox.src}
          alt={lightbox.alt}
          onClose={() => setLightbox(null)}
        />
      )}
    </div>
  );
}
