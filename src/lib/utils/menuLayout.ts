export type PanelSpan = { top: number; bottom: number };

export type InputRect = { x: number; y: number; width: number; height: number };

export function menuWindowHeight(closedHeight: number, listBlockHeight: number): number {
  return closedHeight + listBlockHeight;
}

export function closedMenuHeight(menuHeight: number, listOpen: boolean, listBlockHeight: number): number {
  return menuHeight - (listOpen ? listBlockHeight : 0);
}

export function closedOffset(anchorY: number, windowY: number, listBlockHeight: number): number {
  return Math.min(Math.max(anchorY - windowY, 0), listBlockHeight);
}

export function offsetAfterToggle(offset: number, listOpen: boolean): number {
  return listOpen ? 0 : offset;
}

export function menuInputRect(
  menuTop: number,
  menuHeight: number,
  windowWidth: number,
  panels: PanelSpan[],
): InputRect {
  // One bounding rectangle: clicks in the transparent rounded corners and in the gap between the menu
  // and a submenu stay with the menu window. Stops being correct when a submenu can open apart from
  // the menu (the rectangle would then cover the transparent space between them); upgrade path: one
  // rectangle per element.
  const y = Math.min(menuTop, ...panels.map((p) => p.top));
  const bottom = Math.max(menuTop + menuHeight, ...panels.map((p) => p.bottom));
  return { x: 0, y, width: windowWidth, height: bottom - y };
}
