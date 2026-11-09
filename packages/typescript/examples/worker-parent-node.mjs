// node node_modules/@openbindings/sdk/examples/worker-parent-node.mjs
import { Worker } from "node:worker_threads";

const worker = new Worker(new URL("./worker-node.mjs", import.meta.url), {
  workerData: { id: 0 },
});
try {
  const result = await new Promise((resolve, reject) => {
    worker.once("message", resolve);
    worker.once("error", reject);
    worker.once("exit", (code) =>
      reject(new Error(`Worker exited before replying (${code}).`)),
    );
  });
  console.log(JSON.stringify(result, null, 2));
} finally {
  await worker.terminate();
}
