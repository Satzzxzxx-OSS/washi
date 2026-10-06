import { EDITOR_WIDTH_RANGE } from "../prefs";

/** 分割表示で、エディタが占める割合（%）を、許す範囲に収める */
export function clampPercent(percent: number): number {
  if (!Number.isFinite(percent)) return 50;
  return Math.min(EDITOR_WIDTH_RANGE.max, Math.max(EDITOR_WIDTH_RANGE.min, percent));
}

/** つまみをドラッグした位置から、エディタの幅の割合（%）を求める。`paneLeft` は、エディタの左端 */
export function percentFromPointer(clientX: number, paneLeft: number, containerWidth: number): number {
  if (containerWidth <= 0) return 50;
  return clampPercent(((clientX - paneLeft) / containerWidth) * 100);
}

/** 矢印キーで動かす幅（%） */
export const KEY_STEP_PERCENT = 2;

export function applyEditorWidth(pane: HTMLElement, percent: number) {
  pane.style.setProperty("flex-basis", `${clampPercent(percent)}%`);
}

/** エディタとプレビューの間のつまみ。ドラッグと矢印キーで、幅を変える */
export function attachResizer(
  handle: HTMLElement,
  pane: HTMLElement,
  current: () => number,
  onChange: (percent: number) => void,
) {
  handle.addEventListener("pointerdown", (event) => {
    event.preventDefault();
    handle.setPointerCapture(event.pointerId);
    const move = (e: PointerEvent) => {
      const container = pane.parentElement?.clientWidth ?? window.innerWidth;
      onChange(percentFromPointer(e.clientX, pane.getBoundingClientRect().left, container));
    };
    const stop = () => {
      handle.removeEventListener("pointermove", move);
      handle.removeEventListener("pointerup", stop);
      handle.removeEventListener("pointercancel", stop);
    };
    handle.addEventListener("pointermove", move);
    handle.addEventListener("pointerup", stop);
    handle.addEventListener("pointercancel", stop);
  });
  handle.addEventListener("keydown", (event) => {
    const step = event.key === "ArrowLeft" ? -KEY_STEP_PERCENT : event.key === "ArrowRight" ? KEY_STEP_PERCENT : 0;
    if (!step) return;
    event.preventDefault();
    onChange(clampPercent(current() + step));
  });
}
