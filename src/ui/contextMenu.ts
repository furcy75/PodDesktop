/**
 * Menu contextuel desktop au clic droit (`contextmenu`).
 * Se positionne automatiquement dans les limites de la fenêtre et se ferme sur clic extérieur ou Échap.
 */

export interface ContextMenuItem {
  label?: string;
  danger?: boolean;
  separator?: boolean;
  onClick?: () => void;
}

let initialized = false;

export function showContextMenu(x: number, y: number, items: ContextMenuItem[]): void {
  const root = document.getElementById("context-menu");
  if (!root) return;

  if (!initialized) {
    document.addEventListener("click", hideContextMenu);
    document.addEventListener("keydown", (e) => {
      if (e.key === "Escape") hideContextMenu();
    });
    window.addEventListener("resize", hideContextMenu);
    initialized = true;
  }

  root.innerHTML = "";
  for (const item of items) {
    if (item.separator) {
      const sep = document.createElement("div");
      sep.className = "ctx-sep";
      root.appendChild(sep);
      continue;
    }
    const btn = document.createElement("button");
    btn.className = `ctx-item${item.danger ? " danger" : ""}`;
    btn.textContent = item.label || "";
    btn.addEventListener("click", (e) => {
      e.stopPropagation();
      hideContextMenu();
      item.onClick?.();
    });
    root.appendChild(btn);
  }

  root.classList.remove("hidden");
  const rect = root.getBoundingClientRect();
  const clampedX = Math.min(x, window.innerWidth - rect.width - 8);
  const clampedY = Math.min(y, window.innerHeight - rect.height - 8);
  root.style.left = `${Math.max(6, clampedX)}px`;
  root.style.top = `${Math.max(6, clampedY)}px`;
}

export function hideContextMenu(): void {
  const root = document.getElementById("context-menu");
  if (root) root.classList.add("hidden");
}
