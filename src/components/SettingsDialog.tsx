import { useEffect, useState } from 'react';
import Modal from './Modal';
import type { Settings } from '../lib/types';

interface Props {
  open: boolean;
  settings: Settings | undefined;
  onClose: () => void;
  onSave: (patch: Partial<Settings>) => void;
}

const INTERVALS = [
  { v: 0, label: '手动' },
  { v: 5, label: '5 分钟' },
  { v: 15, label: '15 分钟' },
  { v: 30, label: '30 分钟' },
  { v: 60, label: '1 小时' },
  { v: 360, label: '6 小时' },
  { v: 720, label: '12 小时' },
  { v: 1440, label: '24 小时' },
];

const THEMES: { v: Settings['theme']; label: string }[] = [
  { v: 'system', label: '跟随系统' },
  { v: 'light', label: '浅色' },
  { v: 'dark', label: '深色' },
];

export default function SettingsDialog({ open, settings, onClose, onSave }: Props) {
  const [interval, setInterval] = useState(15);
  const [theme, setTheme] = useState<Settings['theme']>('system');

  useEffect(() => {
    if (!open || !settings) return;
    setInterval(settings.refreshIntervalMinutes);
    setTheme(settings.theme);
  }, [open, settings]);

  return (
    <Modal open={open} onClose={onClose} title="设置">
      <div className="space-y-5">
        <div>
          <span className="mb-2 block text-sm font-medium text-slate-600 dark:text-slate-300">
            自动刷新间隔
          </span>
          <div className="grid grid-cols-4 gap-2">
            {INTERVALS.map((o) => (
              <button
                key={o.v}
                onClick={() => setInterval(o.v)}
                className={`rounded-lg border px-2 py-1.5 text-sm ${
                  interval === o.v
                    ? 'border-indigo-500 bg-indigo-50 text-indigo-700 dark:bg-indigo-500/15 dark:text-indigo-300'
                    : 'border-slate-200 text-slate-600 hover:bg-slate-50 dark:border-slate-700 dark:text-slate-300 dark:hover:bg-slate-700/50'
                }`}
              >
                {o.label}
              </button>
            ))}
          </div>
        </div>

        <div>
          <span className="mb-2 block text-sm font-medium text-slate-600 dark:text-slate-300">
            主题
          </span>
          <div className="flex gap-2">
            {THEMES.map((o) => (
              <button
                key={o.v}
                onClick={() => setTheme(o.v)}
                className={`flex-1 rounded-lg border px-2 py-1.5 text-sm ${
                  theme === o.v
                    ? 'border-indigo-500 bg-indigo-50 text-indigo-700 dark:bg-indigo-500/15 dark:text-indigo-300'
                    : 'border-slate-200 text-slate-600 hover:bg-slate-50 dark:border-slate-700 dark:text-slate-300 dark:hover:bg-slate-700/50'
                }`}
              >
                {o.label}
              </button>
            ))}
          </div>
        </div>

        <div className="flex justify-end gap-2 pt-1">
          <button
            onClick={onClose}
            className="rounded-lg px-4 py-2 text-sm font-medium text-slate-600 hover:bg-slate-100 dark:text-slate-300 dark:hover:bg-slate-700"
          >
            取消
          </button>
          <button
            onClick={() => onSave({ refreshIntervalMinutes: interval, theme })}
            className="rounded-lg bg-indigo-600 px-4 py-2 text-sm font-medium text-white hover:bg-indigo-500"
          >
            保存
          </button>
        </div>
      </div>
    </Modal>
  );
}
