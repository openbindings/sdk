// Every worker initializes its own SDK module instance and keeps its owners here.
import { parentPort, workerData } from "node:worker_threads";
import { initialize } from "@openbindings/sdk";
import { firstUse } from "./first-use.mjs";

await initialize();
// firstUse releases its owners and returns only plain data.
parentPort.postMessage(firstUse(workerData));
