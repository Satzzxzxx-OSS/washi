import { extensionOf } from "./paths";

const SOURCE_EXTENSIONS = ["typ", "tex", "latex"];

export interface PageClick {
  page: number;
  x: number;
  y: number;
}

export const hasJumpableSource = (path: string | null): path is string =>
  path !== null && SOURCE_EXTENSIONS.includes(extensionOf(path));

export function pageClick(
  wrapper: Pick<HTMLElement, "dataset" | "getBoundingClientRect">,
  clientX: number,
  clientY: number,
): PageClick | null {
  const page = Number(wrapper.dataset.page);
  const scale = Number(wrapper.dataset.scale);
  if (!page || !scale) return null;
  const rect = wrapper.getBoundingClientRect();
  return { page, x: (clientX - rect.left) / scale, y: (clientY - rect.top) / scale };
}
