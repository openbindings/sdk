import { WorkerOwner } from "./worker-owner.mjs";
// Component-owned rendering policy: only the latest submitted edit may render.
export function createWorkerView(render, url) {
  let owner,
    generation = 0,
    closed = false;
  return {
    async check(document, value, options = {}) {
      if (closed) throw new Error("View is closed.");
      const current = ++generation;
      owner ??= new WorkerOwner(url);
      const transfer = [document, value].filter(
        (value) => value instanceof ArrayBuffer,
      );
      const result = await owner.run({ ...options, document, value }, transfer);
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
