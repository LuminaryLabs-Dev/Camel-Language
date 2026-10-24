use camel_audio_runtime::language::CompiledProgram;
use serde::Deserialize;
use std::path::PathBuf;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Manifest {
    schema_version: u32,
    language_version: String,
    cases: Vec<Case>,
}

#[derive(Deserialize)]
struct Case {
    source: String,
    expect: String,
}

#[test]
fn edition_02_conformance_manifest() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../conformance/0.2");
    let manifest: Manifest = serde_json::from_slice(
        &std::fs::read(root.join("manifest.json")).expect("read conformance manifest"),
    )
    .expect("parse conformance manifest");
    assert_eq!(manifest.schema_version, 1);
    assert_eq!(manifest.language_version, "0.2");
    assert!(!manifest.cases.is_empty());

    for case in manifest.cases {
        let source = root.join(case.source);
        let result = CompiledProgram::load(&source);
        match case.expect.as_str() {
            "pass" => {
                assert!(result.is_ok(), "{}: {:?}", source.display(), result.err());
            }
            "fail" => assert!(result.is_err(), "{} unexpectedly passed", source.display()),
            expectation => panic!("unknown conformance expectation '{expectation}'"),
        }
    }
}
