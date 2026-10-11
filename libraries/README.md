# Camel public library catalog

[index.json](index.json) lists installable Camel source libraries maintained through this repository. The [camel-library-manager package](../packages/camel-library-manager/README.md) provides discovery and installation for host tools.

| Library | Description | Source |
| --- | --- | --- |
| `standard` | String joining helper. | [core.camel](standard/core.camel) |

## Add a library

1. Create `libraries/<name>/` containing Camel source and optional documentation. Use a unique name made from letters, numbers, hyphens, or underscores.
2. Add an entry to [index.json](index.json) with the following fields. List every required source file; include imported sibling modules too.
3. Check each source with Camel, then submit a pull request. Maintainers expand the public catalog by merging accepted entries.

```json
{
  "name": "standard",
  "description": "String joining helper.",
  "repository": "https://github.com/LuminaryLabs-Dev/Camel-Language",
  "path": "libraries/standard",
  "files": ["core.camel"]
}
```

The document uses `schemaVersion: 1` and a `libraries` array. Paths use forward slashes and may not contain parent traversal. Names are unique; file destinations are unique ignoring case. The installer downloads only listed files from `main`. Additions become discoverable from GitHub once merged.

## Use a library

With the source-backed Camel CLI:

```text
camel library search standard
camel library install standard
```

```camel
using "@camel/standard/core.camel";

workflow string Main() {
    return JoinWith(["Camel", "libraries"], " ");
}
```

`check` and execution resolve installed libraries offline. Imported declarations share global names with the calling program, so contributors should avoid common or conflicting declaration names. Packages currently have no versions, transitive dependency solver, or namespaces.
