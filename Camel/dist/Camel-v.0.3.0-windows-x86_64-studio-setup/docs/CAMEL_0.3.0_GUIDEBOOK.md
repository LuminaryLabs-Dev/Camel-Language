---
title: Camel Guidebook
subtitle: Language, agents, tools, and the 0.3.0 development path
version: v.0.3.0 candidate
chapters: 10
page_sections: 50
---

<!-- Page breaks use Pandoc's \newpage command. The source is divided into exactly 50 page sections, five per chapter. Printed pagination can vary with renderer and fonts. -->

<!-- Page 01 -->
# Chapter 1 — Camel and the Agentic Program

## Page 1 of 50 · A language for agentic work

Camel is a general-purpose programming language shaped for programs that use agents, models, and host tools. Its central idea is to keep ordinary program logic in source code and put model reasoning behind explicit boundaries. A Camel workflow can validate inputs, branch, call functions, start bounded tasks, call an agent, check its typed result, and decide what happens next.

Camel is not a prompt template format. A `.camel` file declares executable functions, records, tools, policies, agents, and workflows. The runtime parses and checks that source, then interprets it. Rust builds the current runtime and Studio; a Camel program is not translated into Rust or native machine code.

The current Windows candidate labels its executable distribution **v.0.3.0**. Hosted Windows CI remains pending; this candidate is not tagged or published. This guide teaches implemented behavior and records remaining limits.

\newpage

<!-- Page 02 -->
## Page 2 of 50 · The programming model

A useful mental model is **source controls; agents suggest; tools act**. Source code owns sequencing and typed data transformations. An agent receives a prompt and may return text or a validated record. If the model requests a tool, Camel checks that the tool exists, its capability is registered, the policy allows it, and call limits remain. The host handler then owns the actual effect.

This separation makes model use optional. Ordinary programs run without a model server. Use an agent only where probabilistic language or judgment adds value. Keep deterministic calculations, bounds checks, policy decisions, and final output assembly in Camel or registered host code.

The program can make nondeterministic steps visible without pretending they are deterministic. Record the prompt inputs, model identifier, response schema, limits, and tool policy in code. Treat model output as untrusted input even when the runtime confirms that its shape matches a record.

\newpage

<!-- Page 03 -->
## Page 3 of 50 · A source-to-result path

The execution path is source loading, lexing, parsing, declaration and name checks, static expression checks, then interpretation. `camel check file.camel` stops before evaluation. It does not execute expressions, call tools, record audio, or contact a model. A successful check proves only the properties the analyzer can establish; dynamic values and host registrations remain runtime concerns.

The interpreter evaluates the selected workflow, normally `Main`. It binds CLI arguments, creates per-run scopes and logs, and applies shared execution limits. A workflow can return a value; with `--json`, the CLI emits a JSON object containing the value and captured logs. Provider and host effects depend on the program and local configuration.

This ordered pipeline gives bugs useful locations. Syntax failures point to source; declaration and type diagnostics identify known inconsistencies; runtime errors identify failures that require actual values or host behavior. When reporting a defect, preserve the source location and the stage at which it occurred.

\newpage

<!-- Page 04 -->
## Page 4 of 50 · A first program

The core example computes a typed result without AI:

```camel
version "0.2";

record Answer {
    int input;
    int doubled;
}

workflow Answer Main(int value = 21) {
    assert(value >= 0, "value must be nonnegative");
    return new Answer { input = value, doubled = value * 2 };
}
```

Run `camel check path/to/first.camel`, then `camel path/to/first.camel --json`. The optional `--` separator is useful when passing positional workflow values: `camel path/to/first.camel --json -- 8`. The declared default lets the example run without an argument.

The record describes output shape, `Main` is the entry workflow, `assert` guards an input invariant, and the constructor makes returned fields visible. This is an executable Camel program even though it contains no agents. Start with programs like this before introducing providers or side effects.

\newpage

<!-- Page 05 -->
## Page 5 of 50 · What the preview promises

The 0.2 preview already includes ordinary functions, records, arrays, control flow, exceptions, imports, static expression checks, lint warnings, workflows, bounded tasks, agents, typed agent results, capability-checked tools, file handlers, audio host input, CLI execution, and a native Studio. The exact contract is in `docs/LANGUAGE_SPEC.md` and `conformance/0.2/`.

The preview does not promise whole-program static proof, safe execution of arbitrary untrusted source, cooperative cancellation, durable task recovery, resumable checkpoints, total run deadlines, persisted provider replays, token accounting, provenance-bearing evidence, provider capability negotiation, or content-locked library dependency resolution. Several of these are tracked as 0.3.0 readiness work; others remain longer-term language work.

Write documentation and examples against the current specification. Label a roadmap item as planned until code, conformance, and runtime behavior establish it. This distinction lets users learn what works while contributors can see what remains.

\newpage

<!-- Page 06 -->
# Chapter 2 — Reading and Writing Camel

## Page 6 of 50 · Files, editions, and declarations

Camel programs use the `.camel` extension. A file may begin with `version "0.2";`. Omitting the declaration selects the runtime's current preview edition; writing an unsupported version causes checking to fail. Imported modules that declare an edition must agree with the entry program. The edition fixture directory records accepted and rejected language behavior.

Top-level declarations include `using`/`import`, `record`, `agent`, `policy`, `tool`, functions, and workflows. A simple declaration ends with a semicolon. Blocks use braces. Identifiers are case-sensitive; they start with a Unicode letter or underscore and continue with letters, digits, or underscores.

Comments use `//` for a line and `/* ... */` for a non-nesting block. Strings use double quotes and support newline, carriage return, tab, quote, and backslash escapes. Keep one main entry workflow and move reusable declarations into imported files only when reuse makes the program clearer.

\newpage

<!-- Page 07 -->
## Page 7 of 50 · Values and types

The preview types include `int`, `double`, `bool`, `string`, `any`, `object`, `Audio`, `Agent`, `Tool`, `Function`, `AgentResult`, declared records, arrays such as `string[]`, and tasks such as `Task<int>`. `void` is a return type for functions, workflows, and tools; it is not a general variable type.

Integers are signed 64-bit values. Floating-point literals contain a decimal point; exponent notation and numeric separators are not implemented. Arithmetic checks overflow, division errors, and non-finite results. Integer division truncates toward zero. Mixed integer and double arithmetic promotes to double.

Use concrete types at boundaries. Reserve `any` for dynamic data from providers or host integrations. It weakens static knowledge, but it does not turn off runtime checks when the value enters a typed binding, function parameter, assignment, or return. An integer satisfies a `double` contract; `null` is accepted only by `any`.

\newpage

<!-- Page 08 -->
## Page 8 of 50 · Expressions and conditions

Expressions include literals, identifiers, arrays, object and record constructors, calls, member access, indexing, and interpolated strings. Interpolation uses `$"value: {expression}"`; write `{{` and `}}` for literal braces. The embedded expression is evaluated as Camel code, and conversion to display text is explicit.

Operator precedence runs from calls/member/index access, unary operators, multiplication, addition, comparisons, equality, `&&`, then `||`. Binary operators associate left. `&&` and `||` short-circuit. Conditions must be booleans: Camel does not treat nonempty strings or nonzero numbers as true.

Evaluation of operands, arguments, array items, and object fields follows source order. String and array indexes start at zero. String indexing counts Unicode scalar values, not bytes or user-perceived grapheme clusters. Equality applies to JSON-serializable values, with numeric integer/double comparison; callable, audio, and task equality is unsupported.

\newpage

<!-- Page 09 -->
## Page 9 of 50 · Bindings and records

Use typed declarations when the contract matters and `var` when the initializer's type is inferable:

```camel
int retries = 2;
var labels = ["analysis", "review"];
record Finding { string claim; string[] evidence; }
Finding item = new Finding {
    claim = "The build passed.",
    evidence = ["CI run 1842"]
};
```

Record contracts are exact at runtime: every declared field must exist, extra fields are rejected, and nested values must conform. `new { ... }` creates an untyped object; `new Finding { ... }` names the record contract. Static analysis treats record schemas nominally while runtime validation checks the concrete shape.

Arrays have value semantics. Parameters and returns pass values rather than mutable references. Updating a typed root stages a copy, validates the new value, then replaces the binding; if validation fails, the prior binding remains. Side effects that took place while calculating the update are not rolled back.

\newpage

<!-- Page 10 -->
## Page 10 of 50 · Common syntax limits

Blocks establish lexical scopes; an inner binding may shadow an outer one. Functions do not capture caller locals. Defaults can refer to globals, but not to earlier parameters or caller-local variables. There are no global mutable variables or anonymous closures in this preview.

The `for` loop initializer and step are simple declarations, assignments, updates, or expressions; the condition is required. `foreach` iterates arrays. Use `break` and `continue` only within loops. At least one of `catch` or `finally` must follow `try`. Trailing commas are not supported in lists.

Before using a construct in a tutorial, confirm it in the specification or the `conformance/0.2` fixtures. Avoid borrowing syntax from Rust, Python, or the recovered alpha demo. In particular, the historical alpha example's `as` casts and `Console.Log` calls are not current preview syntax.

\newpage

<!-- Page 11 -->
# Chapter 3 — Functions, Data, and Modules

## Page 11 of 50 · Functions and workflows

Functions package deterministic transformations; workflows define executable entry paths. Both can have typed parameters and return types. A workflow called `Main` is the default CLI entry. `--workflow Other` selects a different declared workflow. A regular function cannot be selected as the CLI entry.

```camel
int Sum(int[] values) {
    int total = 0;
    foreach (int value in values) { total += value; }
    return total;
}

workflow int Main(int[] values = [2, 3, 5]) {
    return Sum(values);
}
```

All arguments after `--` bind positionally. Required arguments must be supplied; trailing optional parameters use their source defaults. Strings pass literally, numbers and booleans parse as such, while arrays and records accept JSON. Defaults may reference globals but cannot depend on caller-local state.

\newpage

<!-- Page 12 -->
## Page 12 of 50 · Records as contracts

Records make data boundaries explicit. A record can represent a request, a parsed result, a review, or a final report. Keep fields specific enough that consumers can distinguish known facts from open questions. Prefer `string[] evidence` over a single opaque paragraph when downstream logic must inspect evidence.

```camel
record Review {
    string[] findings;
    string[] uncertainties;
}
record Summary {
    string topic;
    Review review;
}
```

Construct values with every declared field. A model response can target a record, in which case Camel validates the returned data locally. This establishes shape and value types, not truth, source quality, calibration, or completeness. Include uncertainty fields and validate claims against source material when correctness matters.

\newpage

<!-- Page 13 -->
## Page 13 of 50 · Arrays and built-ins

Arrays are typed by their elements where the analyzer can infer a useful type. Use indexed assignment for updates; there is no mutating `push` method. Array `.length` and `len(value)` provide counts. `join` works on string arrays, while `concat` returns a new array. `range(start, end)` creates an end-exclusive integer array capped at 100,000 elements.

Useful built-ins include `log(value)`, `env(name[, fallback])`, `json.stringify(value)`, `json.parse(text)`, `string(value)`, `int(value)`, `double(value)`, `assert(bool[, message])`, and `clock.now()`. Conversion functions are explicit; do not assume implicit coercion.

`json.stringify` cannot serialize audio, callable, or task values. `env` reads process configuration and returns a string or fallback. Do not log secrets. Logging is buffered by the CLI and presented after successful completion; it is not a live progress stream.

\newpage

<!-- Page 14 -->
## Page 14 of 50 · Imports and module shape

`using "relative/path.camel";` imports a local module; `import` is an alias. Paths resolve from the importing file. A package import such as `using "@camel/standard/core.camel";` resolves installed library source if no local file matches. Relative imports take precedence over installed-library imports.

Imported files load once. Cycles and duplicate global names fail. The preview flattens imported declarations into a single global namespace; there are no namespaces. Local relative paths may traverse parent directories, so imports are trusted source loading, not a sandbox.

Keep modules cohesive: one domain of functions, records, or agent definitions. Avoid exporting generic names likely to collide. For reusable source libraries, register the package in the catalog and list all required files. Check the module and an importing program separately; `camel check` does not require a `Main` workflow or entry arguments.

\newpage

<!-- Page 15 -->
## Page 15 of 50 · Designing a small application

Separate a program into four concerns: input and validation; deterministic business rules; model work, when needed; and output assembly. Put stable record contracts near the workflows that own them. Put reusable pure functions in local modules. Put reusable agent prompts and response records in a source library only when their lifecycle truly differs from the host program.

Keep one obvious entry workflow. Give it defaults only when those defaults are safe and meaningful. Make constraints visible with typed parameters and `assert`; send concise actionable errors to the caller. Do not let an agent choose the top-level application control flow implicitly.

Start with a no-model version that returns deterministic data. Add one agent boundary and validate its result. Add a tool only after defining the host capability and policy. This incremental shape is easier to check, explain, test with a fake provider, and debug than a monolithic prompt-driven workflow.

\newpage

<!-- Page 16 -->
# Chapter 4 — Analysis, Types, and Diagnostics

## Page 16 of 50 · What `check` establishes

`camel check file.camel` lexes, parses, loads imports, validates declarations and names, and performs static checks on expressions it understands. It checks duplicate declarations, type names, configuration keys, parameter/default order, call arity, constructor field sets, control placement, known assignments and returns, call arguments, loop items, and boolean conditions.

It stops before evaluating source. No model, tool, file effect, or microphone runs. This makes `check` appropriate for quick feedback and CI analysis of source that should not execute. It is not a dry-run of the program's external behavior.

The analyzer cannot prove every non-void path returns, validate unknown host handlers, or reason through dynamic data behind `any`. Keep runtime checks even when static analysis is clean. A successful check is evidence about source properties, not a complete proof of safe or successful execution.

\newpage

<!-- Page 17 -->
## Page 17 of 50 · Gradual static typing

Camel infers known expression types across literals, bindings, records, arrays, operators, function and tool calls, and agent results. When inference is unclear, the value can remain `any`. `var` preserves a known inferred type but does not conjure information from a provider response or dynamic host value.

Treat `any` as an explicit boundary. Convert or validate data as it enters domain logic. Bind agent output to a declared record, check invariants that types cannot express, and handle malformed responses as errors. Static analysis accepts `any` where a concrete type is expected; runtime binding, call, assignment, and return checks still apply.

Record schemas are nominal to static analysis and exact-shape at runtime. An unnamed object is not a named record merely because its fields look similar. Name important contracts and construct the intended record explicitly. This makes model schemas, function interfaces, and serialized outputs easier to review.

\newpage

<!-- Page 18 -->
## Page 18 of 50 · Error recovery and diagnostics

The shared analyzer returns diagnostics with severity, phase, stable rule code, token source range, related locations, and optional help. Syntax analysis currently stops at its first syntax error. Semantic and type checks can collect independent issues across declarations and top-level statements, so a user can fix several errors in one edit cycle.

The CLI analyzes files; Studio analyzes editor buffers. They use the same runtime analyzer. The shared contract is code, message, severity/phase, and range; file labels can differ by API. Studio groups Problems by phase, highlights the source span, and navigates from a diagnostic to its location.

When reporting a diagnostic defect, include the source, expected location, observed code/message, and whether it occurred in CLI, Studio, or the in-memory API. Avoid asserting that every presentation string is identical if the source label is API-specific.

\newpage

<!-- Page 19 -->
## Page 19 of 50 · Warnings and lint

The warning-only linter identifies unused locals, parameters, declarations, and imported declarations. Warnings do not block execution. This keeps cleanliness feedback separate from program correctness: remove accidental dead names, or deliberately retain them with clear context where the current lint supports no suppression mechanism.

An unused import may indicate stale architecture or a missing call. An unused parameter can reveal an incomplete implementation or a contract maintained for a caller. An unused local often means the workflow computes a value it never consumes. Read the code path before deleting anything; a declaration may exist to establish a tool, agent, or configuration boundary.

The 0.3.0 readiness work includes more diagnostic fixtures for imported-file behavior and presentation. Stable shared diagnostic codes, ranges, phases, and analyzer parity are already present in the current source. The remaining gate is broader coverage and validated CLI/Studio presentation, not the invention of diagnostics.

\newpage

<!-- Page 20 -->
## Page 20 of 50 · Analysis parity in practice

Use the same failing and passing sources across file, buffer, and source analysis APIs. Compare diagnostic rule codes, messages, severity and phase, and source ranges. Do not compare API-specific filenames as if they were part of a universal equality contract. Cover errors and warning-only lint; include imported declarations as the 0.3.0 work closes that gap.

For an author, the useful loop is: save or edit; check; fix the first syntax issue; rerun analysis; then run tests before executing the workflow. If diagnostics disagree across CLI and Studio, capture the smallest source file and the matching command or UI action. This separates a language-analysis defect from an editor rendering defect.

Do not make CI pass by weakening valid diagnostics without a fixture explaining the contract. Keep a positive example for accepted syntax and a negative fixture for a rejected program. Prior edition fixtures should remain, so future language changes do not silently redefine prior code.

\newpage

<!-- Page 21 -->
# Chapter 5 — Workflows, Tasks, and Control Flow

## Page 21 of 50 · Entry workflows and inputs

A workflow is the unit users run. The default name is `Main`; `--workflow Other` chooses another workflow. Typed parameters describe the input boundary, with defaults permitted only as trailing optional values. CLI arguments after `--` bind positionally. Arrays and record values use JSON input; strings are passed literally.

Use one workflow to validate inputs, orchestrate computation, and return a declared result. Keep separate workflows for materially different entry paths, such as microphone capture and file-based audio processing. The `check` command does not require the entry workflow's arguments and can analyze imported libraries without selecting a runnable workflow.

With `--json`, output contains a value and logs. Logs are buffered and presented after a successful run. Treat standard output as a machine-readable interface when using JSON mode; keep human diagnostics and trace behavior on their documented streams.

\newpage

<!-- Page 22 -->
## Page 22 of 50 · Branches, loops, and exceptions

Camel supports `if`/`else`, `while`, `for`, and `foreach`, with `break` and `continue` inside loops. Conditions are boolean. Use loops for explicit bounded work and validate collection sizes before processing them. Do not assume a hidden scheduler, task queue, or implicit retry loop.

`throw value;` raises a source-located error. `catch (error)` binds an object with `message`, `file`, and `line`. `finally` runs during ordinary unwinding, and a return or error inside `finally` can override an earlier outcome. Budget exhaustion is fatal and may prevent a language-level `finally` from running; host resources need host-owned cleanup.

Catch errors you can handle meaningfully. Add context when rethrowing, but preserve actionable information. Avoid converting every failure into an empty result: model transport failure, malformed typed output, denied tools, and application validation errors need distinct handling.

\newpage

<!-- Page 23 -->
## Page 23 of 50 · Spawn and await

`spawn expression` snapshots current local bindings and starts a task. `await task` returns a cached result or raises the cached error. Awaiting the same task again does not repeat its work. Use a task to express actual parallel work; awaiting a value that is already computed simply returns it.

```camel
Task<AgentResult> a = spawn Analyst.run(question);
Task<AgentResult> b = spawn Skeptic.run(question);
AgentResult left = await a;
AgentResult right = await b;
```

The tasks do not share mutable local variables with the parent. They do share host services, logs, and execution fuel. Log order and external-effect order can vary under concurrency. This task lifetime belongs to one execution; it is not durable and not independently cancellable per block.

\newpage

<!-- Page 24 -->
## Page 24 of 50 · Structured task lifetime

The engine drains every spawned task before the workflow returns, including tasks whose values were discarded. An unobserved task failure propagates. If an `await` already observed an error, ordinary catch handling can process that error. Repeated awaits use the cached completion and do not trigger a second model call or tool effect.

Tasks share a per-execution step budget. Current defaults include 100,000 expression/statement/provider-step ticks, call depth 128, and 64 total spawned tasks. These bounds limit some forms of runaway execution but are not a wall-clock deadline, memory cap, or cancellation guarantee.

Design explicit result joins. Await each important task, preserve its typed result, and validate the combined output. If one branch can fail independently, handle the error deliberately. Do not assume starting parallel work automatically cancels sibling tasks when one fails; they are drained before execution exits.

\newpage

<!-- Page 25 -->
## Page 25 of 50 · A workflow design checklist

Before execution, define the workflow's input type and domain invariants. Decide which work is deterministic, which calls a model, which tools can cause effects, and which result the caller receives. Make the maximum task count and agent turns proportionate to the input. Keep a clear route for malformed results and provider errors.

During execution, compute cheap deterministic checks first. Run independent model calls in parallel only when shared provider capacity and effect ordering make that appropriate. Await the result of each branch. Assemble the final record in source code, not by trusting a final agent to invent omitted fields.

After execution, expose the returned value and enough logs to troubleshoot without leaking sensitive material. For external effects, make retries safe or explain why they are not. These practices remain relevant even if future Camel versions add cancellation or persisted replay; those features do not replace clear workflow ownership.

\newpage

<!-- Page 26 -->
# Chapter 6 — Agents, Providers, and Tools

## Page 26 of 50 · Declaring an agent

An agent declaration names a configured model interaction. `model` is required. Optional fields include `system`, `response`, `tools`, `policy`, `maxTurns`, `retries`, and `temperature`. Values are evaluated in global context when invoked; configuration expressions are trusted source code. There is no fixed built-in catalog of model names.

```camel
record Answer { string text; }
agent Helper {
    model = env("CAMEL_LM_MODEL", "local-model-id");
    system = "Answer only from supplied evidence.";
    response = Answer;
    maxTurns = 3;
    retries = 1;
    temperature = 0;
}
```

Defaults are empty system prompt, no response schema, no tools, deny-by-default policy, four successful turns, zero retries, and temperature zero. Turn limits are 1–128; retries 0–3; temperature 0–2. Keep these bounds small for predictable runs.

\newpage

<!-- Page 27 -->
## Page 27 of 50 · Prompts and typed results

An agent accepts exactly one prompt. A string becomes a user message. A JSON-serializable object becomes JSON text. An audio prompt uses an object containing text and audio data; a path string is not automatically loaded as audio. Load or record audio explicitly, then pass the resulting value.

When `response` names a record, Camel includes its schema in the system message and validates the returned JSON locally. Invalid JSON or invalid fields fail; there is no fallback to raw plaintext. Without a response schema, `AgentResult.data` is a string. The result also provides raw final `text`, `model`, and `toolCalls`.

Structural validation proves only that data fits the declared shape. It does not prove that claims are true, evidence is complete, or the model followed the intended reasoning. Use fields for uncertainties and evidence, then apply deterministic checks before taking consequential action.

\newpage

<!-- Page 28 -->
## Page 28 of 50 · Model calls and retries

The current CLI uses an OpenAI-compatible local HTTP adapter. `CAMEL_LM_ENDPOINT` defaults to `http://localhost:1234`; root and `/v1` URLs normalize to the chat-completions endpoint. Optional tokens come from `CAMEL_LM_TOKEN` or `LM_API_TOKEN`; `CAMEL_LM_TIMEOUT_SECONDS` defaults to 300. Model identifiers come from source, often through `env`.

`maxTurns` counts successful model turns, including turns that request tools. `retries` adds provider attempts for transport/provider failures on each turn. Malformed final output and tool failure are not automatically retried. Per invocation, provider attempts are bounded by `maxTurns × (retries + 1)`, plus shared execution fuel.

Previously executed tool effects are not repeated just because a later provider request retries. Still, host handlers should define idempotence for their own effects. Live model results are nondeterministic; fake-provider tests verify protocol and handling, not quality or provider compatibility.

\newpage

<!-- Page 29 -->
## Page 29 of 50 · Tools and host capabilities

A Camel `tool` declaration describes typed arguments, a typed return, a description, and a registered handler name. A handler name selects host code; it is not an arbitrary shell command. The host registers handlers through `ToolRegistry` with a capability such as pure computation, file read, file write, or network access.

Standard handlers include `text.length`, `json.stringify`, `files.read`, `files.write`, and `http.get`. No shell handler is shipped. The request client supports HTTP and HTTPS, and HTTP tools use a 30-second timeout. File tools resolve relative paths from the entry source directory and check that canonical paths remain within it.

Keep schemas narrow and descriptions concrete. Give one handler one effect boundary. Model calls can request a tool, but only the host handler executes it. Treat arguments as untrusted, validate them in the handler, and define repeated-call behavior for operations where a retry or second request could duplicate an effect.

\newpage

<!-- Page 30 -->
## Page 30 of 50 · Agent tool loops

An agent tool loop is bounded by its policy, registered capabilities, call budget, turn cap, and shared runtime fuel. Before the first provider request, Camel checks that offered tools are permitted by the policy and available from the registry. Before dispatch, it validates call names, unique IDs, and typed argument objects. Every dispatched call passes through the registry.

The tool-call batch is not a transaction. A handler that succeeds before a later handler fails has already committed its effect. Duplicate call IDs are rejected within the invocation, but new IDs with equal arguments are not deduplicated. Design handlers accordingly.

The conversation retains assistant and tool messages with their IDs for later model turns. Provide only the tools necessary for a task. Prefer a read-only initial policy, narrow filesystem/network scope, and a zero tool-call budget if the agent only needs reasoning. Tool access belongs to a host trust decision, not to the model.

\newpage

<!-- Page 31 -->
# Chapter 7 — Safety, Effects, and Trust

## Page 31 of 50 · Policies are one boundary

Policies name allowed tools, capabilities, per-invocation call limits, and flags for network, file read/write, and shell capabilities. Defaults deny tools and capabilities, with `maxToolCalls = 0`. Before sending a request that offers tools, the runtime checks the policy against registered capabilities. A model cannot make an unregistered handler appear.

The policy governs **model-requested** tools. Trusted workflow source can call declared tools directly and can read environment variables, record audio, or provide audio to a model. `allowNetwork = false` does not block the configured model endpoint. A policy is not an operating-system sandbox or a boundary against malicious source.

Explain this distinction in deployment documentation. Restrict who can author and run trusted workflows. Use host-level permissions and separate accounts for stronger isolation; do not claim that a Camel policy contains arbitrary code or prevents prompt injection.

\newpage

<!-- Page 32 -->
## Page 32 of 50 · File and network effects

The standard file handlers are rooted at the entry source directory, not the imported module directory or process working directory. Canonical path checks reject escapes, including resolved symlinks and junctions. Write parents must exist. A write stages and syncs complete bytes before replacing one target file; a repeated write intentionally replaces that file.

That guarantee is scoped to a single target and the host's staging/replace operation. It is not an OS-grade sandbox and does not defend against every concurrent external filesystem change. A tool batch is not atomic across handlers. Earlier successful operations are not undone if a later operation fails.

`http.get` can reach HTTP or HTTPS endpoints through the registered request client. A denied model-tool capability does not imply that the trusted workflow or configured inference endpoint cannot use the network. Document endpoints and data movement explicitly; never expose secrets or personal information in prompts unnecessarily.

\newpage

<!-- Page 33 -->
## Page 33 of 50 · Secrets and configuration

Use environment variables for provider tokens, endpoints, and machine-specific model IDs. `env("CAMEL_LM_MODEL", "local-model-id")` supplies a fallback when the variable is absent. Do not commit secrets, paste them into help channels, or include them in logs, screenshots, example source, or bug reports.

Keep configuration names specific to their purpose. The audio example uses separate model variables for transcription and summarization; these names have no special interpreter meaning. In Studio, tokens are inherited by workers but are not displayed or stored by the editor. Secure the host account and environment that supply them.

Model names are identifiers chosen by the program and provider. An installed model is not proof that the serving endpoint supports a requested payload. In particular, audio payload construction and live model/server acceptance are separate facts. Test adapters without credentials using fake providers, then separately validate the intended trusted endpoint.

\newpage

<!-- Page 34 -->
## Page 34 of 50 · Audio and other host resources

`record(seconds)` captures from the default microphone into in-memory PCM16 WAV. Durations range from one to 3,600 seconds; zero means stop on Enter. CLI record flags can override a script duration. `audio.load(path)` validates that a supported-extension file is nonempty and base64-encodes it; it does not validate codecs or resample audio.

Recording is record-then-process, not streamed transcription. The backend's native sample rate and channel count are preserved. Relative audio paths prefer the process working directory and then the source directory; absolute paths are accepted. Operating-system microphone permissions and device availability apply.

Host resources require host-owned cleanup. Language-level `finally` is not guaranteed to run after fatal budget exhaustion. A future cooperative cancellation protocol would need explicit ownership and completion behavior; current Studio Stop terminates the worker process and does not undo completed effects.

\newpage

<!-- Page 35 -->
## Page 35 of 50 · Safe workflow review

Before running a workflow, identify every effect: provider request, tool call, file access, network request, microphone capture, and log output. Identify which are initiated by trusted source and which can be requested by a model. Verify the host registration, capability, policy, path scope, timeout, and retry behavior for each effect.

Before asking a model to act, narrow its task and context. Treat its response and tool arguments as untrusted. Validate structure and domain facts. Never grant a tool merely because a prompt says it is necessary. Keep the model's capabilities smaller than the workflow's trusted capabilities where possible.

For consequential work, do not claim human approval protection in 0.2: there is no runtime host callback for approving direct workflow effects. Per-run capability-set approval is a selected 0.3 design direction but remains a release gate. Keep it labeled planned until implemented, tested, and documented.

\newpage

<!-- Page 36 -->
# Chapter 8 — Runtime, CLI, and Studio

## Page 36 of 50 · Installation and local setup

The installation guide describes extracting the operating-system and CPU-specific setup archive, launching the native wizard, selecting an installation folder, and optionally adding commands to the user's `PATH`. Open a new terminal after setup and run `camel --version`. Keep the extracted distribution's directory structure intact.

The repository currently describes the preview distribution as private until access is authorized. Do not publish the installer link as public access. Developers building from source need Rust 1.88 or newer; Linux builds require ALSA development headers and the GUI requires desktop/OpenGL libraries.

For a source checkout, follow the commands in `runtime/README.md`. The runtime, Studio, library manager, and installer have separate manifests and lockfiles; use `--locked`. If a local toolchain fails to link, distinguish that environment problem from a Camel source failure before reporting a product defect.

\newpage

<!-- Page 37 -->
## Page 37 of 50 · The command line

Common commands:

```text
camel check file.camel
camel file.camel --json -- arg1
camel --workflow Other file.camel
camel record file.camel --seconds 30
camel --gui [file.camel]
camel library list
```

Use `camel check` for syntax, names, edition, known expression types, and diagnostics without executing the workflow. `--workflow` selects a named workflow. `--json` prints a JSON result object; arguments following `--` bind to workflow parameters. `--trace` exposes execution events and `--steps` sets a step limit.

Environment configuration includes `CAMEL_LM_ENDPOINT`, `CAMEL_LM_MODEL` or model-specific names, optional token variables, and `CAMEL_LM_TIMEOUT_SECONDS`. The CLI normalizes an endpoint root or `/v1` path. Confirm the local server is reachable and serves the requested model and input type.

\newpage

<!-- Page 38 -->
## Page 38 of 50 · Studio as an editor

Camel Studio is a native Rust code editor and workflow runner, not a visual graph editor. It opens Camel source, provides syntax and analysis feedback, supports workflow inputs, and can inspect declarations and execution events. A fresh launch shows a starter document; shipped examples are not silently made the user's default script.

Open a `.camel` file, resolve diagnostics in Problems, choose a workflow and inputs in Setup, and press F5 to run. The Runs view reports workflow, agent, model-request, tool, and microphone events. Console shows logs and worker status; Result shows returned JSON. Library Manager is under Tools.

Studio starts one hidden worker subprocess. Repeated Run while active is ignored. Stop terminates that worker; it does not cancel a request already accepted by a provider, undo file writes, unload models, or terminate the model server. Treat Studio as a client to the runtime, not a stronger sandbox.

\newpage

<!-- Page 39 -->
## Page 39 of 50 · Runtime ownership and embedding

The runtime owns parsing, AST representation, checking, imports, values, scopes, evaluation, agent protocol, task lifecycle, logs, and step fuel. `Host` provides model-provider and recorder injection, registered tool handlers, and host capabilities. The CLI parses arguments and presents outcomes; Studio owns the editor and launches a runtime worker.

An embedding host can compile a `CompiledProgram`, create an `Engine`, configure `Host`, register handlers through `ToolRegistry`, and call the run API. `ModelProvider::complete` returns an assistant-message JSON object with content and optional tool calls. A recorder can be injected for alternate backends or deterministic tests.

Each engine run creates fresh scopes, logs, tasks, and shared fuel. Define who owns the provider, recorder, tool registry, and external resources. Avoid hidden global mutable state across runs; explicit host injection makes trust and test boundaries reviewable.

\newpage

<!-- Page 40 -->
## Page 40 of 50 · Troubleshooting by layer

If `check` fails, read the phase and source range first. If execution fails before any provider request, inspect workflow arguments, imports, types, and host registrations. If an HTTP request fails, verify endpoint, token, timeout, server availability, and model identifier. If typed output fails, compare the exact JSON fields and value types to the record.

If Studio behaves differently from CLI, compare the same source and inputs using the shared analyzer and the same runtime build. For UI defects, record the window, action, expected result, and a screenshot with private data removed. For worker failures, include the worker status and relevant diagnostic, not a secret-bearing environment dump.

For audio, distinguish file loading or capture from provider support. Camel can construct `input_audio`, but that does not establish that a particular LM Studio model/server accepts the payload. Report the server and exact error; avoid claiming successful inference without a live run.

\newpage

<!-- Page 41 -->
# Chapter 9 — Libraries, Packaging, and Portability

## Page 41 of 50 · Source libraries

Camel libraries are source packages listed in `libraries/index.json`. Each catalog entry declares a name, description, semantic version, Git revision, repository, source path, and allowlist of files. The current catalog includes `standard` and `audio`. The reusable library manager provides discovery, install, update, removal, and installed-source lookup.

Discover with `camel library list` or `camel library search <query>`. Install or update by name. Use `CAMEL_HOME` to select the data root; defaults are under local application data on Windows and `~/.camel` on Unix. Programs import package source with `using "@camel/name/file.camel";`.

Remote installation requires access to the catalog and the declared source repository. In a private repository or restricted network, use local checkout imports or an explicit local catalog as appropriate. Do not tell users a remote install works until both catalog and package access have been tested.

\newpage

<!-- Page 42 -->
## Page 42 of 50 · Adding and updating packages

To add a library, create `libraries/<name>/` with Camel sources and optional documentation. Add a catalog entry and list every required file, including imported sibling modules. Keep names unique and paths within the package. Check the source and a small consumer program, then submit a contribution for review.

Bump the library version and Git revision when publishing a source update. Prefer a tag or commit revision for reproducibility; a moving branch is convenient during development but changes over time. The current manager downloads only allowlisted files from the declared revision and rejects path traversal.

Known limits: installed snapshots do not yet verify per-file content digests or solve transitive library dependencies. A version and revision identify declared source history but are not a content signature. Document what was tested and which runtime edition the library expects.

\newpage

<!-- Page 43 -->
## Page 43 of 50 · Packaging and upgrades

The packaging script builds an operating-system and architecture-specific distribution. It can include Studio and produces manifests with per-file SHA-256 values plus an archive hash. The installer validates payload hashes and stages upgrades with recovery behavior. Repeated installation is part of the distribution workflow.

These checks detect corruption and exercise packaging/upgrade paths; hashes are not publisher signatures. They do not by themselves prove that the publisher is authentic or that a package is safe. Cross-platform bundles must be built and checked on the relevant CI platforms. A local Windows rehearsal does not establish Linux/macOS behavior.

Never overwrite historical archives while rehearsing a new build. Keep a fresh output path and verify the manifest, archive digest, and repeated install behavior. Do not mark a package public or update version labels before the release gates are reviewed and authorized.

\newpage

<!-- Page 44 -->
## Page 44 of 50 · Reproducible builds

Runtime, Studio, installer, and the library manager have committed Cargo lockfiles. CI uses `--locked` to prevent dependency resolution from silently changing during a checked build. Keep each lockfile with its crate; there is no single workspace lockfile that controls all components.

Run formatting and Clippy checks for the component being changed, then its tests. Use the repository's documented locked commands. A failure in a system linker, Linux ALSA dependency, or GUI environment should be reported separately from a Rust diagnostic attributable to Camel source.

For source examples, include the Camel language edition and required environment variables. For libraries, record semantic version and revision. For distributions, retain both archive and file-manifest digests. Reproducibility is a collection of explicit inputs; a lockfile alone does not pin model weights, provider behavior, or remote content lacking a digest.

\newpage

<!-- Page 45 -->
## Page 45 of 50 · Portability and boundaries

The source runtime is Rust-based and the installer targets Windows, macOS, and Linux. Native audio and GUI dependencies vary by platform. CI configuration covers multiple operating systems, but each platform's built bundle still needs its CI evidence. A local test on Windows is not a claim that macOS signing or Linux desktop integration has been validated.

Model behavior is also environment-specific. Model IDs, endpoint payload support, context limits, and audio acceptance depend on the provider and loaded model. Keep provider configuration outside portable source when practical, and include a known-good no-model example so language evaluation does not require external services.

Camel workflows are trusted code. File path checks and tool policies reduce accidental scope; they do not isolate the runtime from the operating system, arbitrary host callbacks, remote endpoints, or prompt injection. State that limit wherever packages or samples invite others to run code.

\newpage

<!-- Page 46 -->
# Chapter 10 — Verification and the 0.3.0 Path

## Page 46 of 50 · Conformance is the language contract

Each `conformance/<edition>/manifest.json` lists source fixtures and expected compile outcomes. Positive fixtures state accepted syntax and static behavior; negative fixtures make rejection part of the contract. Run the current suite with `cargo test --manifest-path runtime/Cargo.toml --locked --test conformance`.

Add a fixture when changing grammar, edition checks, names, static typing, or diagnostics. Keep older edition fixtures so a new release does not silently change the meaning of prior programs. Keep the manifest schema version distinct from the Camel language edition.

Conformance does not replace interpreter tests. Add runtime tests for values, errors, limits, tasks, handlers, and side effects. Use fake model providers and recorders for deterministic protocol coverage; separately identify what those fakes cannot establish about live providers or hardware.

\newpage

<!-- Page 47 -->
## Page 47 of 50 · Test the boundaries

Test the ordinary programming path without a model. Test typed agent success and malformed responses with a fake provider. Test that denied capabilities fail before dispatch, that tool arguments and IDs are validated, and that repeated awaits do not repeat work. Use temporary directories for imports and file effects; assert failure does not replace a typed binding or partial target file.

For task tests, cover normal completion, task errors, repeated await, discarded tasks, and the total spawned-task limit. Parallel completion and log/effect order may vary; assert contracts, not a particular race ordering. For diagnostics, compare file, buffer, and source API results where the shared contract applies.

Tests using fake providers prove runtime protocol and deterministic handling. They do not prove model accuracy, external server compatibility, microphone permission, or the safety of malicious workflows. Keep those evidence classes separate in release notes.

\newpage

<!-- Page 48 -->
## Page 48 of 50 · Current 0.3.0 release gates

The source candidate and Windows bundle identify as v.0.3.0. A release tag and publication remain gated on hosted Windows CI. Focused regression tests cover imported-file diagnostics, CLI/Studio presentation, checkpoints, replay reports, provenance, and source-library locks. The language edition remains 0.2 for this product version.

Execution needs also remain open: Studio Stop is worker termination, not cooperative cancellation; there are no resumable checkpoints or total run deadline. Tests use fake providers, but no persisted request/response replay or deterministic evaluation report exists. Token usage is not reported, and elapsed time is observational rather than a budget.

Handoffs lack provenance-bearing evidence bundles and a shared evidence schema. Providers do not negotiate capabilities before requests. Source libraries lack content digest verification and transitive dependency locks. These are visible limits today, not hidden 0.3 features.

\newpage

<!-- Page 49 -->
## Page 49 of 50 · A practical release checklist

Before calling the release 0.3.0, maintainers should close each readiness row with implementation evidence, automated coverage, operational documentation, and cross-platform CI where required. For any new approval boundary, identify every effect and ensure direct workflow calls and model-requested calls follow the intended host policy. For cancellation, state what is interrupted and what completed effects remain.

For replay, persist enough request and response data to make its scope explicit, protect sensitive values, and produce a report whose deterministic claims can be reproduced. For token budgets, distinguish actual provider-reported usage from estimates. For evidence, define provenance and how it survives task handoffs. For capability negotiation, specify provider support and failure behavior before a request is made.

For libraries, verify content identity and dependency resolution. For packaging, rebuild and install on supported systems. Update version labels only after review and passing gates. No readiness claim should be inferred from a local rehearsal alone.

\newpage

<!-- Page 50 -->
## Page 50 of 50 · Where to start

For a new learner, run the ordinary programming example, read the language specification, and use Studio to inspect the same source. For an agent author, begin with one bounded agent, a record response, and a deny-by-default policy. Add one registered handler only after its capability and effect behavior are clear.

For a library author, start with a small source package, test a consumer import, list every required file, and pin a revision. For a runtime contributor, add conformance fixtures for language behavior and focused tests for execution behavior. For a release contributor, use readiness evidence rather than a version string as proof of completion.

The v.0.3.0 target makes agentic programs more observable, governable, reproducible, and reviewable while preserving ordinary programming as the foundation. Use this guide with the implementation and known limits recorded in the repository README.

## Source of truth

The primary references for this guide are `docs/LANGUAGE_SPEC.md`, `runtime/README.md`, `studio/README.md`, `docs/INSTALLATION.md`, `libraries/README.md`, `conformance/README.md`, the versioned fixtures, and the 0.3.0 readiness table in the repository `README.md`.
