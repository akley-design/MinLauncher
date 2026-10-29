interface Candidate {
  ratio: number;
  select: (selected: boolean) => void;
}
const candidates = new Map<symbol, Candidate>();

function elect() {
  let winner: symbol | undefined;
  let largest = 0;
  for (const [id, entry] of candidates) {
    if (entry.ratio > largest) {
      largest = entry.ratio;
      winner = id;
    }
  }
  for (const [id, entry] of candidates) entry.select(id === winner);
}

export function registerScene(select: Candidate["select"]) {
  const id = Symbol("diorama");
  candidates.set(id, { ratio: 0, select });
  return {
    update(ratio: number) {
      const entry = candidates.get(id);
      if (entry) {
        entry.ratio = ratio;
        elect();
      }
    },
    dispose() {
      candidates.delete(id);
      elect();
    },
  };
}
