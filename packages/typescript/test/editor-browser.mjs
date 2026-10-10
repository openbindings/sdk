import assert from "node:assert/strict";

// Load the delivered page; all edits use its DOM handlers and actual module Worker.
export async function editorPage(browser, url) {
  const page = await browser.newPage();
  const errors = [];
  page.on("pageerror", (e) => errors.push(e.message));
  page.on("console", (e) => {
    if (e.type() === "error") errors.push(e.text());
  });
  const readOutput = async () => {
    await page.waitForFunction(
      () => document.querySelector("output")?.dataset.pending === "false",
    );
    return page.locator("output").textContent();
  };
  const owners = async () =>
    Number(await page.locator("output").getAttribute("data-arenas"));
  let unmount;
  try {
    await page.goto(url);
    const source = page.locator("#source"),
      input = page.locator("#input");
    const originalSource = await source.inputValue();
    const initial = await readOutput();
    assert.match(initial, /Document conformance: non-conformant/);
    assert(initial.includes('pointer "/operations/lookup/inputSchema"'));
    assert.match(initial, /UTF-8 byte column \d+, byte offset \d+/);
    assert.equal(page.workers().length, 1);
    const mainInitialization = await page.evaluate(async (sdkUrl) => {
      const sdk = await import(sdkUrl);
      try {
        sdk.liveStorageOwners();
        return "initialized";
      } catch (e) {
        return e.code;
      }
    }, new URL("../dist/index.js", url).href);
    assert.equal(
      mainInitialization,
      "not-initialized",
      "CPU engine runs only in Worker realm",
    );

    const unicodeSource = originalSource.replace(
      "Find an item",
      "é💡 Find an item",
    );
    await source.fill(unicodeSource);
    await readOutput();
    await page.locator('[data-action="locate"]').click();
    const selection = await source.evaluate((el) => ({
      start: el.selectionStart,
      end: el.selectionEnd,
    }));
    assert.deepEqual(selection, {
      start: unicodeSource.indexOf('"inputSchema"'),
      end: unicodeSource.indexOf('"inputSchema"'),
    });

    await page.locator('[data-action="correct"]').click();
    const corrected = await readOutput();
    const correctedSource = await source.inputValue();
    assert.equal(
      correctedSource,
      unicodeSource.replace('"inputSchema":', '"input":'),
    );
    assert.match(
      corrected,
      /Document is conformant; the input contract is ready/,
    );
    assert.match(corrected, /Selected input satisfies/);
    const active = await owners();

    await input.fill('{"id":0}');
    const privateFailure = await readOutput();
    assert.match(privateFailure, /inclusive lower bound/);
    assert(!privateFailure.includes("Exact bound:"));
    await page.locator("#schema-details").check();
    const detailed = await readOutput();
    assert.match(detailed, /Exact bound: 1/);
    await page.locator("#schema-details").uncheck();
    await readOutput();
    await input.fill("{");
    assert.match(await readOutput(), /Selected input was not parsed/);
    await input.fill('{"id":7}');
    await readOutput();

    const key = 'bad/~\n\u001b<img data-editor-injected src="x">';
    const invalidSource = JSON.stringify({
      openbindings: "0.2.0",
      operations: {
        lookup: {
          aliases: ["find"],
          description: 42,
          input: { type: "integer" },
          [key]: false,
        },
      },
    });
    await source.fill(invalidSource);
    const refused = await readOutput();
    const pointer = `/operations/lookup/${key.replaceAll("~", "~0").replaceAll("/", "~1")}`;
    assert.match(refused, /Document conformance: non-conformant/);
    assert(refused.includes(JSON.stringify(pointer)));
    assert(!refused.includes("\u001b"));
    assert.equal(await page.locator("output img").count(), 0);
    const afterRefusal = await owners();
    assert.equal(
      afterRefusal,
      active,
      "failed edit releases temporaries and retains last proof",
    );

    // Queue two edits in one main-thread task. Only the latest source may render.
    await page.evaluate(
      ({ invalidSource, correctedSource }) => {
        const source = document.querySelector("#source");
        for (const text of [invalidSource, correctedSource]) {
          source.value = text;
          source.dispatchEvent(new Event("input", { bubbles: true }));
        }
      },
      { invalidSource, correctedSource },
    );
    assert.match(await readOutput(), /Selected input satisfies/);
    assert.equal(await source.inputValue(), correctedSource);

    await page.evaluate(() => dispatchEvent(new Event("pagehide")));
    await page.waitForFunction(
      () => document.querySelector("output").dataset.pending !== "true",
    );
    // Terminating the owned Worker releases its entire engine and snapshot realm.
    await page.waitForTimeout(20);
    assert.equal(page.workers().length, 0);
    const pagehide = await removedListeners(page, originalSource);
    unmount = await page.evaluateHandle(async (moduleUrl) => {
      const { mountEditor } = await import(moduleUrl);
      return mountEditor(document.querySelector("main"));
    }, new URL("./editor.mjs", url).href);
    assert.match(await readOutput(), /Document conformance: non-conformant/);
    await page.locator('[data-action="correct"]').click();
    assert.match(await readOutput(), /Selected input satisfies/);
    const remountedActive = await owners();
    assert.equal(remountedActive, active);
    await unmount.evaluate((dispose) => dispose());
    await unmount.dispose();
    unmount = undefined;
    await page.waitForTimeout(20);
    assert.equal(page.workers().length, 0);
    const afterUnmount = await removedListeners(page, originalSource);
    assert.deepEqual(errors, []);
    return {
      editor: {
        initial,
        corrected,
        refused,
        detailed,
        unicodeSource,
        selection,
        mainInitialization,
        actualWorker: true,
        latestEditRendered: true,
        arenas: { active, afterRefusal, remountedActive, finalWorkerRealms: 0 },
        removedListeners: { pagehide, unmount: afterUnmount },
      },
      errors,
    };
  } finally {
    if (unmount) await unmount.dispose();
    await page.close();
  }
}
async function removedListeners(page, draft) {
  const result = await page.evaluate((draft) => {
    const source = document.querySelector("#source"),
      output = document.querySelector("output");
    source.value = draft;
    output.textContent = "after editor teardown";
    const submitUnclaimed = document
      .querySelector("form")
      .dispatchEvent(new Event("submit", { bubbles: true, cancelable: true }));
    source.dispatchEvent(new Event("input", { bubbles: true }));
    document.querySelector('[data-action="correct"]').click();
    return {
      submitUnclaimed,
      sourceUnchanged: source.value === draft,
      outputUnchanged: output.textContent === "after editor teardown",
    };
  }, draft);
  assert.deepEqual(result, {
    submitUnclaimed: true,
    sourceUnchanged: true,
    outputUnchanged: true,
  });
  return result;
}
