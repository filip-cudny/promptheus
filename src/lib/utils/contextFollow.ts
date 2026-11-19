export interface ContextImage {
  data: string;
  media_type: string;
  number?: number | null;
}

export interface ContextValue {
  text: string;
  images: ContextImage[];
}

export interface TabContext {
  context_text: string;
  context_images: ContextImage[];
  context_own: boolean;
  context_hint: boolean;
}

export function contextDiffers(a: ContextValue, b: ContextValue): boolean {
  if (a.text !== b.text || a.images.length !== b.images.length) return true;
  return a.images.some((image, i) => {
    const other = b.images[i];
    return (
      image.data !== other.data ||
      image.media_type !== other.media_type ||
      (image.number ?? null) !== (other.number ?? null)
    );
  });
}

export function followGlobal(tab: TabContext, global: ContextValue): void {
  tab.context_text = global.text;
  tab.context_images = global.images.map((image) => ({ ...image }));
  tab.context_own = false;
  tab.context_hint = false;
}

export function applyGlobalChange(tabs: TabContext[], global: ContextValue): void {
  for (const tab of tabs) {
    if (tab.context_own) {
      tab.context_hint = contextDiffers({ text: tab.context_text, images: tab.context_images }, global);
    } else {
      followGlobal(tab, global);
    }
  }
}

export function syncFollowingTabs(tabs: TabContext[], global: ContextValue): void {
  for (const tab of tabs) {
    if (!tab.context_own) followGlobal(tab, global);
  }
}

export function editContext(tab: TabContext, value: ContextValue): void {
  tab.context_text = value.text;
  tab.context_images = value.images;
  tab.context_own = true;
}

export function keepOwnVersion(tab: TabContext): void {
  tab.context_hint = false;
}

export function restoreOwnVersion(tab: TabContext, value: ContextValue, global: ContextValue): void {
  tab.context_text = value.text;
  tab.context_images = value.images;
  tab.context_own = true;
  tab.context_hint = contextDiffers(value, global);
}

export function acceptsGlobalResult(lastApplied: number, version: number): boolean {
  return version >= lastApplied;
}
