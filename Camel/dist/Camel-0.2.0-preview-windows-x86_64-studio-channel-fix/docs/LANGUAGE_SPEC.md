# Camel 0.2.0 preview language specification

Status: implemented interpreter baseline, not a stable 1.0 contract. This document describes `runtime/`, not every feature claimed by the recovered alpha article. Where they differ, this specification and executable conformance tests define the preview.

## Purpose

Camel combines ordinary programming with first-class agents and host tools. An ordinary program must execute without a model server. Models supply nondeterministic results across explicit, typed boundaries; they do not replace source-level control flow.

The pipeline is source/import loading → lexer → source-located AST parser → declaration/name checks → static expression checks → general interpreter. `check` ends before interpretation and executes no source expressions.

Programs may declare `version "0.2";`. Omitted versions use the current preview edition (`0.2`); explicit unsupported versions fail during checking. Imported modules that declare a version must agree with the entry program. The accepted fixtures under `conformance/0.2/` define this edition's supported syntax and static-check behavior.

## Core syntax

Semicolons terminate simple statements. Blocks use braces. Identifiers are case-sensitive, start with a Unicode letter or underscore, and continue with letters, digits or underscores. Strings use double quotes and escapes `\n`, `\r`, `\t`, `\"`, `\\`. Comments use `//` or non-nesting `/* ... */`.

Integers are decimal signed 64-bit values; floating literals contain a decimal point. Exponent notation and numeric separators are not implemented. Arithmetic is checked: integer overflow, invalid division and non-finite floating results throw errors. Integer division truncates toward zero. Mixed integer/double arithmetic promotes to double. String concatenation requires two strings; interpolation performs explicit display conversion.

```camel
record Measurement {
    string name;
    int[] samples;
}

int Add(int left, int right = 1) {
    return left + right;
}

workflow int Main(int seed = 2) {
    int total = Add(seed);
    var values = [1, 2, 3];
    foreach (int value in values) {
        total += value;
    }
    log($"Total: {total}");
    return total;
}
```

Compact grammar (expression precedence is specified below):

```text
program      := (version-declaration | declaration)*
version-declaration := 'version' STRING ';'
declaration  := ('using' | 'import') STRING ';'
             | 'record' NAME '{' (type NAME ';')* '}'
             | ('agent' | 'policy') NAME configuration
             | 'tool' type NAME parameters configuration
             | type NAME parameters block
             | 'workflow' [type] NAME parameters block
configuration:= '{' (NAME '=' expression ';')* '}'
parameters   := '(' [type NAME ['=' expression] (',' type NAME ['=' expression])*] ')'
type         := NAME | 'Task' '<' type '>' | type '[' ']'
block        := '{' statement* '}'
statement    := block | declaration-statement | assignment ';' | expression ';'
             | 'if' '(' expression ')' statement ['else' statement]
             | 'while' '(' expression ')' statement
             | 'for' '(' simple ';' expression ';' simple ')' statement
             | 'foreach' '(' (type | 'var') NAME 'in' expression ')' statement
             | 'return' [expression] ';' | ('break' | 'continue') ';'
             | 'throw' expression ';'
             | 'try' block ['catch' '(' NAME ')' block] ['finally' block]
declaration-statement := (type | 'var') NAME '=' expression ';'
assignment   := assignable ('=' | '+=' | '-=') expression | assignable ('++' | '--')
assignable   := local-variable ['.' NAME | '[' expression ']']*
```

At least one of `catch` or `finally` must follow `try`. The `for` initializer and step are simple declarations, assignments, updates or expressions; the condition is required. Trailing commas are not implemented.

Expressions include literals, identifiers, arrays, `new { field = expression, ... }`, `new RecordName { ... }`, calls, members, indexes and interpolated strings `$"literal {expression}"`. Escape interpolation braces as `{{` and `}}`; embedded quotes must be escaped in the outer string.

Precedence, highest first: calls/members/indexes; unary `!`, `-`, `+`, `await`, `spawn`; `* / %`; `+ -`; `< <= > >=`; `== !=`; `&&`; `||`. Binary operators associate left. Logical operators require booleans and short-circuit. Conditions are strictly boolean; no truthiness coercion. Equality is defined for JSON-serializable values, with numeric integer/double comparison; callable/audio/task equality is unsupported.

Operand, argument, array-item and object-initializer evaluation follows source order. Compound assignment evaluates its target indexes exactly once. Object storage and JSON key order are not source-order contracts.

## Values, scope and types

Types: `int`, `double`, `bool`, `string`, `any`, `object`, `Audio`, `Agent`, `Tool`, `Function`, `AgentResult`, declared records, `T[]`, and `Task<T>`. `void` is only a function/workflow/tool return type or task result type. Type and declaration keywords are reserved as applicable.

Record contracts are structural and exact: every declared field must exist, no undeclared fields are accepted, and nested values must conform. `new RecordName` checks its fields. `new { ... }` is an untyped object. Arrays have value semantics; string and array indexes are zero-based. Strings index Unicode scalar values, not bytes or grapheme clusters.

Type enforcement occurs when binding typed variables/parameters, assigning typed roots, returning from functions/tools and receiving declared model responses. A double contract accepts integers. Null is accepted by `any`, not by other value types. `Task<T>` checks the awaited result; this is not static proof of task result types.

`check` infers expression types for literals, local bindings, records, arrays, operators, function/tool calls, and agent results. It checks known assignments, returns, call arguments, `foreach` items, and boolean conditions. `var` keeps the inferred type when known; ambiguous arrays and dynamic host/model values remain `any`. `any` is an explicit gradual-typing boundary and is accepted where a concrete type is expected; runtime checks still enforce typed bindings, calls, assignments, and returns. Record schemas are nominal in static analysis and structural/exact at runtime.

`check` validates syntax, names, duplicate declarations, declared type names, configuration keys, parameter/default ordering, named-call arity, explicit constructor field sets, placement of loop controls/returns, and the static checks above. It does not prove that every non-void control-flow path returns, type-check dynamic values behind `any`, or validate registered host handlers.

Blocks create lexical scopes. Inner bindings may shadow outer ones. Functions do not implicitly capture caller locals. Parameters and returns pass values, not mutable references. Named functions, agents and tools can be stored or passed as values. Defaults may refer to globals, not preceding parameters or caller locals. There are no global mutable variables or anonymous closures yet.

An assignment copies the declared root, applies the update, validates it, and then replaces the binding. Failure leaves the prior binding unchanged. Expression/host side effects that occurred while evaluating the right-hand side are not rolled back.

## Modules and entry points

`using "relative/path.camel";` (alias `import`) resolves from the importing file. `using "@camel/<name>/<file>.camel";` resolves installed library source when no local file matches. Library resolution is offline and rejects traversal outside the installed package. Catalog entries carry a semantic library version and Git revision; `camel library update` reads the latest catalog by default when GitHub access is available, so source libraries can advance without a runtime release. An imported file is loaded once; cycles and duplicate global names fail. Imports flatten declarations into one program without namespaces. Local relative imports may traverse parent directories; they are trusted source loading, not a filesystem sandbox. The `camel library` commands manage GitHub source snapshots through the shared [camel-library-manager package](../packages/camel-library-manager/README.md).

The default entry is workflow `Main`. `--workflow Other` selects another declared workflow. A workflow without a return annotation is void. Non-void workflows return their declared value. Ordinary functions cannot be selected as CLI workflows.

All arguments after `--` bind positionally. Required arguments must be supplied; optional trailing defaults come from source. `check` does not require entry arguments or an entry workflow, so library modules can be checked separately.

## Errors and tasks

`throw value;` raises its displayed value as a source-located error. `catch (error)` binds an object with `message`, `file` and `line`. `finally` executes during ordinary unwinding and may override an ordinary return/error. Budget exhaustion is fatal and cannot be swallowed; an exhausted step budget may prevent a language-level finally block from running. Host resources must use their own cleanup ownership.

`spawn expression` immediately snapshots local bindings and starts an OS-thread task. `await task` returns its cached value or raises its cached error. Repeated awaits do not repeat effects. For the current synchronous agent adapter, `await` applied to an already computed non-task value returns that value; use `spawn` for actual parallel model calls.

Tasks do not share mutable local variables with parents. They share host services, logs and execution fuel. The engine waits for every spawned task before returning, including discarded tasks. Unobserved task failures propagate; errors already observed by an await can be handled by catch. Parallel log/effect order is nondeterministic. This is a per-execution structured lifetime, not per-block cancellation or a durable scheduler.

Defaults: 100,000 expression/statement/provider-step ticks, call depth 128, and 64 total spawned tasks. Source size is limited to 8 MiB per file and 100,000 tokens per file; parser/expression/type/import nesting is bounded. Runtime record/value validation is limited to depth 64; model JSON schemas to depth 16. These are safety bounds, not wall-clock or memory guarantees. Blocking host operations follow their own timeout/stop contracts.

## Agent declarations and results

Audio transcription and note summarization agents are supplied by the separately versioned [`audio` source library](../libraries/audio/note_summarizer.camel). The interpreter retains generic host support for audio input (`Audio`, `record`, and `audio.load`); model choices, prompts, response records, and orchestration live in the library and can be updated through the catalog.

```camel
record Answer { string text; }
agent Helper {
    model = env("CAMEL_LM_MODEL", "local-model-id");
    system = "Answer using only supplied evidence.";
    response = Answer;
    maxTurns = 3;
    retries = 1;
    temperature = 0;
}
workflow Answer Main(string question) {
    Agent worker = Helper;
    AgentResult result = await worker.run(question);
    return result.data;
}
```

Allowed fields: required `model`; optional `system`, `response`, `tools`, `policy`, `maxTurns`, `retries`, `temperature`. Configuration expressions are evaluated in global context when invoked; source-authored expressions are trusted code. There is no fixed list of agent/model names.

Defaults: system empty, no response schema, no tools, deny-by-default tool policy, maxTurns 4, retries 0, temperature 0. Bounds: maxTurns 1..128, retries 0..3, temperature 0..2. Response must name a record. Record schema instructions are included in the system message and output is validated locally; server-side schema enforcement is not required.

An agent accepts exactly one prompt. Strings are plain user messages. JSON-serializable objects become JSON text. An audio prompt is `new { text = "...", audio = audioValue }`, where audio may also be an array of `Audio` values. String paths do not automatically become audio.

`AgentResult` contains `text` (raw final text), `data` (validated record when declared, otherwise a string), `model` and `toolCalls`. Invalid JSON or record fields fail, without a plaintext fallback. A closed leading `<think>...</think>` section and a surrounding JSON code fence may be removed before typed parsing. No confidence/accuracy guarantee follows from structural type validation.

`maxTurns` counts successful model turns, including tool-request turns. `retries` allows additional provider attempts for transport/provider failures on each turn; malformed final outputs and tool failures are not automatically retried. Total provider attempts per invocation are bounded by `maxTurns * (retries + 1)`, plus the shared execution fuel. Previously executed tool effects are never repeated merely because the next provider request is retried.

## Tools and policies

```camel
tool int Count(string text) {
    description = "Count Unicode scalar characters.";
    handler = "text.length";
}
policy Safe {
    allowTools = [Count];
    maxToolCalls = 2;
    allowNetwork = false;
    allowFileRead = false;
    allowFileWrite = false;
    allowShell = false;
}
```

Tool arguments and return values use ordinary Camel type contracts. `handler` names a registered host function; it is not an arbitrary shell command. Policies require an allowlisted tool name, an allowed registered capability, and remaining tool budget. Defaults deny capabilities and tools, with maxToolCalls 0; the maximum is 1024 per agent invocation.

Before the first provider request, offered tools are checked against the policy and registry. Before executing a requested batch, all call names, unique IDs and typed argument objects are checked. Duplicate call IDs are rejected across the entire invocation. Every dispatched call passes through ToolRegistry, and assistant/tool messages with their IDs are retained for subsequent turns.

Each handler owns its effect boundary. The model's entire tool-call batch is **not** an atomic transaction. A later failure does not undo earlier successful handlers. Registered handlers must define idempotence where needed; equal arguments with new IDs are not automatically deduplicated.

Standard handlers:

| Handler | Signature | Capability |
| --- | --- | --- |
| `text.length` | `(string) -> int` | Pure |
| `json.stringify` | `(any) -> string` | Pure |
| `files.read` | `(string relativePath) -> string` | FileRead |
| `files.write` | `(string relativePath, string content) -> string` | FileWrite |
| `http.get` | `(string URL) -> string` | Network |

The shipped request client supports HTTP and HTTPS. HTTP tools have a 30-second timeout. No shell handler is shipped.

File tools resolve from the entry source directory, not the imported module or process working directory. Canonical paths must stay within that directory, including resolved symlinks/junctions. Write parents must exist. Writes stage and sync complete bytes before replacing one target file; repeated writes intentionally replace it. Concurrent external filesystem changes are not defended as an OS-grade sandbox.

Policies constrain **model-requested tools**. Trusted workflow code may call declared tools directly, read environment variables, record audio and explicitly provide audio to a model. `allowNetwork=false` does not block the configured inference endpoint. Hosts and providers are trusted. No isolation from arbitrary malicious source, remote endpoint access, host callbacks or prompt injection is claimed.

## Builtins and methods

- `log(value)`: capture displayed text; CLI presents logs after successful completion.
- `env(name[, fallback])`: environment string or supplied fallback, default empty string.
- `json.stringify(value)`, `json.parse(text)`: serialize/parse ordinary JSON values; audio, callable and task values cannot serialize.
- `len(value)`: string character, array item or object field count.
- `string(value)`, `int(value)`, `double(value)`: explicit conversions. Int accepts integers or integer text; double accepts numeric values or numeric text.
- `assert(bool[, message])`: throw when false.
- `range(start, end)`: end-exclusive integer array, at most 100,000 elements.
- `clock.now()`: Unix timestamp seconds.
- `record(seconds)`: default-device microphone capture, PCM16 WAV in memory; seconds 1..3600, or zero to stop on Enter. CLI recording flags override duration. Interactive recording requires an explicit user stop and has no wall-clock cancellation primitive.
- `audio.load(path)`: validate nonempty supported-extension file and base64-encode it. This is not a codec validator or resampler. Absolute paths are accepted; relative paths prefer process cwd, then source directory.

String methods: `trim()`, `startsWith(text)`, `endsWith(text)`, `contains(text)`, `split(separator)`. String/array `.length` is supported. Arrays support `join(separator)` for strings and `concat(array)` returning a new array. There is no mutating `push` method; indexed assignment is supported.

Audio capture is record-then-process, not streamed transcription. The backend's native sample rate/channel count are preserved. Model/server compatibility for `input_audio` must be independently verified; Camel's tests do not establish LM Studio/Liquid serving support.

## Not implemented and next milestones

1. Whole-program static type analysis, nominal record metadata, comprehensive negative/fuzz conformance suites, and compatibility versioning.
2. Namespaced modules, packages/lockfiles, standard library growth, closures and higher-order typed function signatures.
3. Cancellable async I/O, resource/memory budgets, durable tasks, agent state/history APIs, and provider capability negotiation.
4. Native or bytecode compilation, formatting, debugger/source maps, editor/LSP tooling, and release/install infrastructure.

These are language-development milestones, not hidden features of the preview. The recovered alpha demo and architecture article remain historical references; no full feature parity is claimed.
