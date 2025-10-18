# Parser Migration Plan

This note captures how the current JSONata parser/evaluator in `src/jsonata` works today and what we need to replicate when we reimplement it in Rust.  The goal is to help future-me (or the next agent session) pick up the work without rediscovering the internals from scratch.

## 1. Current JS pipeline (status quo)

The hybrid runtime still depends on the upstream JavaScript engine for the *entire* front-end:

1. `src/jsonata/src/parser.js`
   - Implements a hand-written Pratt parser.
   - Contains a custom tokenizer (`tokenizer(path)`) that scans strings, numbers, regex literals, names and operators while tracking position for error reporting.
   - Defines an operator precedence table (`operators` map) and then parses by repeatedly calling `tokenizer` and applying precedence rules (`expression`, `led`, `nud` style).
   - Outputs an AST tree composed of plain JS objects.  The important node shapes: `path`, `binary`, `unary`, `name`, `value`, `string`, `number`, `regex`, `lambda`, `partial`, `apply`, `descendant`, `transform`, etc.  Nodes often carry additional fields (`predicate`, `group`, `focus`, `tuple`, `keepArray`, `slot`, …).

2. `src/jsonata/src/jsonata.js`
   - Receives the AST produced by `parser(path)`.
   - Exposes `jsonata(expr)` which returns an object with `evaluate()`/`assign()`/etc.
   - Runs the evaluation pipeline via the big async `evaluate(expr, input, environment)` dispatcher.  Each node type is mapped to a helper (`evaluatePath`, `evaluateBinary`, `evaluateGroupExpression`, `evaluateFilter`, …).
   - Uses `utils.js` to model JSONata “sequences” (arrays with metadata such as `sequence`, `outerWrapper`, `tupleStream`, `keepSingleton`).
   - Uses lexical environments (“frames”) built via `createFrame`/`environment.lookup(...)` for variable binding, parent references, tail-call protection, and function scope.
   - Handles timeboxing and depth-limiting (`timeboxExpression`) to guard against runaway recursion.
   - Handles async/promise return values uniformly (`isPromise` checks + `await`).

3. `src/jsonata/src/functions.js` + `signature.js` + `datetime.js`
   - Define the built-in functions that the evaluator imports as `fn`.  In the hybrid runtime we now delegate many of those to Rust, but the JS evaluator still orchestrates arguments, partial application, higher-order built-ins, etc.

In short: even though `$sum` now executes in Rust, the *expression* `OrderID & ': ' & $sum(...)` is still parsed, type-checked, and evaluated by the JS Pratt parser + evaluator, with N-API used only when it hits a built-in function shim.

## 2. What we must mirror in Rust

When we port the parser/evaluator we have to replicate these pieces:

| Component                        | Responsibility                                       |
|----------------------------------|------------------------------------------------------|
| Tokenizer                        | Unicode aware lexing of numbers, strings, regex, operators, comments, whitespace handling. |
| Pratt parser                     | Operator precedence/associativity, binding powers, support for ternary (`?:`), `??`, `~>`, array/object constructors, lambda syntax, partial application, etc. |
| AST node definitions             | Node kinds and fields must match what `jsonata.js` expects (so we can keep tests and data structures identical). |
| Environment/Frames               | Variable binding, scoping (`$`, `$$`, `$var :=`), partial application, closures. |
| Sequence semantics               | JSONata “array” wrappers with `sequence`, `outerWrapper`, `tupleStream`, `keepSingleton` flags. |
| Evaluation helpers               | Everything under `evaluate*` in `jsonata.js`: path navigation, predicates, grouping, focus-handling, higher-order combinators, timeboxing guards. |
| Error reporting                  | All `SXXXX`/`TXXXX` diagnostic codes with correct `position`, `token`, stack augmentation. |
| Async handling                   | The current evaluator propagates Promises: the Rust version must replicate or replace that semantics. |

Initially we can keep the Rust evaluator inside the same process boundary (still callable from JS via N-API) but the implementation must be capable of returning either JSONata sequences or Promises/Futures.

## 3. Migration strategy

1. **Baseline specification**
   - Extract grammar/precedence tables from `parser.js`.
   - List every token type and node shape we need to support (make a JSON schema snapshot of AST nodes).
   - Document evaluation semantics per node (based on `evaluate*` functions).

2. **Rust tokenizer and AST**
   - Implement a Rust lexer (consider `logos` or a custom iterator) that mirrors the JS tokenizer, including the regex literal scanning and string escapes.
   - Define AST structs/enums matching the JS nodes.  Preserve field names/semantics to ease diff testing (`serde_json` snapshots from JS AST vs Rust AST).  A good first milestone: parse expressions in Rust and compare the AST (in JSON) with the JS parser output.

3. **Parser (Pratt)**
   - Implement a Pratt parser in Rust using the same binding powers and oddities (`consarray`, `tuple`, `'?:'`, etc.).
   - Integrate signature parsing for function annotations (`signature.js`) or provide equivalent metadata.

4. **Evaluator in Rust**
   - Port each `evaluate*` helper from `jsonata.js`, starting with pure/stateless ones (`evaluateLiteral`, `evaluateUnary`, `evaluateBinary`, `evaluatePath`).
   - Reimplement JSONata sequences (`struct Sequence { elements: Vec<Value>, sequence: bool, … }`) matching the `utils.createSequence` behavior.
   - Port environment/frame logic (lexical scope, closures, partials, `$parent`, `$variable`).
   - Port grouping, tuple streams, focus propagation, transform expressions, function application, higher-order functions.  Reuse the Rust built-ins we already have in `jsonata-rust`.
   - Implement timeboxing depth/timeout checks (probably using a monotonic timer and recursion depth counters).
   - Provide async support: either decide on `tokio` futures or keep the interface synchronous for the first milestone and map async-only tests to TODO.

5. **Bridging & rollout**
   - Expose the Rust parser/evaluator behind a feature flag so we can run both engines side-by-side.
   - Write differential tests: parse an expression in JS, parse in Rust, ensure AST equality.  Evaluate with both engines and compare outputs against the conformance suite.
   - Once confident, switch the Node addon to call the Rust engine and keep the JS one as fallback for missing features during migration.

## 4. Immediate action items for the next session

1. Produce a JSON snapshot of AST node shapes from the JS parser (small script that calls `parser(path)` and dumps the result).  This will be our contract for the Rust structs.
2. Pick the Rust parsing crates/tools we will use (hand-written Pratt vs `pest`/`lalrpop`).
3. Scaffold a Rust crate (maybe `jsonata-rust/parser`) with lexer + AST definitions and start porting the tokenizer logic.
4. Set up golden tests to compare JS AST vs Rust AST for a battery of expressions (to confirm parity as we implement).
5. Plan how to integrate the Rust evaluator with the existing N-API bridge once the parser is ready.

Once these steps are underway we can confidently retire the JS front-end and run everything through the Rust stack.  Until then the hybrid mode remains, but the above plan gives us a clear path to the full port.

## 5. Progress snapshot (2024-??-??)

- **Tokenizer**: the Rust lexer in `src/jsonata-rust/src/parser/tokenizer.rs` now mirrors the JS behaviour for strings, escapes, regex literals (with error codes and positions), variables, names, numbers, and operators.
- **Pratt parser scaffolding**: the Rust parser (`parser.rs`) can drive most basic constructs plus function calls, rudimentary lambda detection (`function`/`λ` names with `$var` args) and block/array/object literals. Signatures are still skipped rather than parsed.
- **Hybrid glue**: `src/jsonata-js-rust/src/parser.js` calls the Rust parser first, and rehydrates any `type: 'regex'` nodes back into real `RegExp` objects before the evaluator touches them. This fixed the `$match("test escape \\", /\\/)` OOM.
- **Known gaps**: no `processAST` equivalent yet (paths/predicates/ancestry remain untouched), signature parsing is still TODO, and many node types are stubs (partials, chains, transforms, etc.). Evaluator still depends on JS runtime for everything beyond built-in dispatch.

### Helper scripts

- `scripts/dev_exec.sh`: runs commands *inside* the dev container (`docker compose run --rm dev …`) while forwarding stdin/stdout. Useful for quick experiments:
  ```bash
  ./scripts/dev_exec.sh -w src/jsonata-js-rust -- pnpm test
  ./scripts/dev_exec.sh -- bash -lc 'ls -al'
  ./scripts/dev_exec.sh -w src/jsonata-js-rust -- node <<'JS'
  console.log('hello from container');
  JS
  ```
- `scripts/rust_parse.sh`: pipes an expression (argument or stdin) through the Rust parser via N-API and prints the JSON result. Ideal for diffing ASTs.
  ```bash
  ./scripts/rust_parse.sh '$sum([1,2,3])'
  ./scripts/rust_parse.sh -r <<'EOF'
  $match("test escape \\", /\\/)
  EOF
  ```
  Returns `{ ok: true, ast: … }` on success or propagates the structured parser error `{ ok: false, error: … }`.

Keep these in mind when drilling down on remaining gaps—they save a lot of manual escaping compared to one-off `node -e` invocations.
