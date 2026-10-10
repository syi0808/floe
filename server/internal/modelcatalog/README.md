# Local model capability data

`model-catalog.json` has two data roles. Provider/model suggestions and the
existing per-model `metadata.capabilities` are descriptive. Inference uses only
the separately versioned `capability_evidence` object.

The evidence contract is version 1. Each entry binds an exact `provider_id`,
`model_id`, and canonical absolute HTTP(S) `endpoint`. The scheme and host are
lowercase, default ports are omitted, and trailing path slashes are removed;
non-default ports and paths remain significant. Endpoints cannot contain
userinfo, a query, a fragment, or a wildcard. Each fact is `chat`,
`structured_output`, or `tool_proposals`, has an explicit `supported` or
`unsupported` value, and includes a source and RFC3339 verification time. A
missing contract, entry, or fact means `unknown`; do not write `unknown` as a
fact. An endpoint listed for another provider or model does not match. The UI
displays unknown states, and execution admission requires positive evidence for
chat plus every requested feature. Adapter protocol support is an upper bound:
it can make a feature unsupported, but cannot establish model support.

`example.json` contains a synthetic, non-routable `fixture.invalid` model and
synthetic provenance. It demonstrates the format and makes no live provider
claim. Copy the format into a local data file, replace the synthetic identity
and provenance only with verified evidence for that exact endpoint/model, then
validate and install with the same local binary:

```sh
./floe-server --validate-model-catalog /path/to/model-catalog.json
FLOE_SERVER_DATA=/absolute/profile/path ./floe-server --install-model-catalog /path/to/model-catalog.json
```

The existing atomic install, periodic reload, last-good, and explicit rollback
flow validates the complete file before publication. No provider discovery or
network fetch is performed. Removing or deprecating a suggestion does not
remove a configured model; model IDs remain enterable when absent from the
suggestion list. Display names, suggestion order, and descriptive token limits
do not change the capability commitment. A change to effective capability
states does. Trailing literal slashes are normalized because provider adapters
trim them before appending API paths. Escaped path separators and escaped
percent signs remain escaped in endpoint identity, so distinct request paths do
not share evidence.
