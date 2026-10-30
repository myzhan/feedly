import { useEffect, useMemo, useRef, useState } from 'react';
import type { MouseEvent } from 'react';
import { convertFileSrc } from '@tauri-apps/api/core';
import type { Article } from '../lib/types';
import { absolutizeUrls, openExternal } from '../lib/utils';
import { api } from '../lib/api';
import { IconExternal, IconStar } from './Icons';
import Lightbox from './Lightbox';

interface Props {
  article: Article | null;
  loading: boolean;
  onToggleStar: (a: Article) => void;
}

export default function ArticleView({ article, loading, onToggleStar }: Props) {
  // Prefer the background-extracted full text when present, else the feed body.
  const srcHtml = article?.fulltextHtml ?? article?.contentHtml ?? '';
  // Resolve feed-relative image/link URLs against the article's own page so
  // they don't 404 against the app origin. Hook stays above the early returns.
  const bodyHtml = useMemo(() => absolutizeUrls(srcHtml, article?.link), [srcHtml, article?.link]);
  const [lightbox, setLightbox] = useState<{ src: string; alt: string } | null>(
    null,
  );

  // After paint, download this article's images into the local cache and
  // rewrite their src to the asset protocol so repeat/offline reads are instant
  // and hotlink protection is bypassed. Unresolved images keep the remote URL.
  const bodyRef = useRef<HTMLDivElement>(null);
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

  // Clicking an image opens the in-app lightbox (never navigates). Any other
  // link — including an image wrapped in <a> — is routed to the system browser
  // so the webview never leaves the app.
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
    if (!href.startsWith('#')) openExternal(href);
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
    <div className="flex min-h-0 flex-1 flex-col overflow-y-auto">
      <div className="sticky top-0 z-10 flex items-center gap-2 border-b border-slate-200 bg-canvas/80 px-6 py-3 backdrop-blur dark:border-slate-700">
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
        {article.link && (
          <button
            onClick={() => openExternal(article.link)}
            className="rounded-lg p-2 text-slate-400 hover:bg-slate-100 hover:text-indigo-600 dark:hover:bg-slate-700"
            title="在浏览器打开原文"
          >
            <IconExternal width={18} height={18} />
          </button>
        )}
      </div>

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
