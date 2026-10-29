export class ResourceRegistry {
  private items = new Set<{ dispose: () => void }>();

  add<T extends { dispose: () => void }>(item: T): T {
    this.items.add(item);
    return item;
  }

  addCleanup(cleanup: () => void) {
    this.items.add({ dispose: cleanup });
  }

  dispose() {
    for (const item of [...this.items].reverse()) {
      try {
        item.dispose();
      } catch (error) {
        console.warn("Site diorama cleanup failed", error);
      }
    }
    this.items.clear();
  }
}
