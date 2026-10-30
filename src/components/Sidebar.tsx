import { useMemo, useState } from 'react';
import type { FeedNode } from '../lib/types';
import { IconChevron, IconFolder, IconMenu, IconPlus, IconRss } from './Icons';

interface Props {
  tree: FeedNode[];
  selectedId: number | null; // null = "all feeds"
  totalUnread: number;
  onSelectAll: () => void;
  onSelect: (node: FeedNode) => void;
  onAdd: (parentId: number | null) => void;
  onAddFolder: (parentId: number | null) => void;
  onEdit: (node: FeedNode) => void;
  onDelete: (node: FeedNode) => void;
}

/// A row-level action menu revealed via the ⋮ button.
function RowMenu({
  node,
  onEdit,
  onDelete,
}: {
  node: FeedNode;
  onEdit: (n: FeedNode) => void;
  onDelete: (n: FeedNode) => void;
}) {
  const [open, setOpen] = useState(false);
  return (
    <div className="relative">
      <button
        onClick={(e) => {
          e.stopPropagation();
          setOpen((v) => !v);
        }}
        className="rounded p-1 text-slate-400 hover:bg-slate-200 hover:text-slate-600 dark:hover:bg-slate-600"
        aria-label="更多操作"
      >
        <IconMenu width={14} height={14} />
      </button>
      {open && (
        <>
          <div className="fixed inset-0 z-10" onClick={() => setOpen(false)} />
          <div className="absolute right-0 z-20 mt-1 w-32 rounded-lg border border-slate-200 bg-white py-1 text-sm shadow-lg dark:border-slate-600 dark:bg-slate-800">
            <button
              onClick={(e) => {
                e.stopPropagation();
                setOpen(false);
                onEdit(node);
              }}
              className="block w-full px-3 py-1.5 text-left hover:bg-slate-100 dark:hover:bg-slate-700"
            >
              重命名/移动
            </button>
            <button
              onClick={(e) => {
                e.stopPropagation();
                setOpen(false);
                onDelete(node);
              }}
              className="block w-full px-3 py-1.5 text-left text-red-600 hover:bg-red-50 dark:text-red-400 dark:hover:bg-red-500/10"
            >
              删除
            </button>
          </div>
        </>
      )}
    </div>
  );
}

function UnreadBadge({ count }: { count: number }) {
  if (count <= 0) return null;
  return (
    <span className="ml-auto shrink-0 rounded-full bg-indigo-100 px-2 py-0.5 text-xs font-medium text-indigo-700 dark:bg-indigo-500/20 dark:text-indigo-300">
      {count > 999 ? '999+' : count}
    </span>
  );
}

function Node({
  node,
  depth,
  selectedId,
  collapsed,
  toggle,
  onSelect,
  onEdit,
  onDelete,
}: {
  node: FeedNode;
  depth: number;
  selectedId: number | null;
  collapsed: Set<number>;
  toggle: (id: number) => void;
  onSelect: (n: FeedNode) => void;
  onEdit: (n: FeedNode) => void;
  onDelete: (n: FeedNode) => void;
}) {
  const isFolder = node.kind === 'folder';
  const isOpen = !collapsed.has(node.id);
  const active = selectedId === node.id;

  return (
    <>
      <div
        onClick={() => (isFolder ? toggle(node.id) : onSelect(node))}
        className={`group flex cursor-pointer items-center gap-1.5 rounded-lg py-1.5 pr-2 text-sm ${
          active
            ? 'bg-indigo-50 text-indigo-700 dark:bg-indigo-500/15 dark:text-indigo-300'
            : 'text-slate-700 hover:bg-slate-100 dark:text-slate-300 dark:hover:bg-slate-700/60'
        }`}
        style={{ paddingLeft: 8 + depth * 14 }}
      >
        {isFolder ? (
          <>
            <IconChevron
              width={14}
              height={14}
              className={`shrink-0 transition-transform ${isOpen ? 'rotate-90' : ''}`}
            />
            <IconFolder width={15} height={15} className="shrink-0 text-amber-500" />
          </>
        ) : (
          <>
            <span className="w-[14px] shrink-0" />
            <IconRss width={15} height={15} className="shrink-0 text-orange-500" />
          </>
        )}
        <span className="truncate">{node.title}</span>
        <UnreadBadge count={node.unreadCount} />
        <div className={`ml-1 shrink-0 ${isFolder ? '' : 'opacity-0 group-hover:opacity-100'}`}>
          <RowMenu node={node} onEdit={onEdit} onDelete={onDelete} />
        </div>
      </div>
      {isFolder &&
        isOpen &&
        node.children.map((child) => (
          <Node
            key={child.id}
            node={child}
            depth={depth + 1}
            selectedId={selectedId}
            collapsed={collapsed}
            toggle={toggle}
            onSelect={onSelect}
            onEdit={onEdit}
            onDelete={onDelete}
          />
        ))}
    </>
  );
}

export default function Sidebar({
  tree,
  selectedId,
  totalUnread,
  onSelectAll,
  onSelect,
  onAdd,
  onAddFolder,
  onEdit,
  onDelete,
}: Props) {
  const [collapsed, setCollapsed] = useState<Set<number>>(new Set());
  const toggle = (id: number) =>
    setCollapsed((prev) => {
      const next = new Set(prev);
      if (next.has(id)) next.delete(id);
      else next.add(id);
      return next;
    });

  const folderIds = useMemo(() => {
    const out: number[] = [];
    const walk = (list: FeedNode[]) => {
      for (const n of list) {
        if (n.kind === 'folder') {
          out.push(n.id);
          walk(n.children);
        }
      }
    };
    walk(tree);
    return out;
  }, [tree]);

  return (
    <aside className="flex h-full w-64 shrink-0 flex-col border-r border-slate-200 bg-canvas-soft dark:border-slate-700">
      <div className="border-b border-slate-200 p-3 dark:border-slate-700">
        <div
          onClick={onSelectAll}
          className={`flex cursor-pointer items-center gap-2 rounded-lg px-2 py-1.5 text-sm font-medium ${
            selectedId === null
              ? 'bg-indigo-50 text-indigo-700 dark:bg-indigo-500/15 dark:text-indigo-300'
              : 'text-slate-700 hover:bg-slate-100 dark:text-slate-300 dark:hover:bg-slate-700/60'
          }`}
        >
          <IconRss width={16} height={16} className="text-orange-500" />
          <span>全部文章</span>
          <UnreadBadge count={totalUnread} />
        </div>
      </div>

      <div className="flex items-center justify-between px-3 py-2">
        <span className="text-xs font-semibold uppercase tracking-wide text-slate-400">
          订阅
        </span>
        <div className="flex gap-1">
          <button
            onClick={() => onAddFolder(null)}
            title="新建文件夹"
            className="rounded p-1 text-slate-500 hover:bg-slate-200 dark:hover:bg-slate-700"
          >
            <IconFolder width={15} height={15} />
          </button>
          <button
            onClick={() => onAdd(null)}
            title="添加订阅"
            className="rounded p-1 text-slate-500 hover:bg-slate-200 dark:hover:bg-slate-700"
          >
            <IconPlus width={15} height={15} />
          </button>
        </div>
      </div>

      <div className="min-h-0 flex-1 overflow-y-auto px-2 pb-3">
        {tree.length === 0 ? (
          <p className="px-2 py-6 text-center text-xs text-slate-400">
            还没有订阅，点击右上角 + 添加
          </p>
        ) : (
          tree.map((n) => (
            <Node
              key={n.id}
              node={n}
              depth={0}
              selectedId={selectedId}
              collapsed={collapsed}
              toggle={toggle}
              onSelect={onSelect}
              onEdit={onEdit}
              onDelete={onDelete}
            />
          ))
        )}
      </div>

      {folderIds.length > 0 && (
        <div className="flex justify-end gap-3 border-t border-slate-200 p-2 text-xs text-slate-400 dark:border-slate-700">
          <button
            onClick={() => setCollapsed(new Set())}
            className="hover:text-slate-600 dark:hover:text-slate-300"
          >
            展开全部
          </button>
          <button
            onClick={() => setCollapsed(new Set(folderIds))}
            className="hover:text-slate-600 dark:hover:text-slate-300"
          >
            折叠全部
          </button>
        </div>
      )}
    </aside>
  );
}
