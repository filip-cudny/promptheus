import { handleListNavKey } from "$lib/utils/listNavigation";

export function useListSelection(options: {
  count: () => number;
  active: () => boolean;
  onEscape: () => void;
  onEnter: (index: number) => void;
  stopPropagation?: boolean;
}) {
  let index = $state(0);
  const itemEls: (HTMLElement | null)[] = $state([]);

  $effect(() => {
    const count = options.count();
    if (index >= count) index = Math.max(0, count - 1);
  });

  $effect(() => {
    if (!options.active()) return;
    itemEls[index]?.scrollIntoView({ block: "nearest" });
  });

  function consume(e: KeyboardEvent) {
    e.preventDefault();
    if (options.stopPropagation) e.stopPropagation();
  }

  function handleKeydown(e: KeyboardEvent): boolean {
    if (e.key === "Escape") {
      consume(e);
      options.onEscape();
      return true;
    }
    if (e.key === "Enter") {
      consume(e);
      options.onEnter(index);
      return true;
    }
    const next = handleListNavKey(e, index, options.count());
    if (next === null) return false;
    consume(e);
    index = next;
    return true;
  }

  return {
    get index() {
      return index;
    },
    set index(value: number) {
      index = value;
    },
    itemEls,
    handleKeydown,
  };
}
