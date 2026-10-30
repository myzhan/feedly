import { useRef, useState } from 'react';
import Modal from './Modal';
import type { ImportReport } from '../lib/types';

interface Props {
  mode: 'import' | 'export' | null;
  exportContent: string;
  onClose: () => void;
  onImport: (xml: string) => Promise<ImportReport>;
}

export default function OpmlDialog({ mode, exportContent, onClose, onImport }: Props) {
  const [text, setText] = useState('');
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [report, setReport] = useState<ImportReport | null>(null);
  const fileRef = useRef<HTMLInputElement>(null);

  if (!mode) return null;

  const readFiles = async (files: FileList | null) => {
    if (!files || files.length === 0) return;
    setError(null);
    const buf = await files[0].text();
    setText(buf);
  };

  const doImport = async () => {
    if (!text.trim()) {
      setError('请粘贴或选择 OPML 文件');
      return;
    }
    setBusy(true);
    setError(null);
    try {
      setReport(await onImport(text));
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  };

  const download = () => {
    const blob = new Blob([exportContent], { type: 'text/xml' });
    const url = URL.createObjectURL(blob);
    const a = document.createElement('a');
    a.href = url;
    a.download = 'feedly-opml.xml';
    a.click();
    URL.revokeObjectURL(url);
  };

  return (
    <Modal
      open
      onClose={onClose}
      title={mode === 'import' ? '导入 OPML' : '导出 OPML'}
      width="max-w-2xl"
    >
      {mode === 'import' ? (
        <div className="space-y-3">
          <div className="flex items-center gap-2">
            <input
              ref={fileRef}
              type="file"
              accept=".opml,.xml,text/xml"
              className="hidden"
              onChange={(e) => void readFiles(e.target.files)}
            />
            <button
              onClick={() => fileRef.current?.click()}
              className="rounded-lg border border-slate-300 px-3 py-1.5 text-sm hover:bg-slate-50 dark:border-slate-600 dark:hover:bg-slate-700/50"
            >
              选择文件…
            </button>
            <span className="text-xs text-slate-400">或直接粘贴 OPML 内容</span>
          </div>
          <textarea
            value={text}
            onChange={(e) => setText(e.target.value)}
            placeholder="<?xml version=&quot;1.0&quot?>&#10;<opml>…"
            rows={10}
            className="w-full rounded-lg border border-slate-300 p-2 font-mono text-xs outline-none focus:border-indigo-500 dark:border-slate-600 dark:bg-slate-900/50"
          />
          {error && <p className="text-sm text-red-600 dark:text-red-400">{error}</p>}
          {report && (
            <p className="rounded-lg bg-emerald-50 px-3 py-2 text-sm text-emerald-700 dark:bg-emerald-500/10 dark:text-emerald-300">
              新增 {report.feedsAdded} 个订阅，跳过 {report.feedsSkipped} 个，创建{' '}
              {report.foldersCreated} 个文件夹。
            </p>
          )}
          <div className="flex justify-end gap-2">
            <button
              onClick={onClose}
              className="rounded-lg px-4 py-2 text-sm font-medium text-slate-600 hover:bg-slate-100 dark:text-slate-300 dark:hover:bg-slate-700"
            >
              {report ? '完成' : '取消'}
            </button>
            <button
              onClick={() => void doImport()}
              disabled={busy}
              className="rounded-lg bg-indigo-600 px-4 py-2 text-sm font-medium text-white hover:bg-indigo-500 disabled:opacity-50"
            >
              {busy ? '导入中…' : '导入'}
            </button>
          </div>
        </div>
      ) : (
        <div className="space-y-3">
          <textarea
            readOnly
            value={exportContent}
            rows={12}
            className="w-full rounded-lg border border-slate-300 p-2 font-mono text-xs outline-none dark:border-slate-600 dark:bg-slate-900/50"
          />
          <div className="flex justify-end gap-2">
            <button
              onClick={() => void navigator.clipboard.writeText(exportContent)}
              className="rounded-lg border border-slate-300 px-4 py-2 text-sm font-medium hover:bg-slate-50 dark:border-slate-600 dark:hover:bg-slate-700/50"
            >
              复制到剪贴板
            </button>
            <button
              onClick={download}
              className="rounded-lg bg-indigo-600 px-4 py-2 text-sm font-medium text-white hover:bg-indigo-500"
            >
              下载 .opml
            </button>
          </div>
        </div>
      )}
    </Modal>
  );
}
