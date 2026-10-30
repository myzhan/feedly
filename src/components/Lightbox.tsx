import { useEffect, useRef, useState } from 'react';
import type { PointerEvent as ReactPointerEvent } from 'react';

interface Props {
  src: string;
  alt?: string;
  onClose: () => void;
}

const MIN = 0.3; // 30% of viewport width
const MAX = 8; // 800% of viewport width

/// In-app image preview overlay. Does not navigate: click the backdrop / close
/// button / press Escape to dismiss. The vertical slider on the right resizes
/// the image (great for SVG flame graphs — vector stays crisp when enlarged);
/// clicking the image toggles between fit and a quick zoom.
export default function Lightbox({ src, alt, onClose }: Props) {
  const [scale, setScale] = useState(1);
  const trackRef = useRef<HTMLDivElement>(null);
  const scrollRef = useRef<HTMLDivElement>(null);
  const dragging = useRef(false);

  useEffect(() => {
    // Capture phase + stopPropagation so the app's global shortcuts (j/k/…)
    // are suppressed while the lightbox is open.
    const onKey = (e: KeyboardEvent) => {
      e.stopPropagation();
      if (e.key === 'Escape') onClose();
    };
    window.addEventListener('keydown', onKey, true);
    return () => window.removeEventListener('keydown', onKey, true);
  }, [onClose]);

  useEffect(() => {
    // Wheel resizes the image. Registered natively with passive:false because
    // React's onWheel can't preventDefault, and the scroll container would
    // otherwise pan instead of zooming.
    const el = scrollRef.current;
    if (!el) return;
    const onWheel = (e: WheelEvent) => {
      e.preventDefault();
      setScale((s) => {
        const next = s * Math.exp(-e.deltaY * 0.0015);
        return Math.min(MAX, Math.max(MIN, next));
      });
    };
    el.addEventListener('wheel', onWheel, { passive: false });
    return () => el.removeEventListener('wheel', onWheel);
  }, []);

  const setFromClientY = (clientY: number) => {
    const el = trackRef.current;
    if (!el) return;
    const r = el.getBoundingClientRect();
    // Top of the track = largest, bottom = smallest.
    const t = Math.min(1, Math.max(0, (clientY - r.top) / r.height));
    setScale(MAX - t * (MAX - MIN));
  };

  const onPointerDown = (e: ReactPointerEvent<HTMLDivElement>) => {
    e.stopPropagation();
    dragging.current = true;
    e.currentTarget.setPointerCapture(e.pointerId);
    setFromClientY(e.clientY);
  };
  const onPointerMove = (e: ReactPointerEvent<HTMLDivElement>) => {
    if (dragging.current) setFromClientY(e.clientY);
  };
  const stopDrag = () => {
    dragging.current = false;
  };

  const thumbPct = ((MAX - scale) / (MAX - MIN)) * 100;

  return (
    <div
      className="fixed inset-0 z-50 bg-black/85 backdrop-blur-sm"
      onClick={onClose}
      role="dialog"
      aria-modal="true"
      aria-label="图片预览"
    >
      <button
        onClick={(e) => {
          e.stopPropagation();
          onClose();
        }}
        className="absolute right-4 top-4 z-10 rounded-full bg-white/10 p-2 text-white hover:bg-white/25"
        aria-label="关闭"
        title="关闭 (Esc)"
      >
        <svg width={22} height={22} viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth={2} strokeLinecap="round">
          <path d="M6 6l12 12M18 6L6 18" />
        </svg>
      </button>

      {/* Scroll container: flex + `m-auto` on the image centres it on both
          axes when it fits, and collapses the auto margins to 0 (scroll from
          the start, no clipping) once it's enlarged past the viewport. */}
      <div ref={scrollRef} className="absolute inset-0 flex overflow-auto p-4">
        <img
          src={src}
          alt={alt || ''}
          onClick={(e) => {
            e.stopPropagation();
            setScale((s) => (Math.abs(s - 1) < 0.01 ? 3 : 1));
          }}
          style={
            scale <= 1
              ? {
                  width: 'auto',
                  height: 'auto',
                  maxWidth: `${scale * 92}vw`,
                  maxHeight: `${scale * 90}vh`,
                }
              : {
                  width: `${scale * 92}vw`,
                  height: 'auto',
                  maxWidth: 'none',
                  maxHeight: 'none',
                }
          }
          className="m-auto select-none rounded-lg"
          draggable={false}
        />
      </div>

      {/* Right-side vertical size slider. */}
      <div
        className="absolute right-5 top-1/2 flex -translate-y-1/2 flex-col items-center gap-2 select-none"
        onClick={(e) => e.stopPropagation()}
      >
        <span className="text-xs text-white/70">+</span>
        <div
          ref={trackRef}
          onPointerDown={onPointerDown}
          onPointerMove={onPointerMove}
          onPointerUp={stopDrag}
          onPointerCancel={stopDrag}
          className="relative h-[60vh] w-1.5 cursor-pointer touch-none rounded-full bg-white/25"
        >
          <div
            className="absolute left-1/2 h-6 w-4 -translate-x-1/2 -translate-y-1/2 rounded-full bg-white shadow"
            style={{ top: `${thumbPct}%` }}
          />
        </div>
        <span className="text-xs text-white/70">−</span>
        <button
          onClick={() => setScale(1)}
          className="mt-1 rounded bg-white/10 px-1.5 py-0.5 text-[10px] text-white/80 hover:bg-white/25"
          title="恢复适合宽度"
        >
          100%
        </button>
      </div>

      {alt && (
        <p className="pointer-events-none absolute bottom-4 left-1/2 max-w-[70vw] -translate-x-1/2 truncate text-center text-xs text-white/70">
          {alt}
        </p>
      )}
    </div>
  );
}
