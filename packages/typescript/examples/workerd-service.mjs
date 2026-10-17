// Local: wrangler dev --config examples/wrangler.jsonc --local
// Initialize inside fetch: workerd permits host entropy during a request.
import module from "@openbindings/sdk/openbindings.wasm";
import { initialize } from "@openbindings/sdk";
import { prepareService, ValidationService } from "./service.mjs";
let service;
export default {
  async fetch(request) {
    await initialize(module); // failed initialization remains retryable
    if (!service) {
      const setup = prepareService(
        '{"openbindings":"0.2.0","operations":{"lookup":{"input":{"type":"integer"}}}}',
      );
      if (setup.status !== "ready")
        return Response.json(setup, { status: 503 });
      service = new ValidationService(setup.contract);
    }
    if (request.method === "DELETE") {
      service.dispose();
      service = undefined;
      return new Response(null, { status: 204 });
    }
    return Response.json(
      await service.checkBytes(
        request.arrayBuffer().then((bytes) => new Uint8Array(bytes)),
      ),
    );
  },
};
