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

const URL_HARD_TRAIL = '.,;:!?\'"`。，；：！？…';

function countChar(s: string, c: string): number {
  let n = 0;
  for (const ch of s) if (ch === c) n++;
  return n;
}

/// Splits punctuation glued to the end of a bare URL so it stays plain text,
/// while keeping brackets that legitimately belong to the URL (Wikipedia's
/// `Foo_(bar)`).
function splitTrailingPunct(raw: string): [string, string] {
  let url = raw;
  for (;;) {
    const last = url[url.length - 1];
    if (!last) break;
    if (URL_HARD_TRAIL.includes(last)) {
      url = url.slice(0, -1);
      continue;
    }
    if (
      (last === ')' && countChar(url, ')') > countChar(url, '(')) ||
      (last === ']' && countChar(url, ']') > countChar(url, '[')) ||
      (last === '}' && countChar(url, '}') > countChar(url, '{'))
    ) {
      url = url.slice(0, -1);
      continue;
    }
    break;
  }
  return [url, raw.slice(url.length)];
}

/// Wraps bare-text http(s) URLs (e.g. feeds that ship "原文链接： https://…" as
/// plain text) in anchors so they become clickable. Anchor gets
/// `data-inline-original` so the reader opens it in the in-app original-page view
/// instead of the system browser. Text inside existing <a>/<code>/<pre>/scripts
/// is left untouched.
export function linkifyBareUrls(html: string): string {
  if (!html || !/https?:\/\//i.test(html)) return html;
  const doc = new DOMParser().parseFromString(html, 'text/html');
  const SKIP = new Set(['A', 'PRE', 'CODE', 'SCRIPT', 'STYLE']);
  const walker = doc.createTreeWalker(doc.body, NodeFilter.SHOW_TEXT);
  const nodes: Text[] = [];
  for (let n = walker.nextNode(); n; n = walker.nextNode()) {
    const t = n as Text;
    let skip = false;
    for (let p = t.parentNode; p && p !== doc.body; p = p.parentNode) {
      if (p.nodeType === 1 && SKIP.has((p as Element).tagName)) {
        skip = true;
        break;
      }
    }
    if (!skip && /https?:\/\//i.test(t.data)) nodes.push(t);
  }
  for (const t of nodes) {
    const text = t.data;
    const re = /https?:\/\/[^\s<>"'`]+/gi;
    const frag = doc.createDocumentFragment();
    let last = 0;
    let m: RegExpExecArray | null;
    while ((m = re.exec(text))) {
      const [url, trail] = splitTrailingPunct(m[0]);
      if (!url) continue;
      if (m.index > last) frag.appendChild(doc.createTextNode(text.slice(last, m.index)));
      const a = doc.createElement('a');
      a.setAttribute('href', url);
      a.setAttribute('data-inline-original', '');
      a.textContent = url;
      frag.appendChild(a);
      if (trail) frag.appendChild(doc.createTextNode(trail));
      last = m.index + m[0].length;
    }
    if (last < text.length) frag.appendChild(doc.createTextNode(text.slice(last)));
    t.parentNode?.replaceChild(frag, t);
  }
  return doc.body.innerHTML;
}

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
