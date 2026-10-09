// Application scheduling policy, not an SDK runtime. Bundlers resolve the Worker's imports.
export class WorkerOwner {
  #worker;
  #next = 0;
  #pending = new Map();
  constructor(url = new URL("./worker.mjs", import.meta.url)) {
    this.#worker = new Worker(url, { type: "module" });
    this.#worker.onmessage = ({ data }) => {
      if (data.status === "started") return;
      const pending = this.#pending.get(data.id);
      if (!pending) return; // stale job/owner result
      this.#pending.delete(data.id);
      pending.resolve(data);
    };
    this.#worker.onerror = (event) => this.dispose(new Error(event.message));
  }
  run(job, transfer = []) {
    if (!this.#worker)
      return Promise.reject(new Error("Worker owner is closed."));
    const id = ++this.#next;
    return new Promise((resolve, reject) => {
      this.#pending.set(id, { resolve, reject });
      try {
        this.#worker.postMessage({ ...job, id }, transfer);
      } catch (error) {
        this.#pending.delete(id);
        reject(error);
      }
    });
  }
  dispose(
    reason = new DOMException("Worker terminated by its owner.", "AbortError"),
  ) {
    this.#worker?.terminate();
    this.#worker = undefined;
    for (const pending of this.#pending.values()) pending.reject(reason);
    this.#pending.clear();
  }
}
// await owner.run({kind:'initialize'}); // initialized-worker boundary
// Transfer dedicated document/value ArrayBuffers; do not transfer SDK-owned views.
// const result = await owner.run({document, value}, [document, value]);
// Termination is the interruption boundary. Create a new WorkerOwner to retry.
