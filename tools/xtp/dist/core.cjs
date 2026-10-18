"use strict";
var __defProp = Object.defineProperty;
var __getOwnPropDesc = Object.getOwnPropertyDescriptor;
var __getOwnPropNames = Object.getOwnPropertyNames;
var __hasOwnProp = Object.prototype.hasOwnProperty;
var __export = (target, all) => {
  for (var name in all)
    __defProp(target, name, { get: all[name], enumerable: true });
};
var __copyProps = (to, from, except, desc) => {
  if (from && typeof from === "object" || typeof from === "function") {
    for (let key of __getOwnPropNames(from))
      if (!__hasOwnProp.call(to, key) && key !== except)
        __defProp(to, key, { get: () => from[key], enumerable: !(desc = __getOwnPropDesc(from, key)) || desc.enumerable });
  }
  return to;
};
var __toCommonJS = (mod) => __copyProps(__defProp({}, "__esModule", { value: true }), mod);

// tools/xtp/vendor/xtp-bindgen/src/index.ts
var index_exports = {};
__export(index_exports, {
  ArrayType: () => ArrayType,
  BooleanType: () => BooleanType,
  BufferType: () => BufferType,
  DateTimeType: () => DateTimeType,
  DoubleType: () => DoubleType,
  EnumType: () => EnumType,
  FloatType: () => FloatType,
  FreeFormObjectType: () => FreeFormObjectType,
  Int16Type: () => Int16Type,
  Int32Type: () => Int32Type,
  Int64Type: () => Int64Type,
  Int8Type: () => Int8Type,
  MapType: () => MapType,
  NormalizeError: () => NormalizeError,
  ObjectType: () => ObjectType,
  StringType: () => StringType,
  UInt16Type: () => UInt16Type,
  UInt32Type: () => UInt32Type,
  UInt64Type: () => UInt64Type,
  UInt8Type: () => UInt8Type,
  ValidationError: () => ValidationError,
  getContext: () => getContext,
  helpers: () => helpers,
  isExport: () => isExport,
  isProperty: () => isProperty,
  parse: () => parse,
  parseAndNormalizeJson: () => parseAndNormalizeJson
});
module.exports = __toCommonJS(index_exports);

// tools/xtp/vendor/xtp-bindgen/src/common.ts
var ValidationError = class {
  constructor(message, path) {
    this.message = message;
    this.path = path;
    this.message = message;
    this.path = path;
  }
};
if (!Array.prototype.none) {
  Array.prototype.none = function(predicate) {
    return !this.some(predicate);
  };
}

// tools/xtp/vendor/xtp-bindgen/src/keywords.ts
var KEYWORDS = {
  python: /* @__PURE__ */ new Set(["false", "none", "true", "and", "as", "assert", "async", "await", "break", "class", "continue", "def", "del", "elif", "else", "except", "finally", "for", "from", "global", "if", "import", "in", "is", "lambda", "nonlocal", "not", "or", "pass", "raise", "return", "try", "while", "with", "yield"]),
  go: /* @__PURE__ */ new Set(["break", "default", "func", "interface", "select", "case", "defer", "go", "map", "struct", "chan", "else", "goto", "package", "switch", "const", "fallthrough", "if", "range", "type", "continue", "for", "import", "return", "var"]),
  csharp: /* @__PURE__ */ new Set(["abstract", "as", "base", "bool", "break", "byte", "case", "catch", "char", "checked", "class", "const", "continue", "decimal", "default", "delegate", "do", "double", "else", "enum", "event", "explicit", "extern", "false", "finally", "fixed", "float", "for", "foreach", "goto", "if", "implicit", "in", "int", "interface", "internal", "is", "lock", "long", "namespace", "new", "null", "object", "operator", "out", "override", "params", "private", "protected", "public", "readonly", "ref", "return", "sbyte", "sealed", "short", "sizeof", "stackalloc", "static", "string", "struct", "switch", "this", "throw", "true", "try", "typeof", "uint", "ulong", "unchecked", "unsafe", "ushort", "using", "virtual", "void", "volatile", "while"]),
  rust: /* @__PURE__ */ new Set(["as", "break", "const", "continue", "crate", "else", "enum", "extern", "false", "fn", "for", "if", "impl", "in", "let", "loop", "match", "mod", "move", "mut", "pub", "ref", "return", "self", "static", "struct", "super", "trait", "true", "type", "unsafe", "use", "where", "while", "async", "await", "dyn", "abstract", "become", "box", "do", "final", "macro", "override", "priv", "try", "typeof", "unsized", "virtual", "yield"]),
  cpp: /* @__PURE__ */ new Set(["alignas", "alignof", "and", "and_eq", "asm", "auto", "bitand", "bitor", "bool", "break", "case", "catch", "char", "char8_t", "char16_t", "char32_t", "class", "compl", "concept", "const", "consteval", "constexpr", "constinit", "const_cast", "continue", "co_await", "co_return", "co_yield", "decltype", "default", "delete", "do", "double", "dynamic_cast", "else", "enum", "explicit", "export", "extern", "false", "float", "for", "friend", "goto", "if", "inline", "int", "long", "mutable", "namespace", "new", "noexcept", "not", "not_eq", "nullptr", "operator", "or", "or_eq", "private", "protected", "public", "register", "reinterpret_cast", "requires", "return", "short", "signed", "sizeof", "static", "static_assert", "static_cast", "struct", "switch", "template", "this", "thread_local", "throw", "true", "try", "typedef", "typeid", "typename", "union", "unsigned", "using", "virtual", "void", "volatile", "wchar_t", "while", "xor", "xor_eq"]),
  zig: /* @__PURE__ */ new Set(["addrspace", "align", "allowzero", "and", "anyframe", "anytype", "asm", "async", "await", "break", "callconv", "catch", "comptime", "const", "continue", "defer", "else", "enum", "errdefer", "error", "export", "extern", "fn", "for", "if", "inline", "linksection", "noalias", "noinline", "nosuspend", "opaque", "or", "orelse", "packed", "pub", "resume", "return", "struct", "suspend", "switch", "test", "threadlocal", "try", "union", "unreachable", "usingnamespace", "var", "volatile", "while"])
};
function checkForKeyword(name) {
  const normalizedName = name.toLowerCase();
  const langs = [];
  for (const lang in KEYWORDS) {
    if (KEYWORDS[lang].has(normalizedName)) {
      langs.push(lang);
    }
  }
  return langs.length > 0 ? langs : null;
}

// tools/xtp/vendor/xtp-bindgen/src/parser.ts
function parseAny(doc) {
  doc.errors = [];
  doc.warnings = [];
  switch (doc.version) {
    case "v0":
      const v0Doc = doc;
      return v0Doc;
    case "v1-draft":
      const v1Doc = doc;
      const validator = new V1Validator(v1Doc);
      validator.validate();
      return v1Doc;
    default:
      doc.errors.push(
        new ValidationError(`version property not valid: ${doc.version}`, "#/version")
      );
      return doc;
  }
}
function isV0Schema(schema) {
  return schema?.version === "v0";
}
function isV1Schema(schema) {
  return schema?.version === "v1-draft";
}
var V1Validator = class {
  constructor(doc) {
    this.location = ["#"];
    this.doc = doc;
  }
  /**
   * Validate the document and sets any errors or warnings on the document
   */
  validate() {
    this.doc.errors = [];
    this.doc.warnings = [];
    this.validateRootNames();
    this.validateNode(this.doc);
  }
  /**
   * Recursively walk through the untyped document and validate each node.
   * This saves us a lot of code but might be a little bit slower.
   */
  validateNode(node) {
    const currentPath = this.getLocation();
    const skipPatterns = [
      /^#\/components\/schemas\/[^/]+\/properties$/,
      // allow defining properties named `type` or `format`
      /^#\/components\/schemas$/,
      // allow defining schemas named `type` or `format`
      /^#\/components$/,
      // reserved for future use
      /^#\/exports$/,
      // allow defining exports named `type` or `format`
      /^#\/imports$/
      // allow defining imports named `type` or `format`
    ];
    const shouldValidate = skipPatterns.none((pattern) => pattern.test(currentPath));
    if (shouldValidate) {
      this.validateTypedInterface(node);
    }
    if (node && typeof node === "object") {
      if (Array.isArray(node)) return;
      for (const key in node) {
        if (Object.prototype.hasOwnProperty.call(node, key)) {
          const child = node[key];
          if (typeof child === "object") {
            this.location.push(key);
            this.validateNode(child);
            this.location.pop();
          }
        }
      }
    }
  }
  /**
   * Validates the root names of the doc.
   * We just do this by hand as it's tricky to do recursively
   */
  validateRootNames() {
    const exports2 = this.doc.exports || {};
    for (const n in exports2) {
      this.validateIdentifier(n, ["exports", n]);
    }
    const imports = this.doc.imports || {};
    for (const n in imports) {
      this.validateIdentifier(n, ["imports", n]);
    }
    const schemas = this.doc.components?.schemas || {};
    for (const n in schemas) {
      this.validateIdentifier(n, ["components", "schemas", n]);
    }
  }
  recordError(msg, suffix) {
    const path = this.getLocation(suffix);
    this.doc.errors.push(
      new ValidationError(msg, path)
    );
  }
  recordWarning(msg, suffix) {
    const path = this.getLocation(suffix);
    this.doc.warnings.push(
      new ValidationError(msg, path)
    );
  }
  /**
   * Validates that a node conforms to the rules of
   * the XtpTyped interface. These validations catch a lot of
   * what we can't catch in JSON Schema validation.
   */
  validateTypedInterface(prop) {
    if (!prop) return;
    const validTypes = ["string", "number", "integer", "boolean", "object", "array", "buffer"];
    if (prop.type && !validTypes.includes(prop.type)) {
      this.recordError(`Invalid type '${stringify(prop.type)}'. Options are: [${validTypes.map((t) => `'${t}'`).join(", ")}]`);
    }
    if (prop.format) {
      let validFormats = [];
      if (prop.type === "string") {
        validFormats = ["date-time", "byte"];
      } else if (prop.type === "number") {
        validFormats = ["float", "double"];
      } else if (prop.type === "integer") {
        validFormats = [
          "uint8",
          "int8",
          "uint16",
          "int16",
          "uint32",
          "int32",
          "uint64",
          "int64"
        ];
      }
      if (!validFormats.includes(prop.format)) {
        this.recordError(`Invalid format ${stringify(prop.format)} for type ${stringify(prop.type)}. Valid formats are: [${validFormats.join(", ")}]`);
      }
    }
    if ("properties" in prop && Object.keys(prop.properties).length > 0) {
      const schema = prop;
      if (schema.additionalProperties) {
        this.recordError("We currently do not support objects with both fixed properties and additionalProperties");
      }
      if (schema.required) {
        for (const name of schema.required) {
          if (!schema.properties?.[name]) {
            this.recordError(`Property ${name} is marked as required but not defined`);
          }
        }
      }
      for (const name in schema.properties) {
        this.validateIdentifier(name, ["properties", name]);
      }
    }
    if (prop.enum) {
      for (const item of prop.enum) {
        if (typeof item !== "string") {
          this.recordError(`Enum item must be a string: ${item}`);
        }
        this.validateIdentifier(item, ["enum"]);
      }
    }
    if (prop.items) this.validateTypedInterface(prop.items);
    if (prop.additionalProperties) {
      if (prop.type !== "object") {
        this.recordError(`The parent type must be 'object' when using additionalProperties but your type is ${prop.type}`);
      }
      this.validateTypedInterface(prop.additionalProperties);
    }
    if (prop.$ref) {
      const parts = prop.$ref.split("/");
      if (parts[0] === "#" && parts[1] === "components" && parts[2] === "schemas") {
        const name = parts[3];
        const schemas = this.doc.components?.schemas || {};
        const s = schemas[name];
        if (!s) {
          const availableSchemas = Object.keys(schemas).join(", ");
          this.recordError(`Invalid $ref "${prop.$ref}". Cannot find schema "${name}". Options are: [${availableSchemas}]`, ["$ref"]);
        }
      } else {
        this.recordError(`Invalid $ref "${prop.$ref}"`, ["$ref"]);
      }
    }
  }
  validateIdentifier(name, path) {
    if (!/^[a-zA-Z_$][a-zA-Z0-9_$]*$/.test(name)) {
      this.recordError(`Invalid identifier: "${name}". Must match /^[a-zA-Z_$][a-zA-Z0-9_$]*$/`, path);
    }
    const langs = checkForKeyword(name);
    if (langs) {
      this.recordWarning(`Potentially Invalid identifier: "${name}". This is a keyword in the following languages and may cause trouble with code generation: ${langs.join(",")}`, path);
    }
  }
  getLocation(suffix = []) {
    return this.location.concat(suffix).join("/");
  }
};
function stringify(typ) {
  if (typeof typ === "object") {
    return JSON.stringify(typ);
  }
  return `${typ}`;
}

// tools/xtp/vendor/xtp-bindgen/src/types.ts
function cons(t, opts) {
  t.nullable = opts?.nullable === void 0 ? false : opts.nullable;
  return t;
}
var StringType = class {
  constructor(opts) {
    this.kind = "string";
    cons(this, opts);
  }
};
var UInt8Type = class {
  constructor(opts) {
    this.kind = "uint8";
    cons(this, opts);
  }
};
var Int8Type = class {
  constructor(opts) {
    this.kind = "int8";
    cons(this, opts);
  }
};
var UInt16Type = class {
  constructor(opts) {
    this.kind = "uint16";
    cons(this, opts);
  }
};
var Int16Type = class {
  constructor(opts) {
    this.kind = "int16";
    cons(this, opts);
  }
};
var UInt32Type = class {
  constructor(opts) {
    this.kind = "uint32";
    cons(this, opts);
  }
};
var Int32Type = class {
  constructor(opts) {
    this.kind = "int32";
    cons(this, opts);
  }
};
var UInt64Type = class {
  constructor(opts) {
    this.kind = "uint64";
    cons(this, opts);
  }
};
var Int64Type = class {
  constructor(opts) {
    this.kind = "int64";
    cons(this, opts);
  }
};
var FloatType = class {
  constructor(opts) {
    this.kind = "float";
    cons(this, opts);
  }
};
var DoubleType = class {
  constructor(opts) {
    this.kind = "double";
    cons(this, opts);
  }
};
var BooleanType = class {
  constructor(opts) {
    this.kind = "boolean";
    cons(this, opts);
  }
};
var BufferType = class {
  constructor(opts) {
    this.kind = "buffer";
    cons(this, opts);
  }
};
var DateTimeType = class {
  constructor(opts) {
    this.kind = "date-time";
    cons(this, opts);
  }
};
var ObjectType = class {
  constructor(name, properties, opts) {
    this.kind = "object";
    this.name = name;
    this.properties = properties;
    cons(this, opts);
  }
};
var FreeFormObjectType = class {
  constructor(opts) {
    this.kind = "free-form-object";
    cons(this, opts);
  }
};
var MapType = class {
  constructor(valueType, opts) {
    this.kind = "map";
    this.keyType = new StringType();
    this.valueType = valueType;
    cons(this, opts);
  }
};
var EnumType = class {
  constructor(name, elementType, values, opts) {
    this.kind = "enum";
    this.name = name;
    this.elementType = elementType;
    this.values = values;
    cons(this, opts);
  }
};
var ArrayType = class {
  constructor(elementType, opts) {
    this.kind = "array";
    this.elementType = elementType;
    cons(this, opts);
  }
};

// tools/xtp/vendor/xtp-bindgen/src/normalizer.ts
function isProperty(p) {
  return !!p.type;
}
function isExport(e) {
  return !!e.name;
}
function normalizeV0Schema(parsed) {
  const exports2 = [];
  const imports = [];
  const schemas = {};
  const errors = parsed.errors;
  const warnings = parsed.warnings;
  parsed.exports.forEach((ex) => {
    exports2.push({
      name: ex
    });
  });
  return {
    version: "v0",
    exports: exports2,
    imports,
    schemas,
    errors,
    warnings
  };
}
function detectSchemaRefCycles(schema, schemas, context) {
  const schemaName = schema.name;
  if (context.stack.has(schemaName)) {
    const cycle = context.path.slice(context.path.indexOf(schemaName));
    cycle.push(schemaName);
    context.errors.push(
      new ValidationError(
        `Detected circular reference: ${cycle.join(" -> ")}`,
        context.path.join("/")
      )
    );
    return;
  }
  if (context.visited.has(schemaName)) {
    return;
  }
  context.stack.add(schemaName);
  context.path.push(schemaName);
  function checkPossibleRef(obj, label) {
    if (!obj) return;
    context.path.push(label);
    if (obj.$ref && typeof obj.$ref !== "string") {
      detectSchemaRefCycles(obj.$ref, schemas, context);
    }
    if (obj.items) {
      context.path.push("items");
      if (obj.items.$ref && typeof obj.items.$ref !== "string") {
        detectSchemaRefCycles(obj.items.$ref, schemas, context);
      }
      context.path.pop();
    }
    if (obj.additionalProperties) {
      context.path.push("additionalProperties");
      checkPossibleRef(obj.additionalProperties, "value");
      context.path.pop();
    }
    context.path.pop();
  }
  for (const prop of schema.properties) {
    checkPossibleRef(prop, prop.name);
  }
  if (schema.additionalProperties) {
    checkPossibleRef(schema.additionalProperties, "additionalProperties");
  }
  context.stack.delete(schemaName);
  context.path.pop();
  context.visited.add(schemaName);
}
var V1SchemaNormalizer = class {
  constructor(parsed) {
    this.version = "v1";
    this.exports = [];
    this.imports = [];
    this.schemas = {};
    this.parsed = parsed;
    this.errors = parsed.errors;
    this.warnings = parsed.warnings;
  }
  normalize() {
    if (this.parsed.components?.schemas) {
      for (const name in this.parsed.components.schemas) {
        const pSchema = this.parsed.components.schemas[name];
        const properties = [];
        if (pSchema.properties) {
          for (const name2 in pSchema.properties) {
            const required = pSchema.required?.includes(name2);
            properties.push({ ...pSchema.properties[name2], name: name2, required });
          }
        }
        const schema = pSchema;
        schema.name = name;
        schema.properties = properties;
        this.schemas[name] = schema;
      }
    }
    this.annotateType(this.parsed);
    const cycleContext = {
      visited: /* @__PURE__ */ new Set(),
      stack: /* @__PURE__ */ new Set(),
      errors: [],
      path: ["#", "components", "schemas"]
    };
    for (const schema of Object.values(this.schemas)) {
      detectSchemaRefCycles(schema, this.schemas, cycleContext);
    }
    this.errors.push(...cycleContext.errors);
    if (this.parsed.exports) {
      for (const name in this.parsed.exports) {
        const ex = this.parsed.exports[name];
        ex.name = name;
        this.exports.push(ex);
      }
    }
    if (this.parsed.imports) {
      for (const name in this.parsed.imports) {
        const im = this.parsed.imports[name];
        im.name = name;
        this.imports.push(im);
      }
    }
    return {
      version: "v1",
      exports: this.exports,
      imports: this.imports,
      schemas: this.schemas,
      errors: this.errors,
      warnings: this.warnings
    };
  }
  // NOTE: we may want to relax this again so we can keep normalizing
  // even if a ref is invalid
  querySchemaRef(ref) {
    const parts = ref.split("/");
    const name = parts[3];
    return this.schemas[name];
  }
  annotateType(s) {
    if (!s || typeof s !== "object" || Array.isArray(s)) return void 0;
    if (s.xtpType) return s.xtpType;
    if (s.type && s.type === "object" && s.additionalProperties) {
      s.type = "map";
      const valueType = this.annotateType(s.additionalProperties);
      return valueType ? new MapType(valueType, s) : void 0;
    }
    if (s.properties && s.properties.length > 0) {
      s.type = "object";
      const properties = [];
      for (const pname in s.properties) {
        const p = s.properties[pname];
        const t = this.annotateType(p);
        if (t) {
          p.xtpType = t;
          properties.push(t);
        }
      }
      return new ObjectType(s.name, properties, s);
    }
    if (s.$ref) {
      if (s.type) {
        return void 0;
      }
      if (typeof s.$ref === "string") {
        s.$ref = this.querySchemaRef(s.$ref);
      }
      s.type = "object";
      let obj = this.annotateType(s.$ref);
      if (obj && s.nullable) {
        obj = { ...obj };
        obj.nullable = s.nullable;
      }
      return obj;
    }
    if (s.enum) {
      s.type = "enum";
      return new EnumType(s.name || "", new StringType(), s.enum, s);
    }
    if (s.items) {
      const itemType = this.annotateType(s.items);
      return itemType ? new ArrayType(itemType, s) : void 0;
    }
    switch (s.type) {
      case "string":
        return s.format === "date-time" ? new DateTimeType(s) : new StringType(s);
      case "integer":
        if (s.format === "uint8") return new UInt8Type(s);
        if (s.format === "int8") return new Int8Type(s);
        if (s.format === "uint16") return new UInt16Type(s);
        if (s.format === "int16") return new Int16Type(s);
        if (s.format === "uint32") return new UInt32Type(s);
        if (s.format === "int32") return new Int32Type(s);
        if (s.format === "uint64") return new UInt64Type(s);
        return new Int64Type(s);
      case "boolean":
        return new BooleanType(s);
      case "buffer":
        return new BufferType(s);
      case "number":
        if (s.format === "float") return new FloatType(s);
        return new DoubleType(s);
      case "object":
        return new FreeFormObjectType(s);
    }
    for (const key in s) {
      if (Object.prototype.hasOwnProperty.call(s, key)) {
        const child = s[key];
        if (child && typeof child === "object" && !Array.isArray(child)) {
          const t = this.annotateType(child);
          if (t) child.xtpType = t;
        }
      }
    }
    return void 0;
  }
};
function normalizeV1Schema(parsed) {
  const normalizer = new V1SchemaNormalizer(parsed);
  const schema = normalizer.normalize();
  return schema;
}
function parseAndNormalizeJson(encoded) {
  const doc = parseAny(JSON.parse(encoded));
  assert(doc);
  if (isV0Schema(doc)) {
    return assert(normalizeV0Schema(doc));
  } else if (isV1Schema(doc)) {
    return assert(normalizeV1Schema(doc));
  } else {
    throw new NormalizeError("Could not normalize unknown version of schema", [{
      message: "Could not normalize unknown version of schema",
      path: "#/version"
    }]);
  }
}
function assert(results) {
  const { errors } = results;
  if (errors && errors.length > 0) {
    if (errors.length === 1) {
      throw new NormalizeError(errors[0].message, errors);
    } else {
      throw new NormalizeError(`${errors[0].message} (and ${errors.length - 1} other error(s))`, errors);
    }
  }
  return results;
}
var NormalizeError = class extends Error {
  constructor(msg, errors) {
    super(msg);
    this.errors = errors;
  }
};

// tools/xtp/vendor/xtp-bindgen/src/index.ts
function parse(schema) {
  return parseAndNormalizeJson(schema);
}
function getContext() {
  const ctx = JSON.parse(Config.get("ctx") || "{}");
  ctx.schema = parse(JSON.stringify(ctx.schema));
  if (Array.isArray(ctx.featureFlags)) {
    ctx.featureFlags = ctx.featureFlags.reduce((a, c) => {
      a[c] = true;
      return a;
    }, {});
  } else {
    ctx.featureFlags = ctx.featureFlags || {};
  }
  return ctx;
}
function codeSamples(ex, lang) {
  if (!ex.codeSamples) {
    return [];
  }
  return ex.codeSamples.filter(
    (s) => s.lang.toLowerCase() === lang.toLowerCase()
  );
}
function hasComment(p) {
  if (!p) return false;
  if (isProperty(p)) {
    return !!(p.description || p.$ref);
  } else if (isExport(p)) {
    return !!(p.description || hasComment(p.input) || hasComment(p.output));
  }
  return false;
}
function formatCommentLine(s) {
  if (!s) return "";
  return s.trimEnd().replace(/\n/g, " ");
}
function formatCommentBlock(s, prefix) {
  if (!s) return "";
  if (!prefix) prefix = " * ";
  return s.trimEnd().replace(/\n/g, `
${prefix}`);
}
function isJsonEncoded(p) {
  if (!p) return false;
  return p.contentType === "application/json";
}
function isUtf8Encoded(p) {
  if (!p) return false;
  return p.contentType === "text/plain; charset=utf-8";
}
function isPrimitive(p) {
  if (!p.$ref) return true;
  return !!p.$ref.enum || !p.$ref.properties;
}
function isDateTime(p) {
  return p?.xtpType?.kind === "date-time";
}
function isBuffer(p) {
  return p?.xtpType?.kind === "buffer";
}
function isObject(p) {
  return p?.xtpType?.kind === "object";
}
function isFreeFormObject(p) {
  return p?.xtpType?.kind === "free-form-object";
}
function isArray(p) {
  return p?.xtpType?.kind === "array";
}
function isEnum(p) {
  return p?.xtpType?.kind === "enum";
}
function isString(p) {
  return p?.xtpType?.kind === "string";
}
function isUInt8(p) {
  return p?.xtpType?.kind === "uint8";
}
function isInt8(p) {
  return p?.xtpType?.kind === "int8";
}
function isUInt16(p) {
  return p?.xtpType?.kind === "uint16";
}
function isInt16(p) {
  return p?.xtpType?.kind === "int16";
}
function isUInt32(p) {
  return p?.xtpType?.kind === "uint32";
}
function isInt32(p) {
  return p?.xtpType?.kind === "int32";
}
function isUInt64(p) {
  return p?.xtpType?.kind === "uint64";
}
function isInt64(p) {
  return p?.xtpType?.kind === "int64";
}
function isFloat(p) {
  return p?.xtpType?.kind === "float";
}
function isDouble(p) {
  return p?.xtpType?.kind === "double";
}
function isBoolean(p) {
  return p?.xtpType?.kind === "boolean";
}
function isMap(p) {
  return p?.xtpType?.kind === "map";
}
function capitalize(s) {
  return s.charAt(0).toUpperCase() + s.slice(1);
}
function camelToSnakeCase(s) {
  return s.split(/(?=[A-Z])/).join("_").toLowerCase();
}
function snakeToCamelCase(s) {
  return s.split("_").map((part, index) => {
    if (index === 0) return part;
    return part.charAt(0).toUpperCase() + part.slice(1);
  }).join("");
}
function snakeToPascalCase(s) {
  return capitalize(snakeToCamelCase(s));
}
var helpers = {
  hasComment,
  formatCommentLine,
  formatCommentBlock,
  codeSamples,
  isDateTime,
  isPrimitive,
  isBuffer,
  isObject,
  isFreeFormObject,
  isEnum,
  isArray,
  isString,
  isUInt8,
  isInt8,
  isUInt16,
  isInt16,
  isUInt32,
  isInt32,
  isUInt64,
  isInt64,
  isFloat,
  isDouble,
  isMap,
  isBoolean,
  isJsonEncoded,
  isUtf8Encoded,
  capitalize,
  camelToSnakeCase,
  snakeToCamelCase,
  snakeToPascalCase
};
// Annotate the CommonJS export names for ESM import in node:
0 && (module.exports = {
  ArrayType,
  BooleanType,
  BufferType,
  DateTimeType,
  DoubleType,
  EnumType,
  FloatType,
  FreeFormObjectType,
  Int16Type,
  Int32Type,
  Int64Type,
  Int8Type,
  MapType,
  NormalizeError,
  ObjectType,
  StringType,
  UInt16Type,
  UInt32Type,
  UInt64Type,
  UInt8Type,
  ValidationError,
  getContext,
  helpers,
  isExport,
  isProperty,
  parse,
  parseAndNormalizeJson
});
