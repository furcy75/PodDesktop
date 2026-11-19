/**
 * Moteur de Virtual Scrolling DOM-recyclé natif (sans bibliothèque externe).
 *
 * Permet d'afficher instantanément plus de 50 000 épisodes à 60 FPS en ne conservant
 * dans le DOM que les lignes visibles (+ un tampon `overscan` de 8 lignes au-dessus/dessous).
 */

export class VirtualList<T> {
  private viewport: HTMLElement;
  private spacer: HTMLElement;
  private items: T[] = [];
  private rowHeight: number;
  private overscan = 8;
  private renderRow: (item: T, index: number) => HTMLElement;
  private rafId: number | null = null;
  private lastStart = -1;
  private lastEnd = -1;

  constructor(
    viewport: HTMLElement,
    rowHeight: number,
    renderRow: (item: T, index: number) => HTMLElement
  ) {
    this.viewport = viewport;
    this.rowHeight = rowHeight;
    this.renderRow = renderRow;

    this.spacer = document.createElement("div");
    this.spacer.className = "virtual-spacer";
    this.viewport.innerHTML = "";
    this.viewport.appendChild(this.spacer);

    this.viewport.addEventListener("scroll", () => this.scheduleRender(), { passive: true });
  }

  public setRowHeight(height: number): void {
    this.rowHeight = height;
    this.forceRender();
  }

  public setItems(items: T[]): void {
    this.items = items;
    this.spacer.style.height = `${items.length * this.rowHeight}px`;
    this.forceRender();
  }

  public forceRender(): void {
    this.lastStart = -1;
    this.lastEnd = -1;
    this.renderVisible();
  }

  private scheduleRender(): void {
    if (this.rafId !== null) return;
    this.rafId = requestAnimationFrame(() => {
      this.rafId = null;
      this.renderVisible();
    });
  }

  private renderVisible(): void {
    const scrollTop = this.viewport.scrollTop;
    const viewHeight = this.viewport.clientHeight || 600;
    const start = Math.max(0, Math.floor(scrollTop / this.rowHeight) - this.overscan);
    const end = Math.min(
      this.items.length,
      Math.ceil((scrollTop + viewHeight) / this.rowHeight) + this.overscan
    );

    if (start === this.lastStart && end === this.lastEnd) return;
    this.lastStart = start;
    this.lastEnd = end;

    const frag = document.createDocumentFragment();
    for (let i = start; i < end; i++) {
      const el = this.renderRow(this.items[i], i);
      el.style.top = `${i * this.rowHeight}px`;
      frag.appendChild(el);
    }
    this.spacer.innerHTML = "";
    this.spacer.appendChild(frag);
  }
}
