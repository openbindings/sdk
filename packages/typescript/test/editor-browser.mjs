import assert from "node:assert/strict";

// The URL may point at either workspace assets or an installed package root.
// Load the shipped page and module; all edits travel through their real DOM handlers.
export async function editorPage(browser, url) {
  const page = await browser.newPage();
  const errors = [];
  let rejectLoading;
  const loadingFailed = new Promise((_, reject) => {
    rejectLoading = reject;
  });
  const record = (message) => {
    errors.push(message);
    rejectLoading(new Error(`Editor example failed: ${message}`));
  };
  page.on("pageerror", (error) => record(error.message));
  page.on("requestfailed", (request) => record(request.url()));
  page.on("response", (response) => {
    if (response.status() >= 400)
      record(`${response.status()} ${response.url()}`);
  });
  page.on("console", (message) => {
    if (message.type() === "error") record(message.text());
  });
  const sdkUrl = new URL("../dist/index.js", url).href;
  const moduleUrl = new URL("./editor.mjs", url).href;
  const ownerCount = () =>
    page.evaluate(async (sdkUrl) => {
      const sdk = await import(sdkUrl);
      return sdk.liveStorageOwners();
    }, sdkUrl);
  const readOutput = () => page.locator("output").textContent();
  let unmount;
  try {
    const result = await Promise.race([
      (async () => {
        await page.goto(url);
        await page.waitForFunction(
          () => document.querySelector("output")?.textContent.trim().length > 0,
        );
        const source = page.locator("textarea");
        const originalSource = await source.inputValue();
        const initial = await readOutput();
        assert.match(initial, /Document conformance: non-conformant/);
        assert(initial.includes('pointer "/operations/lookup/inputSchema"'));
        assert.match(initial, /UTF-8 byte column \d+, byte offset \d+/);
        assert(
          initial.includes(
            "this member is not permitted here; extension member names begin with x-",
          ),
        );

        await page.locator('[data-action="correct"]').click();
        const correctedSource = await source.inputValue();
        assert.equal(
          correctedSource,
          originalSource.replace('"inputSchema":', '"input":'),
        );
        const corrected = await readOutput();
        assert.match(
          corrected,
          /Document is conformant; the input contract is ready/,
        );
        assert.match(corrected, /Example input \{ id: 7 \} satisfies/);
        const active = await ownerCount();

        // An arbitrary edited draft must render actual refusal data, including an
        // escaped/control/markup-bearing pointer, without inserting markup nodes.
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
        await page.locator('button[type="submit"]').click();
        const refused = await readOutput();
        const pointer = `/operations/lookup/${key.replaceAll("~", "~0").replaceAll("/", "~1")}`;
        assert.match(refused, /Document conformance: non-conformant/);
        assert(refused.includes(JSON.stringify(pointer)));
        assert(!refused.includes("\u001b"));
        assert.equal(await page.locator("output img").count(), 0);
        assert.equal(await source.inputValue(), invalidSource);
        const afterRefusal = await ownerCount();
        assert.equal(
          afterRefusal,
          active,
          "Refused draft must release temporary owners",
        );

        // Exercise the HTML page's registered pagehide handler without destroying
        // its realm, so released storage and disconnected listeners remain observable.
        await page.evaluate(() => dispatchEvent(new Event("pagehide")));
        const warmed = await ownerCount();
        assert(
          warmed < active,
          "Pagehide must release the editor's retained snapshot",
        );
        const afterPagehide = await removedListeners(page, originalSource);
        assert.equal(await ownerCount(), warmed);

        // A complete second mount provides a warmed cleanup baseline and exercises
        // the module's returned unmount callback independently of the pagehide wiring.
        unmount = await page.evaluateHandle(async (moduleUrl) => {
          const { mountEditor } = await import(moduleUrl);
          return mountEditor(document.querySelector("main"));
        }, moduleUrl);
        assert.match(
          await readOutput(),
          /Document conformance: non-conformant/,
        );
        await page.locator('[data-action="correct"]').click();
        assert.match(await readOutput(), /Example input \{ id: 7 \} satisfies/);
        const remountedActive = await ownerCount();
        assert.equal(remountedActive, active);
        await unmount.evaluate((dispose) => dispose());
        await unmount.dispose();
        unmount = undefined;
        const released = await ownerCount();
        assert.equal(released, warmed);
        const afterUnmount = await removedListeners(page, originalSource);
        assert.equal(await ownerCount(), warmed);
        return {
          initial,
          corrected,
          refused,
          originalSource,
          correctedSource,
          renderedPointer: pointer,
          markupElementsCreated: 0,
          arenas: { active, afterRefusal, warmed, remountedActive, released },
          removedListeners: { pagehide: afterPagehide, unmount: afterUnmount },
        };
      })(),
      loadingFailed,
    ]);
    assert.deepEqual(errors, []);
    return { editor: result, errors: [...errors] };
  } finally {
    if (unmount) await unmount.dispose();
    await page.close();
  }
}

async function removedListeners(page, draft) {
  const result = await page.evaluate((draft) => {
    const source = document.querySelector("textarea");
    const output = document.querySelector("output");
    source.value = draft;
    output.textContent = "after editor teardown";
    // Synthetic submit dispatch checks the listener without native form navigation.
    const submitUnclaimed = document
      .querySelector("form")
      .dispatchEvent(new Event("submit", { bubbles: true, cancelable: true }));
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
