import { openUrl } from '@tauri-apps/plugin-opener';
import type { FeedNode } from './types';

/// Opens an external URL in the system browser.
export function openExternal(url: string | null | undefined) {
  if (!url) return;
  void openUrl(url);
}

/// Relative "time ago" formatting for article lists.
export function relativeTime(iso: string | null | undefined): string {
  if (!iso) return '';
  const then = new Date(iso).getTime();
  if (Number.isNaN(then)) return '';
  const diff = Date.now() - then;
  const min = 60_000;
  const hour = 60 * min;
  const day = 24 * hour;
  if (diff < min) return '刚刚';
  if (diff < hour) return `${Math.floor(diff / min)} 分钟前`;
  if (diff < day) return `${Math.floor(diff / hour)} 小时前`;
  if (diff < 7 * day) return `${Math.floor(diff / day)} 天前`;
  return new Date(then).toLocaleDateString();
}

/// Flattens the feed tree into a list, depth-first, preserving order.
export function flattenFeeds(nodes: FeedNode[]): FeedNode[] {
  const out: FeedNode[] = [];
  const walk = (list: FeedNode[]) => {
    for (const n of list) {
      out.push(n);
      if (n.children.length) walk(n.children);
    }
  };
  walk(nodes);
  return out;
}

/// Returns the list of selectable folder/feed ids for keyboard navigation.
export function collectFeeds(nodes: FeedNode[]): FeedNode[] {
  return flattenFeeds(nodes).filter((n) => n.kind === 'feed');
}

const ABSOLUTE_RE = /^(https?:|data:|blob:|mailto:|tel:|cid:)/i;

/// Rewrites relative / site-root URLs in sanitized article HTML into absolute
/// ones resolved against the article's canonical link. Feed bodies often use
/// paths like `/images/x.png` or `thumb.jpg`, which would otherwise resolve
/// against the app's own origin and 404 (broken images / links).
export function absolutizeUrls(html: string, base: string | null | undefined): string {
  if (!base) return html;
  let baseUrl: URL;
  try {
    baseUrl = new URL(base);
  } catch {
    return html;
  }
  if (baseUrl.protocol !== 'http:' && baseUrl.protocol !== 'https:') return html;

  const resolve = (raw: string | null): string | null => {
    const v = raw?.trim();
    if (!v || v.startsWith('#') || ABSOLUTE_RE.test(v)) return null;
    try {
      return new URL(v, baseUrl).href;
    } catch {
      return null;
    }
  };

  const doc = new DOMParser().parseFromString(html, 'text/html');
  const fixAttr = (el: Element, attr: string) => {
    const abs = resolve(el.getAttribute(attr));
    if (abs) el.setAttribute(attr, abs);
  };
  doc.querySelectorAll('img').forEach((el) => {
    fixAttr(el, 'src');
    fixAttr(el, 'data-src');
    const ss = el.getAttribute('srcset');
    if (ss) {
      const rewritten = ss
        .split(',')
        .map((part) => {
          const seg = part.trim();
          const sp = seg.search(/\s/);
          const u = sp === -1 ? seg : seg.slice(0, sp);
          const d = sp === -1 ? '' : seg.slice(sp);
          return (resolve(u) ?? u) + d;
        })
        .join(', ');
      el.setAttribute('srcset', rewritten);
    }
  });
  doc.querySelectorAll('a').forEach((el) => fixAttr(el, 'href'));
  doc.querySelectorAll('source').forEach((el) => fixAttr(el, 'src'));
  doc.querySelectorAll('video,audio').forEach((el) => {
    fixAttr(el, 'src');
    fixAttr(el, 'poster');
  });
  return doc.body.innerHTML;
}
