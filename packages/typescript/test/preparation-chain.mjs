// Same deterministic pointer-reference chain as the 2026-11-14 Workers lab.
export function chainDocument(links) {
  const parts = [];
  for (let i = 0; i < links; i++)
    parts.push(`"d${i}":{"$ref":"#/schemas/d${i + 1}"}`);
  parts.push(`"d${links}":{"type":"integer"}`);
  return `{"openbindings":"0.2.0","schemas":{${parts.join(",")}},"operations":{"op":{"input":{"$ref":"#/schemas/d0"}}}}`;
}
