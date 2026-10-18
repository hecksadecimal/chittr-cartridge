// Compatibility core for the non-Rust backends.
//
// The pinned core (237aa1) normalizes a bare `type: object` property (an "any
// JSON" value) to a `free-form-object` XtpNormalizedType. Only the pinned Rust
// backend knows that kind. Every other vendored backend predates it and instead
// predicates on an *empty* ObjectType -- they already render it as `any`
// (TypeScript), `interface{}` (Go), `dict` (Python), `JsonNode` (C#) and
// `jsoncons::json` (C++). So without this shim, those five backends throw
// "Can't convert XTP type ... free-form-object".
//
// This module re-exports the pinned core unchanged except for `getContext()`,
// which rewrites `free-form-object` types in the normalized schema into the
// empty-object shape those backends expect. It is used only when bundling the
// non-Rust backends; Rust keeps the pristine core so its output stays
// byte-identical to `xtp plugin init`.
//
// The vendored core is not modified.
import * as core from "../vendor/xtp-bindgen/src/index";

export * from "../vendor/xtp-bindgen/src/index";

function patchFreeFormObjects(node: unknown, seen: WeakSet<object>): void {
  if (!node || typeof node !== "object") return;
  if (seen.has(node)) return;
  seen.add(node);
  if (Array.isArray(node)) {
    for (const item of node) patchFreeFormObjects(item, seen);
    return;
  }
  const obj = node as Record<string, unknown>;
  if (obj.kind === "free-form-object") {
    obj.kind = "object";
    if (obj.name === undefined) obj.name = "";
    if (obj.properties === undefined) obj.properties = [];
  }
  for (const key of Object.keys(obj)) patchFreeFormObjects(obj[key], seen);
}

export function getContext() {
  const ctx = core.getContext();
  patchFreeFormObjects(ctx.schema, new WeakSet());
  return ctx;
}
