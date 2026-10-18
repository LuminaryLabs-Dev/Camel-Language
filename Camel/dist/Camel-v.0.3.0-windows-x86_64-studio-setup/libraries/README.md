# Camel public library catalog

[index.json](index.json) lists Camel source libraries maintained through this repository. Remote installation requires access to the catalog and each library's GitHub source. The [camel-library-manager package](../packages/camel-library-manager/README.md) provides discovery and installation for host tools.

| Library | Description | Source |
| --- | --- | --- |
| `standard` | String joining helper. | [core.camel](standard/core.camel) |
| `audio` | Audio transcription and topic-focused note summarization agents. | [note_summarizer.camel](audio/note_summarizer.camel) |

## Add a library

1. Create `libraries/<name>/` containing Camel source and optional documentation. Use a unique name made from letters, numbers, hyphens, or underscores.
2. Add an entry to [index.json](index.json) with a semantic `version`, immutable full commit SHA `revision`, a SHA-256 checksum for each listed file in `fileChecksums`, and a `dependencies` array (empty when there are none). List every required source file; include imported sibling modules too. Bump the library version, commit revision, and checksums when publishing a source update; runtime releases are independent.
3. Check each source with Camel, then submit a pull request. Maintainers expand the public catalog by merging accepted entries.

```json
{
  "name": "standard",
  "description": "String joining helper.",
  "version": "1.0.0",
  "revision": "e7b770737516558652d38642583c6db184fd3fd4",
  "repository": "https://github.com/LuminaryLabs-Dev/Camel-Language",
  "path": "libraries/standard",
  "files": ["core.camel"],
  "fileChecksums": {"core.camel": "<64-character SHA-256>"},
  "dependencies": []
}
```

The document uses `schemaVersion: 3` and a `libraries` array. Revisions must be full immutable commit IDs; mutable branch names are rejected. Paths use forward slashes and may not contain parent traversal. Names are unique; file destinations are unique ignoring case. The installer downloads only listed files and verifies each digest before replacing an installation. Digests detect content drift or corruption but do not authenticate the publisher.

## Use a library

With the source-backed Camel CLI:

```text
camel library search standard
camel library install standard
camel library update audio
camel library lock standard audio --output camel.lock
camel library verify-lock camel.lock
camel library install-locked camel.lock
```

```camel
using "@camel/standard/core.camel";

workflow string Main() {
    return JoinWith(["Camel", "libraries"], " ");
}
```

`check` and execution resolve installed libraries offline. `update` reads the latest catalog from GitHub by default; use `--catalog <path-or-url>` for offline catalog snapshots. `lock` resolves exact versions and writes deterministic dependency-first package metadata to `camel.lock`; `verify-lock` validates it without network access; `install-locked` installs pinned sources and verifies their checksums. Lock installation processes packages in dependency order, but multiple downloads are not a single transaction: a failure can leave earlier packages installed, and rerunning is safe. Imported declarations share global names with the calling program, so contributors should avoid common or conflicting declaration names. Namespaces are not implemented.
