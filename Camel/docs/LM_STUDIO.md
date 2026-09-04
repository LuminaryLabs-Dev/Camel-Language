# Camel and LM Studio

Camel treats LM Studio as its primary local inference and model-management runtime.

## Setup

Start LM Studio's server from the Developer tab or with:

~~~powershell
lms server start
~~~

The default endpoint is http://localhost:1234. Configure Camel with:

~~~powershell
$env:CAMEL_LM_ENDPOINT = "http://localhost:1234"
$env:CAMEL_LM_MODEL = "publisher/model"
$env:CAMEL_LM_TOKEN = "optional-api-token"
~~~

LM_API_TOKEN is also recognized.

Check connectivity:

~~~powershell
cargo run -- doctor
cargo run -- models
~~~

## Default chat mode

Default agents use POST /v1/chat/completions. This mode supports:

- full message history;
- custom Camel tools;
- parallel tool requests returned by a model;
- structured JSON output;
- inference parameters;
- streaming;
- multimodal text-and-image messages.

Camel preserves assistant tool-call messages, executes requested capabilities through ToolRegistry, appends matching tool responses, and continues until the model returns final content or the configured turn bound is reached.

## Native mode

Set mode = "native"; to use POST /api/v1/chat. This provides:

- stateful conversation IDs;
- LM Studio-installed MCP plugins;
- ephemeral remote MCP servers;
- native performance statistics;
- per-request context length;
- optional stored conversation state.

Native mode and custom Camel tools are intentionally separate because LM Studio's native endpoint exposes MCP integrations while custom function tools are handled by Chat Completions or Responses.

## Structured output

Assign a Camel record to an agent's response property:

~~~camel
record Decision {
    string choice;
    string rationale;
    double confidence;
}

agent Decider {
    model = env("CAMEL_LM_MODEL");
    response = Decision;
}
~~~

Camel generates an OpenAI-compatible response-format JSON Schema, parses message content, and validates the result before returning result.data.

## Model management

CLI:

~~~powershell
cargo run -- models
cargo run -- load "publisher/model" '{"context_length":16384,"flash_attention":true}'
cargo run -- unload "model-instance-id"
cargo run -- download "publisher/model"
cargo run -- download-status "job-id"
~~~

Language:

~~~camel
var models = lm.models();
var loaded = lm.load("publisher/model", new { context_length = 16384 });
lm.unload(loaded.model_instance_id);
~~~

Agents can use autoLoad, loadOptions, and autoUnload.

## Covered endpoints

- /api/v1/models
- /api/v1/models/load
- /api/v1/models/unload
- /api/v1/models/download
- /api/v1/models/download/status
- /api/v1/chat
- /v1/chat/completions
- /v1/responses
- /v1/embeddings

These follow LM Studio's current official API organization. See [LM Studio's developer documentation](https://lmstudio.ai/docs/developer).

## Failure behavior

- Requests have configurable timeouts.
- Agent calls have configurable retries with bounded backoff.
- Agent turns and tool calls have independent ceilings.
- HTTP status bodies are retained in LM Studio errors.
- Malformed structured output fails validation.
- Unknown or policy-denied tools never execute.
- Streaming tool-call fragments are reconstructed before dispatch.
