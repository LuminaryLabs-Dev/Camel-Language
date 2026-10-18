# Camel public library catalog

[index.json](index.json) lists Camel source libraries maintained through this repository. Remote installation requires access to the catalog and each library's GitHub source. The [camel-library-manager package](../packages/camel-library-manager/README.md) provides discovery and installation for host tools.

| Library | Description | Source |
| --- | --- | --- |
| `standard` | String joining helper. | [core.camel](standard/core.camel) |
| `audio` | Audio transcription and topic-focused note summarization agents. | [note_summarizer.camel](audio/note_summarizer.camel) |

## Add a library

1. Create `libraries/<name>/` containing Camel source and optional documentation. Use a unique name made from letters, numbers, hyphens, or underscores.
2. Add an entry to [index.json](index.json) with a semantic `version` and Git `revision`. List every required source file; include imported sibling modules too. Bump the library version and revision when publishing a source update; runtime releases are independent.
3. Check each source with Camel, then submit a pull request. Maintainers expand the public catalog by merging accepted entries.

```json
{
  "name": "standard",
  "description": "String joining helper.",
  "version": "1.0.0",
  "revision": "main",
  "repository": "https://github.com/LuminaryLabs-Dev/Camel-Language",
  "path": "libraries/standard",
  "files": ["core.camel"]
}
```

The document uses `schemaVersion: 2` and a `libraries` array. Paths and revisions use forward slashes and may not contain parent traversal. Names are unique; file destinations are unique ignoring case. The installer downloads only listed files from each library's declared revision. Prefer a tag or commit revision for a reproducible release; `main` is useful while developing.

## Use a library

With the source-backed Camel CLI:

```text
camel library search standard
camel library install standard
camel library update audio
```

```camel
using "@camel/standard/core.camel";

workflow string Main() {
    return JoinWith(["Camel", "libraries"], " ");
}
```

`check` and execution resolve installed libraries offline. `update` reads the latest catalog from GitHub by default; use `--catalog <path-or-url>` for private catalogs or offline catalog snapshots. Imported declarations share global names with the calling program, so contributors should avoid common or conflicting declaration names. Transitive dependency solving and namespaces are not implemented.
