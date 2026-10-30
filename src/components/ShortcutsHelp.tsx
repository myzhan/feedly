import Modal from './Modal';

interface Props {
  open: boolean;
  onClose: () => void;
}

const ROWS: { keys: string[]; desc: string }[] = [
  { keys: ['j', '↓'], desc: '下一篇文章' },
  { keys: ['k', '↑'], desc: '上一篇文章' },
  { keys: ['n'], desc: '下一个未读订阅源' },
  { keys: ['p'], desc: '上一个未读订阅源' },
  { keys: ['Enter'], desc: '打开原文（浏览器）' },
  { keys: ['s'], desc: '星标 / 取消星标当前文章' },
  { keys: ['u'], desc: '切换当前文章已读 / 未读' },
  { keys: ['m'], desc: '当前订阅全部标记已读' },
  { keys: ['r'], desc: '刷新全部订阅' },
  { keys: ['/'], desc: '聚焦搜索框' },
  { keys: ['c'], desc: '新建订阅' },
  { keys: ['?'], desc: '显示 / 隐藏本帮助' },
  { keys: [','], desc: '打开设置' },
  { keys: ['Esc'], desc: '关闭对话框 / 清除选择' },
];

export default function ShortcutsHelp({ open, onClose }: Props) {
  return (
    <Modal open={open} onClose={onClose} title="键盘快捷键">
      <div className="divide-y divide-slate-100 dark:divide-slate-700">
        {ROWS.map((row) => (
          <div key={row.desc} className="flex items-center justify-between py-2 text-sm">
            <span className="text-slate-600 dark:text-slate-300">{row.desc}</span>
            <span className="flex gap-1">
              {row.keys.map((k) => (
                <kbd
                  key={k}
                  className="rounded border border-slate-300 bg-slate-50 px-1.5 py-0.5 text-xs font-medium text-slate-600 dark:border-slate-600 dark:bg-slate-700 dark:text-slate-200"
                >
                  {k}
                </kbd>
              ))}
            </span>
          </div>
        ))}
      </div>
    </Modal>
  );
}
