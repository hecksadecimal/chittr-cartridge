"use strict";
var __create = Object.create;
var __defProp = Object.defineProperty;
var __getOwnPropDesc = Object.getOwnPropertyDescriptor;
var __getOwnPropNames = Object.getOwnPropertyNames;
var __getProtoOf = Object.getPrototypeOf;
var __hasOwnProp = Object.prototype.hasOwnProperty;
var __commonJS = (cb, mod) => function __require() {
  return mod || (0, cb[__getOwnPropNames(cb)[0]])((mod = { exports: {} }).exports, mod), mod.exports;
};
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
var __toESM = (mod, isNodeMode, target) => (target = mod != null ? __create(__getProtoOf(mod)) : {}, __copyProps(
  // If the importer is in node compatibility mode or this is not an ESM
  // file that has been converted to a CommonJS file using a Babel-
  // compatible transform (i.e. "__esModule" has not been set), then set
  // "default" to the CommonJS "module.exports" for node compatibility.
  isNodeMode || !mod || !mod.__esModule ? __defProp(target, "default", { value: mod, enumerable: true }) : target,
  mod
));
var __toCommonJS = (mod) => __copyProps(__defProp({}, "__esModule", { value: true }), mod);

// tools/xtp/build/node_modules/ejs/lib/utils.js
var require_utils = __commonJS({
  "tools/xtp/build/node_modules/ejs/lib/utils.js"(exports2) {
    "use strict";
    var regExpChars = /[|\\{}()[\]^$+*?.]/g;
    var hasOwnProperty = Object.prototype.hasOwnProperty;
    var hasOwn = function(obj, key) {
      return hasOwnProperty.apply(obj, [key]);
    };
    exports2.escapeRegExpChars = function(string) {
      if (!string) {
        return "";
      }
      return String(string).replace(regExpChars, "\\$&");
    };
    var _ENCODE_HTML_RULES = {
      "&": "&amp;",
      "<": "&lt;",
      ">": "&gt;",
      '"': "&#34;",
      "'": "&#39;"
    };
    var _MATCH_HTML = /[&<>'"]/g;
    function encode_char(c) {
      return _ENCODE_HTML_RULES[c] || c;
    }
    var escapeFuncStr = `var _ENCODE_HTML_RULES = {
      "&": "&amp;"
    , "<": "&lt;"
    , ">": "&gt;"
    , '"': "&#34;"
    , "'": "&#39;"
    }
  , _MATCH_HTML = /[&<>'"]/g;
function encode_char(c) {
  return _ENCODE_HTML_RULES[c] || c;
};
`;
    exports2.escapeXML = function(markup) {
      return markup == void 0 ? "" : String(markup).replace(_MATCH_HTML, encode_char);
    };
    function escapeXMLToString() {
      return Function.prototype.toString.call(this) + ";\n" + escapeFuncStr;
    }
    try {
      if (typeof Object.defineProperty === "function") {
        Object.defineProperty(exports2.escapeXML, "toString", { value: escapeXMLToString });
      } else {
        exports2.escapeXML.toString = escapeXMLToString;
      }
    } catch (err) {
      console.warn("Unable to set escapeXML.toString (is the Function prototype frozen?)");
    }
    exports2.shallowCopy = function(to, from) {
      from = from || {};
      if (to !== null && to !== void 0) {
        for (var p in from) {
          if (!hasOwn(from, p)) {
            continue;
          }
          if (p === "__proto__" || p === "constructor") {
            continue;
          }
          to[p] = from[p];
        }
      }
      return to;
    };
    exports2.shallowCopyFromList = function(to, from, list) {
      list = list || [];
      from = from || {};
      if (to !== null && to !== void 0) {
        for (var i = 0; i < list.length; i++) {
          var p = list[i];
          if (typeof from[p] != "undefined") {
            if (!hasOwn(from, p)) {
              continue;
            }
            if (p === "__proto__" || p === "constructor") {
              continue;
            }
            to[p] = from[p];
          }
        }
      }
      return to;
    };
    exports2.cache = {
      _data: {},
      set: function(key, val) {
        this._data[key] = val;
      },
      get: function(key) {
        return this._data[key];
      },
      remove: function(key) {
        delete this._data[key];
      },
      reset: function() {
        this._data = {};
      }
    };
    exports2.hyphenToCamel = function(str) {
      return str.replace(/-[a-z]/g, function(match) {
        return match[1].toUpperCase();
      });
    };
    exports2.createNullProtoObjWherePossible = function() {
      if (typeof Object.create == "function") {
        return function() {
          return /* @__PURE__ */ Object.create(null);
        };
      }
      if (!({ __proto__: null } instanceof Object)) {
        return function() {
          return { __proto__: null };
        };
      }
      return function() {
        return {};
      };
    }();
    exports2.hasOwnOnlyObject = function(obj) {
      var o = exports2.createNullProtoObjWherePossible();
      for (var p in obj) {
        if (hasOwn(obj, p)) {
          o[p] = obj[p];
        }
      }
      return o;
    };
  }
});

// tools/xtp/build/node_modules/ejs/package.json
var require_package = __commonJS({
  "tools/xtp/build/node_modules/ejs/package.json"(exports2, module2) {
    module2.exports = {
      name: "ejs",
      description: "Embedded JavaScript templates",
      keywords: [
        "template",
        "engine",
        "ejs"
      ],
      version: "3.1.10",
      author: "Matthew Eernisse <mde@fleegix.org> (http://fleegix.org)",
      license: "Apache-2.0",
      bin: {
        ejs: "./bin/cli.js"
      },
      main: "./lib/ejs.js",
      jsdelivr: "ejs.min.js",
      unpkg: "ejs.min.js",
      repository: {
        type: "git",
        url: "git://github.com/mde/ejs.git"
      },
      bugs: "https://github.com/mde/ejs/issues",
      homepage: "https://github.com/mde/ejs",
      dependencies: {
        jake: "^10.8.5"
      },
      devDependencies: {
        browserify: "^16.5.1",
        eslint: "^6.8.0",
        "git-directory-deploy": "^1.5.1",
        jsdoc: "^4.0.2",
        "lru-cache": "^4.0.1",
        mocha: "^10.2.0",
        "uglify-js": "^3.3.16"
      },
      engines: {
        node: ">=0.10.0"
      },
      scripts: {
        test: "npx jake test"
      }
    };
  }
});

// tools/xtp/build/node_modules/ejs/lib/ejs.js
var require_ejs = __commonJS({
  "tools/xtp/build/node_modules/ejs/lib/ejs.js"(exports2) {
    "use strict";
    var fs = require("fs");
    var path = require("path");
    var utils = require_utils();
    var scopeOptionWarned = false;
    var _VERSION_STRING = require_package().version;
    var _DEFAULT_OPEN_DELIMITER = "<";
    var _DEFAULT_CLOSE_DELIMITER = ">";
    var _DEFAULT_DELIMITER = "%";
    var _DEFAULT_LOCALS_NAME = "locals";
    var _NAME = "ejs";
    var _REGEX_STRING = "(<%%|%%>|<%=|<%-|<%_|<%#|<%|%>|-%>|_%>)";
    var _OPTS_PASSABLE_WITH_DATA = [
      "delimiter",
      "scope",
      "context",
      "debug",
      "compileDebug",
      "client",
      "_with",
      "rmWhitespace",
      "strict",
      "filename",
      "async"
    ];
    var _OPTS_PASSABLE_WITH_DATA_EXPRESS = _OPTS_PASSABLE_WITH_DATA.concat("cache");
    var _BOM = /^\uFEFF/;
    var _JS_IDENTIFIER = /^[a-zA-Z_$][0-9a-zA-Z_$]*$/;
    exports2.cache = utils.cache;
    exports2.fileLoader = fs.readFileSync;
    exports2.localsName = _DEFAULT_LOCALS_NAME;
    exports2.promiseImpl = new Function("return this;")().Promise;
    exports2.resolveInclude = function(name, filename, isDir) {
      var dirname = path.dirname;
      var extname = path.extname;
      var resolve = path.resolve;
      var includePath = resolve(isDir ? filename : dirname(filename), name);
      var ext = extname(name);
      if (!ext) {
        includePath += ".ejs";
      }
      return includePath;
    };
    function resolvePaths(name, paths) {
      var filePath;
      if (paths.some(function(v) {
        filePath = exports2.resolveInclude(name, v, true);
        return fs.existsSync(filePath);
      })) {
        return filePath;
      }
    }
    function getIncludePath(path2, options) {
      var includePath;
      var filePath;
      var views = options.views;
      var match = /^[A-Za-z]+:\\|^\//.exec(path2);
      if (match && match.length) {
        path2 = path2.replace(/^\/*/, "");
        if (Array.isArray(options.root)) {
          includePath = resolvePaths(path2, options.root);
        } else {
          includePath = exports2.resolveInclude(path2, options.root || "/", true);
        }
      } else {
        if (options.filename) {
          filePath = exports2.resolveInclude(path2, options.filename);
          if (fs.existsSync(filePath)) {
            includePath = filePath;
          }
        }
        if (!includePath && Array.isArray(views)) {
          includePath = resolvePaths(path2, views);
        }
        if (!includePath && typeof options.includer !== "function") {
          throw new Error('Could not find the include file "' + options.escapeFunction(path2) + '"');
        }
      }
      return includePath;
    }
    function handleCache(options, template) {
      var func;
      var filename = options.filename;
      var hasTemplate = arguments.length > 1;
      if (options.cache) {
        if (!filename) {
          throw new Error("cache option requires a filename");
        }
        func = exports2.cache.get(filename);
        if (func) {
          return func;
        }
        if (!hasTemplate) {
          template = fileLoader(filename).toString().replace(_BOM, "");
        }
      } else if (!hasTemplate) {
        if (!filename) {
          throw new Error("Internal EJS error: no file name or template provided");
        }
        template = fileLoader(filename).toString().replace(_BOM, "");
      }
      func = exports2.compile(template, options);
      if (options.cache) {
        exports2.cache.set(filename, func);
      }
      return func;
    }
    function tryHandleCache(options, data, cb) {
      var result;
      if (!cb) {
        if (typeof exports2.promiseImpl == "function") {
          return new exports2.promiseImpl(function(resolve, reject) {
            try {
              result = handleCache(options)(data);
              resolve(result);
            } catch (err) {
              reject(err);
            }
          });
        } else {
          throw new Error("Please provide a callback function");
        }
      } else {
        try {
          result = handleCache(options)(data);
        } catch (err) {
          return cb(err);
        }
        cb(null, result);
      }
    }
    function fileLoader(filePath) {
      return exports2.fileLoader(filePath);
    }
    function includeFile(path2, options) {
      var opts = utils.shallowCopy(utils.createNullProtoObjWherePossible(), options);
      opts.filename = getIncludePath(path2, opts);
      if (typeof options.includer === "function") {
        var includerResult = options.includer(path2, opts.filename);
        if (includerResult) {
          if (includerResult.filename) {
            opts.filename = includerResult.filename;
          }
          if (includerResult.template) {
            return handleCache(opts, includerResult.template);
          }
        }
      }
      return handleCache(opts);
    }
    function rethrow(err, str, flnm, lineno, esc) {
      var lines = str.split("\n");
      var start = Math.max(lineno - 3, 0);
      var end = Math.min(lines.length, lineno + 3);
      var filename = esc(flnm);
      var context = lines.slice(start, end).map(function(line, i) {
        var curr = i + start + 1;
        return (curr == lineno ? " >> " : "    ") + curr + "| " + line;
      }).join("\n");
      err.path = filename;
      err.message = (filename || "ejs") + ":" + lineno + "\n" + context + "\n\n" + err.message;
      throw err;
    }
    function stripSemi(str) {
      return str.replace(/;(\s*$)/, "$1");
    }
    exports2.compile = function compile(template, opts) {
      var templ;
      if (opts && opts.scope) {
        if (!scopeOptionWarned) {
          console.warn("`scope` option is deprecated and will be removed in EJS 3");
          scopeOptionWarned = true;
        }
        if (!opts.context) {
          opts.context = opts.scope;
        }
        delete opts.scope;
      }
      templ = new Template(template, opts);
      return templ.compile();
    };
    exports2.render = function(template, d, o) {
      var data = d || utils.createNullProtoObjWherePossible();
      var opts = o || utils.createNullProtoObjWherePossible();
      if (arguments.length == 2) {
        utils.shallowCopyFromList(opts, data, _OPTS_PASSABLE_WITH_DATA);
      }
      return handleCache(opts, template)(data);
    };
    exports2.renderFile = function() {
      var args = Array.prototype.slice.call(arguments);
      var filename = args.shift();
      var cb;
      var opts = { filename };
      var data;
      var viewOpts;
      if (typeof arguments[arguments.length - 1] == "function") {
        cb = args.pop();
      }
      if (args.length) {
        data = args.shift();
        if (args.length) {
          utils.shallowCopy(opts, args.pop());
        } else {
          if (data.settings) {
            if (data.settings.views) {
              opts.views = data.settings.views;
            }
            if (data.settings["view cache"]) {
              opts.cache = true;
            }
            viewOpts = data.settings["view options"];
            if (viewOpts) {
              utils.shallowCopy(opts, viewOpts);
            }
          }
          utils.shallowCopyFromList(opts, data, _OPTS_PASSABLE_WITH_DATA_EXPRESS);
        }
        opts.filename = filename;
      } else {
        data = utils.createNullProtoObjWherePossible();
      }
      return tryHandleCache(opts, data, cb);
    };
    exports2.Template = Template;
    exports2.clearCache = function() {
      exports2.cache.reset();
    };
    function Template(text, optsParam) {
      var opts = utils.hasOwnOnlyObject(optsParam);
      var options = utils.createNullProtoObjWherePossible();
      this.templateText = text;
      this.mode = null;
      this.truncate = false;
      this.currentLine = 1;
      this.source = "";
      options.client = opts.client || false;
      options.escapeFunction = opts.escape || opts.escapeFunction || utils.escapeXML;
      options.compileDebug = opts.compileDebug !== false;
      options.debug = !!opts.debug;
      options.filename = opts.filename;
      options.openDelimiter = opts.openDelimiter || exports2.openDelimiter || _DEFAULT_OPEN_DELIMITER;
      options.closeDelimiter = opts.closeDelimiter || exports2.closeDelimiter || _DEFAULT_CLOSE_DELIMITER;
      options.delimiter = opts.delimiter || exports2.delimiter || _DEFAULT_DELIMITER;
      options.strict = opts.strict || false;
      options.context = opts.context;
      options.cache = opts.cache || false;
      options.rmWhitespace = opts.rmWhitespace;
      options.root = opts.root;
      options.includer = opts.includer;
      options.outputFunctionName = opts.outputFunctionName;
      options.localsName = opts.localsName || exports2.localsName || _DEFAULT_LOCALS_NAME;
      options.views = opts.views;
      options.async = opts.async;
      options.destructuredLocals = opts.destructuredLocals;
      options.legacyInclude = typeof opts.legacyInclude != "undefined" ? !!opts.legacyInclude : true;
      if (options.strict) {
        options._with = false;
      } else {
        options._with = typeof opts._with != "undefined" ? opts._with : true;
      }
      this.opts = options;
      this.regex = this.createRegex();
    }
    Template.modes = {
      EVAL: "eval",
      ESCAPED: "escaped",
      RAW: "raw",
      COMMENT: "comment",
      LITERAL: "literal"
    };
    Template.prototype = {
      createRegex: function() {
        var str = _REGEX_STRING;
        var delim = utils.escapeRegExpChars(this.opts.delimiter);
        var open = utils.escapeRegExpChars(this.opts.openDelimiter);
        var close = utils.escapeRegExpChars(this.opts.closeDelimiter);
        str = str.replace(/%/g, delim).replace(/</g, open).replace(/>/g, close);
        return new RegExp(str);
      },
      compile: function() {
        var src;
        var fn;
        var opts = this.opts;
        var prepended = "";
        var appended = "";
        var escapeFn = opts.escapeFunction;
        var ctor;
        var sanitizedFilename = opts.filename ? JSON.stringify(opts.filename) : "undefined";
        if (!this.source) {
          this.generateSource();
          prepended += '  var __output = "";\n  function __append(s) { if (s !== undefined && s !== null) __output += s }\n';
          if (opts.outputFunctionName) {
            if (!_JS_IDENTIFIER.test(opts.outputFunctionName)) {
              throw new Error("outputFunctionName is not a valid JS identifier.");
            }
            prepended += "  var " + opts.outputFunctionName + " = __append;\n";
          }
          if (opts.localsName && !_JS_IDENTIFIER.test(opts.localsName)) {
            throw new Error("localsName is not a valid JS identifier.");
          }
          if (opts.destructuredLocals && opts.destructuredLocals.length) {
            var destructuring = "  var __locals = (" + opts.localsName + " || {}),\n";
            for (var i = 0; i < opts.destructuredLocals.length; i++) {
              var name = opts.destructuredLocals[i];
              if (!_JS_IDENTIFIER.test(name)) {
                throw new Error("destructuredLocals[" + i + "] is not a valid JS identifier.");
              }
              if (i > 0) {
                destructuring += ",\n  ";
              }
              destructuring += name + " = __locals." + name;
            }
            prepended += destructuring + ";\n";
          }
          if (opts._with !== false) {
            prepended += "  with (" + opts.localsName + " || {}) {\n";
            appended += "  }\n";
          }
          appended += "  return __output;\n";
          this.source = prepended + this.source + appended;
        }
        if (opts.compileDebug) {
          src = "var __line = 1\n  , __lines = " + JSON.stringify(this.templateText) + "\n  , __filename = " + sanitizedFilename + ";\ntry {\n" + this.source + "} catch (e) {\n  rethrow(e, __lines, __filename, __line, escapeFn);\n}\n";
        } else {
          src = this.source;
        }
        if (opts.client) {
          src = "escapeFn = escapeFn || " + escapeFn.toString() + ";\n" + src;
          if (opts.compileDebug) {
            src = "rethrow = rethrow || " + rethrow.toString() + ";\n" + src;
          }
        }
        if (opts.strict) {
          src = '"use strict";\n' + src;
        }
        if (opts.debug) {
          console.log(src);
        }
        if (opts.compileDebug && opts.filename) {
          src = src + "\n//# sourceURL=" + sanitizedFilename + "\n";
        }
        try {
          if (opts.async) {
            try {
              ctor = new Function("return (async function(){}).constructor;")();
            } catch (e) {
              if (e instanceof SyntaxError) {
                throw new Error("This environment does not support async/await");
              } else {
                throw e;
              }
            }
          } else {
            ctor = Function;
          }
          fn = new ctor(opts.localsName + ", escapeFn, include, rethrow", src);
        } catch (e) {
          if (e instanceof SyntaxError) {
            if (opts.filename) {
              e.message += " in " + opts.filename;
            }
            e.message += " while compiling ejs\n\n";
            e.message += "If the above error is not helpful, you may want to try EJS-Lint:\n";
            e.message += "https://github.com/RyanZim/EJS-Lint";
            if (!opts.async) {
              e.message += "\n";
              e.message += "Or, if you meant to create an async function, pass `async: true` as an option.";
            }
          }
          throw e;
        }
        var returnedFn = opts.client ? fn : function anonymous(data) {
          var include = function(path2, includeData) {
            var d = utils.shallowCopy(utils.createNullProtoObjWherePossible(), data);
            if (includeData) {
              d = utils.shallowCopy(d, includeData);
            }
            return includeFile(path2, opts)(d);
          };
          return fn.apply(
            opts.context,
            [data || utils.createNullProtoObjWherePossible(), escapeFn, include, rethrow]
          );
        };
        if (opts.filename && typeof Object.defineProperty === "function") {
          var filename = opts.filename;
          var basename = path.basename(filename, path.extname(filename));
          try {
            Object.defineProperty(returnedFn, "name", {
              value: basename,
              writable: false,
              enumerable: false,
              configurable: true
            });
          } catch (e) {
          }
        }
        return returnedFn;
      },
      generateSource: function() {
        var opts = this.opts;
        if (opts.rmWhitespace) {
          this.templateText = this.templateText.replace(/[\r\n]+/g, "\n").replace(/^\s+|\s+$/gm, "");
        }
        this.templateText = this.templateText.replace(/[ \t]*<%_/gm, "<%_").replace(/_%>[ \t]*/gm, "_%>");
        var self = this;
        var matches = this.parseTemplateText();
        var d = this.opts.delimiter;
        var o = this.opts.openDelimiter;
        var c = this.opts.closeDelimiter;
        if (matches && matches.length) {
          matches.forEach(function(line, index) {
            var closing;
            if (line.indexOf(o + d) === 0 && line.indexOf(o + d + d) !== 0) {
              closing = matches[index + 2];
              if (!(closing == d + c || closing == "-" + d + c || closing == "_" + d + c)) {
                throw new Error('Could not find matching close tag for "' + line + '".');
              }
            }
            self.scanLine(line);
          });
        }
      },
      parseTemplateText: function() {
        var str = this.templateText;
        var pat = this.regex;
        var result = pat.exec(str);
        var arr = [];
        var firstPos;
        while (result) {
          firstPos = result.index;
          if (firstPos !== 0) {
            arr.push(str.substring(0, firstPos));
            str = str.slice(firstPos);
          }
          arr.push(result[0]);
          str = str.slice(result[0].length);
          result = pat.exec(str);
        }
        if (str) {
          arr.push(str);
        }
        return arr;
      },
      _addOutput: function(line) {
        if (this.truncate) {
          line = line.replace(/^(?:\r\n|\r|\n)/, "");
          this.truncate = false;
        }
        if (!line) {
          return line;
        }
        line = line.replace(/\\/g, "\\\\");
        line = line.replace(/\n/g, "\\n");
        line = line.replace(/\r/g, "\\r");
        line = line.replace(/"/g, '\\"');
        this.source += '    ; __append("' + line + '")\n';
      },
      scanLine: function(line) {
        var self = this;
        var d = this.opts.delimiter;
        var o = this.opts.openDelimiter;
        var c = this.opts.closeDelimiter;
        var newLineCount = 0;
        newLineCount = line.split("\n").length - 1;
        switch (line) {
          case o + d:
          case o + d + "_":
            this.mode = Template.modes.EVAL;
            break;
          case o + d + "=":
            this.mode = Template.modes.ESCAPED;
            break;
          case o + d + "-":
            this.mode = Template.modes.RAW;
            break;
          case o + d + "#":
            this.mode = Template.modes.COMMENT;
            break;
          case o + d + d:
            this.mode = Template.modes.LITERAL;
            this.source += '    ; __append("' + line.replace(o + d + d, o + d) + '")\n';
            break;
          case d + d + c:
            this.mode = Template.modes.LITERAL;
            this.source += '    ; __append("' + line.replace(d + d + c, d + c) + '")\n';
            break;
          case d + c:
          case "-" + d + c:
          case "_" + d + c:
            if (this.mode == Template.modes.LITERAL) {
              this._addOutput(line);
            }
            this.mode = null;
            this.truncate = line.indexOf("-") === 0 || line.indexOf("_") === 0;
            break;
          default:
            if (this.mode) {
              switch (this.mode) {
                case Template.modes.EVAL:
                case Template.modes.ESCAPED:
                case Template.modes.RAW:
                  if (line.lastIndexOf("//") > line.lastIndexOf("\n")) {
                    line += "\n";
                  }
              }
              switch (this.mode) {
                // Just executing code
                case Template.modes.EVAL:
                  this.source += "    ; " + line + "\n";
                  break;
                // Exec, esc, and output
                case Template.modes.ESCAPED:
                  this.source += "    ; __append(escapeFn(" + stripSemi(line) + "))\n";
                  break;
                // Exec and output
                case Template.modes.RAW:
                  this.source += "    ; __append(" + stripSemi(line) + ")\n";
                  break;
                case Template.modes.COMMENT:
                  break;
                // Literal <%% mode, append as raw output
                case Template.modes.LITERAL:
                  this._addOutput(line);
                  break;
              }
            } else {
              this._addOutput(line);
            }
        }
        if (self.opts.compileDebug && newLineCount) {
          this.currentLine += newLineCount;
          this.source += "    ; __line = " + this.currentLine + "\n";
        }
      }
    };
    exports2.escapeXML = utils.escapeXML;
    exports2.__express = exports2.renderFile;
    exports2.VERSION = _VERSION_STRING;
    exports2.name = _NAME;
    if (typeof window != "undefined") {
      window.ejs = exports2;
    }
  }
});

// tools/xtp/vendor/xtp-typescript-bindgen/src/index.ts
var index_exports = {};
__export(index_exports, {
  render: () => render
});
module.exports = __toCommonJS(index_exports);
var import_ejs = __toESM(require_ejs());

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

// tools/xtp/build/core-compat.ts
function patchFreeFormObjects(node, seen) {
  if (!node || typeof node !== "object") return;
  if (seen.has(node)) return;
  seen.add(node);
  if (Array.isArray(node)) {
    for (const item of node) patchFreeFormObjects(item, seen);
    return;
  }
  const obj = node;
  if (obj.kind === "free-form-object") {
    obj.kind = "object";
    if (obj.name === void 0) obj.name = "";
    if (obj.properties === void 0) obj.properties = [];
  }
  for (const key of Object.keys(obj)) patchFreeFormObjects(obj[key], seen);
}
function getContext2() {
  const ctx = getContext();
  patchFreeFormObjects(ctx.schema, /* @__PURE__ */ new WeakSet());
  return ctx;
}

// tools/xtp/vendor/xtp-typescript-bindgen/src/index.ts
function toTypeScriptTypeX(type) {
  const nullify = (t) => `${t}${type.nullable ? " | null" : ""}`;
  switch (type.kind) {
    case "string":
      return nullify("string");
    case "int32":
    case "float":
    case "double":
    case "byte":
      return nullify("number");
    case "date-time":
      return nullify("Date");
    case "boolean":
      return nullify("boolean");
    case "array":
      const arrayType = type;
      return nullify(`Array<${toTypeScriptTypeX(arrayType.elementType)}>`);
    case "buffer":
      return nullify("ArrayBufferLike");
    case "object":
      const oType = type;
      if (oType.properties?.length > 0) {
        return nullify(oType.name);
      } else {
        return nullify("any");
      }
    case "enum":
      return nullify(type.name);
    case "map":
      const { keyType, valueType } = type;
      return nullify(`Record<${toTypeScriptTypeX(keyType)}, ${toTypeScriptTypeX(valueType)}>`);
    case "int64":
      return nullify("number");
    default:
      throw new Error("Cant convert property to typescript type: " + JSON.stringify(type));
  }
}
function toTypeScriptType(property) {
  return toTypeScriptTypeX(property.xtpType);
}
function isCastable(t) {
  if (["date-time", "buffer"].includes(t.kind)) return true;
  switch (t.kind) {
    case "object":
      const oType = t;
      return oType.properties && oType.properties.length > 0;
    case "array":
      return isCastable(t.elementType);
    case "map":
      return isCastable(t.valueType);
    default:
      return false;
  }
}
function castExpression(t, direction) {
  switch (t.kind) {
    case "object":
      const oType = t;
      return `${oType.name}.${direction.toLowerCase()}Json`;
    case "array":
      return `castArray(${castExpression(t.elementType, direction)})`;
    case "map":
      return `castMap(${castExpression(t.valueType, direction)})`;
    case "date-time":
      return `date${direction}Json`;
    case "buffer":
      return `buffer${direction}Json`;
    default:
      throw new Error(`Type not meant to be casted ${JSON.stringify(t)}`);
  }
}
function render() {
  const tmpl = Host.inputString();
  const ctx = {
    ...getContext2(),
    ...helpers,
    isCastable,
    castExpression,
    toTypeScriptType
  };
  const output = import_ejs.default.render(tmpl, ctx);
  Host.outputString(output);
}
// Annotate the CommonJS export names for ESM import in node:
0 && (module.exports = {
  render
});
/*! Bundled license information:

ejs/lib/ejs.js:
  (**
   * @file Embedded JavaScript templating engine. {@link http://ejs.co}
   * @author Matthew Eernisse <mde@fleegix.org>
   * @author Tiancheng "Timothy" Gu <timothygu99@gmail.com>
   * @project EJS
   * @license {@link http://www.apache.org/licenses/LICENSE-2.0 Apache License, Version 2.0}
   *)
*/
