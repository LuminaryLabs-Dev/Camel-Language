# Camel language conformance

Each `conformance/<language-version>/manifest.json` lists source fixtures and the expected result of compiling them with the matching Camel runtime. The separate `schemaVersion` describes the fixture manifest format.

Run the current edition suite from the repository root:

```powershell
cargo test --manifest-path .\runtime\Cargo.toml --locked --test conformance
```

Add positive fixtures for supported behavior and negative fixtures for syntax, version, name, and static-type failures. A new language edition gets a new directory; keep prior fixtures to detect compatibility regressions.
