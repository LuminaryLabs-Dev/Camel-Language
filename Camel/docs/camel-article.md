# How Camel Works Now

Camel is a small, specialized programming language for agentic applications. Its goal is specific: the orchestration layer that developers commonly write in Python or JavaScript should be expressible as a readable, testable Camel program, while model inference remains local through LM Studio.

That distinction matters. Camel is not trying to become a general-purpose replacement for Python. It does not need desktop GUI frameworks, scientific notebooks, or web application ecosystems. It does need the pieces that make a serious agent application: functions, structured data, control flow, errors, concurrency, tools, policies, memory, model routing, typed responses, observability, and a practical command line.

Version 0.1.0-alpha implements that language shape.

## Source looks like a program

Camel now follows familiar C# organization:

~~~camel
record Decision {
    string choice;
    string rationale;
    double confidence;
}

tool string Search(string query) {
    description = "Search the public web.";
    handler = "web.search";
}

agent Researcher {
    model = env("CAMEL_LM_MODEL");
    tools = [Search];
    response = Decision;
    temperature = 0.2;
}

workflow Main(string question) {
    AgentResult answer = await Researcher.run(question);
    log(answer.data.rationale);
}
~~~

Types precede names. Configuration uses assignments. Blocks use braces and semicolons. Camel deliberately avoids colon-oriented type or configuration syntax so source reads like application code rather than YAML, XAML, or XML.

## The compilation path

Running a Camel file passes through distinct stages:

~~~text
source files
    -> recursive import loader
    -> lexer
    -> parser and AST
    -> semantic validation
    -> compiled program image
    -> workflow interpreter
    -> agents and registered tools
    -> LM Studio
~~~

The import loader resolves using-file declarations relative to their source file, deduplicates imports, and reports circular chains. The lexer handles comments, escaped strings, multiline raw strings, interpolation, numbers, punctuation, and operators. The parser produces an abstract syntax tree for records, functions, tools, memories, policies, agents, workflows, statements, and expressions.

Semantic validation happens before side effects. It checks duplicate declarations, unknown types and symbols, invalid call arity, misplaced loop control, duplicate parameters and configuration fields, and broken references between agents, tools, records, memories, and policies. The check command runs this path without executing a workflow.

## The executable core

The interpreter provides the parts agent scripts repeatedly need:

- typed and inferred local variables;
- strings, integers, floating-point numbers, booleans, nulls, arrays, and objects;
- record validation;
- functions with defaults and lexical scopes;
- if/else, while, for, and foreach;
- return, break, and continue;
- try/catch/finally and throw;
- arithmetic, comparison, logic, null coalescing, assignment, member access, and indexing;
- JSON parsing and serialization;
- string and collection helpers;
- bounded loops.

Values are represented directly by the Rust runtime. Explicit local, parameter, return, record, and tool types are checked at runtime, while semantic checks catch symbol and declaration errors earlier.

## Workflows replace top-level scripts

A workflow is a named executable entry point. Main is the default:

~~~camel
workflow Main(string topic = "local AI") {
    log($"Researching {topic}");
}
~~~

Command-line arguments after -- are decoded as JSON when possible and passed to the selected workflow. This gives Camel files a predictable application boundary without relying on implicit global script execution.

## Tools are typed capabilities

Rust host tools implement a name-and-call interface and live in ToolRegistry. Camel gives those low-level handlers a typed public contract:

~~~camel
tool string ReadProjectFile(string path) {
    description = "Read a UTF-8 project file.";
    handler = "files.read";
}
~~~

When a workflow calls the tool, Camel validates arguments, dispatches through the registry, converts the returned string to the declared result type, and records lifecycle events.

When an agent receives the tool, Camel converts parameter types into an OpenAI-compatible JSON Schema. The model only sees tools listed by that agent. If the model requests one, Camel validates the JSON arguments and invokes the same registry path. The model never executes a Rust function, process, file operation, or network request itself.

The bundled CLI host includes safe relative file reading and writing, web search, HTTP GET/POST, lightweight HTML title/link extraction, and an allowlisted shell adapter. Embedders can register application-specific tools.

## Policies narrow authority

Capabilities are explicit, but production agents also need reusable limits:

~~~camel
policy SafeResearch {
    allowTools = [Search];
    maxToolCalls = 12;
    allowNetwork = true;
    allowFileRead = false;
    allowShell = false;
}
~~~

An agent policy can permit or deny named tools, cap total calls, block network or file adapters, require an explicit shell opt-in, and constrain file reads to configured roots. Policies are merged into agent configuration before execution, with the agent's own settings taking precedence.

Tool-call and turn limits are independent. A model cannot keep the runtime alive indefinitely by repeatedly requesting capabilities.

## Agents are runtime objects

An agent declaration supplies defaults for a reusable model-backed component:

~~~camel
agent Reviewer {
    model = env("CAMEL_REVIEW_MODEL", env("CAMEL_LM_MODEL"));
    system = "Find unsupported claims and missing risks.";
    maxTurns = 6;
    retries = 2;
}
~~~

Calling Reviewer.run(prompt) creates independent conversation state. Camel constructs the system and user messages, attaches declared tool schemas, applies inference settings, sends the request, executes tool calls, and loops until the model returns final content.

The result is a normal Camel object containing text, structured data, model, turn count, tool-call count, finish reason, and token usage. Agent output can flow through functions, records, other agents, memory, or the final workflow return value.

Default chat results also expose their complete history. Passing that history through the optional overrides object continues a stateless Chat Completions conversation. Native mode uses its response ID instead.

This is the important shift from the first prototype: an agent declaration no longer stores a fixed goal and symbolic output label. The goal is a runtime argument, and the output is a real value.

## Correct multi-turn tool use

OpenAI-compatible tool loops require both sides of a tool exchange in history:

1. the assistant message containing tool_calls;
2. one tool message for every call identifier.

Camel's LM Studio client preserves the complete assistant message before appending results. The earlier prototype retained assistant text but dropped its tool-call structure, which could make subsequent turns invalid or confusing to the model. The new wire types serialize and replay the complete exchange.

Tool errors are returned to the model as explicit error results, allowing it to recover within the same bounded run.

## Structured output uses records

Assigning a response record causes Camel to generate JSON Schema:

~~~camel
record Review {
    string verdict;
    string[] problems;
    double confidence;
}

agent Critic {
    response = Review;
}
~~~

The schema is sent through response_format. Camel parses the final message content as JSON and validates it against the record before exposing result.data. A malformed or incomplete response is a runtime failure instead of silently becoming loosely typed text.

## LM Studio coverage

Camel has two agent modes and a lower-level API surface.

Default chat mode uses /v1/chat/completions. It supports custom Camel tools, full assistant history, structured output, inference parameters, streaming, and text-plus-image prompts.

Native mode uses /api/v1/chat. It supports stateful response IDs, installed LM Studio MCP plugins, ephemeral remote MCP servers, per-request context length, and native performance statistics. Native mode uses LM Studio integrations; custom Camel tools remain on Chat Completions.

The LM runtime object and CLI also cover:

- model listing;
- model loading with configuration;
- model unloading;
- model downloading and status;
- embeddings;
- OpenAI-compatible Responses;
- native chat.

Authentication is optional and reads either CAMEL_LM_TOKEN or LM_API_TOKEN. Endpoints, request timeouts, retries, sampling parameters, context settings, automatic model loading, and automatic unloading are configurable.

If an agent has no model setting and CAMEL_LM_MODEL is absent, Camel queries LM Studio and selects a loaded language model when possible.

## Multimodal input

An agent prompt may be an object:

~~~camel
AgentResult result = await VisionAgent.run(new {
    text = "Explain the visual defect",
    images = ["screenshots/menu.png"]
});
~~~

HTTP URLs and data URLs pass through. Local image files are read relative to the source directory and converted to data URLs. Chat mode emits OpenAI-compatible image content; native mode emits LM Studio native image input.

## Persistent memory

Memory is a named store:

~~~camel
memory Project {
    provider = "file";
    path = ".camel/project.json";
}
~~~

Workflows can get, set, remove, enumerate, and clear values. Attaching memory to an agent injects its current snapshot into the system prompt and appends the completed exchange to a bounded conversation history. File access is synchronized across spawned work.

The current provider is JSON on disk. The declaration shape leaves room for SQLite, vector, or application-owned providers without changing agent syntax.

## Concurrency is explicit

Multi-agent work commonly fans out, then joins. Camel expresses that directly:

~~~camel
Task<AgentResult> research = spawn Researcher.run(topic);
Task<AgentResult> critique = spawn Critic.run(topic);

AgentResult first = await research;
AgentResult second = await critique;
~~~

spawn evaluates an expression on a worker thread with a cloned lexical environment and shared runtime services. await blocks for that task, caches the result for repeat awaits, and propagates runtime failures. This is enough to run independent local-model calls concurrently without forcing every function to become async.

## Observability

The runtime records structured events for workflow start/completion/failure, agent start/turn/completion, and tool start/completion/failure. The CLI can print these with --trace or include them with values and logs under --json.

This event stream is separate from user-facing log output. That separation makes tests deterministic and lets future hosts route telemetry to a UI, file, or tracing backend.

## Command-line experience

The executable is named camel:

~~~text
camel app.camel -- argument
camel check app.camel
camel models
camel load publisher/model
camel doctor
~~~

The original colon-oriented prototype is no longer accepted. Every Camel file runs through the v0.1 loader, validator, interpreter, and LM Studio runtime automatically; there is no compatibility parser or LM switch.

## What remains intentionally bounded

Camel is now a coherent application language, but the alpha is still small. It does not implement classes, inheritance, arbitrary native packages, a bytecode VM, a package registry, editor language servers, or every C# collection API. Those are not prerequisites for its agentic purpose.

Areas that still need hardening before a public stability promise include source-level static type inference, richer collection transformations, pluggable memory backends, production sandbox hosts, package signing, cancellation tokens, debugger integration, and compatibility guarantees.

The implemented center is already concrete: C#-shaped source becomes a validated program; workflows execute normal control flow; agents call LM Studio; tools cross a typed and policy-checked boundary; memory persists; concurrent work joins; and every important transition is observable.

Camel is released under the MIT License.
