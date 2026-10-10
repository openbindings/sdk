import assert from "node:assert/strict";

// Own each page so an early loading failure also cancels pending page waits.
export async function firstUsePage(browser, url, configure = async () => {}) {
  const page = await browser.newPage();
  const errors = [];
  let rejectLoading;
  const loadingFailed = new Promise((_, reject) => {
    rejectLoading = reject;
  });
  const record = (message) => {
    errors.push(message);
    rejectLoading(new Error(`First-use example failed to load: ${message}`));
  };
  page.on("pageerror", (error) => record(error.message));
  page.on("requestfailed", (request) => record(request.url()));
  page.on("response", (response) => {
    if (response.status() >= 400)
      record(`${response.status()} ${response.url()}`);
  });
  try {
    await configure(page);
    const result = await Promise.race([
      (async () => {
        await page.goto(url);
        await page.waitForFunction(() => {
          const output = document.querySelector("#result");
          return output && output.dataset.state !== "loading";
        });
        const text = await page.locator("#result").textContent();
        assert.equal(
          await page.locator("#result").getAttribute("data-state"),
          "ready",
          `First-use example failed to load: ${text}`,
        );
        return JSON.parse(text);
      })(),
      loadingFailed,
    ]);
    assert.deepEqual(errors, []);
    assert.equal(result.accepted.result.outcome, "satisfies");
    assert.equal(result.accepted.operations[0].key, "lookup");
    assert.equal(result.fails.result.outcome, "fails");
    assert.equal(result.invalidInput.result.outcome, "input-error");
    return { firstUse: result, errors: [...errors] };
  } finally {
    await page.close();
  }
}

export async function firstUseLoadingFailures(browser, url) {
  const missingModule = new URL("../dist/index.js", url).href;
  const configure = (page) =>
    page.route(missingModule, (route) =>
      route.fulfill({ status: 404, contentType: "text/javascript", body: "" }),
    );
  const page = await browser.newPage();
  let missingResponses = 0;
  page.on("response", (response) => {
    if (response.url() === missingModule && response.status() === 404)
      missingResponses++;
  });
  let displayedError;
  try {
    await configure(page);
    await page.goto(url);
    await page.waitForFunction(
      () => document.querySelector("#result")?.dataset.state === "error",
    );
    displayedError = await page.locator("#result").textContent();
    assert(
      missingResponses > 0,
      "Negative control must actually miss the SDK module",
    );
    assert(displayedError.trim().length > 0);
    assert.notEqual(displayedError, "Loading…");
  } finally {
    await page.close();
  }
  // The positive harness must report this loading error, not a generic timeout.
  await assert.rejects(
    firstUsePage(browser, url, configure),
    (error) =>
      error.message.startsWith("First-use example failed to load:") &&
      error.message.includes(missingModule),
  );
  // A caught module-evaluation failure can have successful HTTP responses and
  // no pageerror. Its displayed cause must still reach the test failure.
  const exampleModule = new URL("./first-use.mjs", url).href;
  const evaluationError = "Controlled first-use module evaluation failure";
  let evaluationResponses = 0;
  await assert.rejects(
    firstUsePage(browser, url, async (page) => {
      page.on("response", (response) => {
        if (response.url() === exampleModule && response.status() === 200)
          evaluationResponses++;
      });
      await page.route(exampleModule, (route) =>
        route.fulfill({
          status: 200,
          contentType: "text/javascript",
          body: `throw new Error(${JSON.stringify(evaluationError)});`,
        }),
      );
    }),
    (error) => error.message.includes(evaluationError),
  );
  assert(
    evaluationResponses > 0,
    "Control must load the failing module with HTTP 200",
  );
  return {
    missingModule,
    missingResponses,
    displayedError,
    harnessRejectedLoadFailure: true,
    evaluationResponses,
    harnessReportedEvaluationFailure: true,
  };
}
