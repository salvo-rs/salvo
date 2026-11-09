# Downstream and OpenAPI contracts

Run `python .github/scripts/check_macro_contracts.py` after installing
`jsonschema==4.26.0`. The fixture is an independent workspace under the target
directory returned by Cargo metadata. Its framework and OpenAPI dependencies
are renamed, covering public handler, Extractible, endpoint and ToSchema macros.
Two compile-fail consumers check unsupported flatten and generic impl diagnostics.

The emitted document is checked with the official OpenAPI 3.1 JSON Schema:
https://spec.openapis.org/oas/3.1/schema/2022-10-07

The vendored `openapi-3.1.json` SHA-256 is
`da01ba28852cac0de53893797cb8d1942bc3b05084f526dcc216717dec314ed0`.
It is distributed under the upstream Apache 2.0 license in `OpenAPI-LICENSE`.
Intentional invalid schema/content and multi-media header mutations must fail
validation. The normative example/examples exclusion is asserted separately;
the official schema does not encode that constraint. This is a representative
generated document check, not a claim to validate embedded JSON Schema semantics
or every OpenAPI document the library can express.

CI preserves the generated document, dependency lockfile and compiler diagnostics
on failures as well as success. No crate registry publication is involved.
