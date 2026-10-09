import { WorkerOwner } from "./worker-owner.mjs";
// Component-owned rendering policy: only the latest submitted edit may render.
export function createWorkerView(render, url) {
  let owner,
    generation = 0,
    closed = false;
  return {
    async check(document, value) {
      if (closed) throw new Error("View is closed.");
      const current = ++generation;
      owner ??= new WorkerOwner(url);
      const result = await owner.run({ document, value }, [document, value]);
      const accepted = current === generation;
      if (accepted) render(result);
      return { accepted, result };
    },
    cancel() {
      generation++;
      owner?.dispose();
      owner = undefined;
    },
    dispose() {
      this.cancel();
      closed = true;
    },
  };
}
