# Anchor support (optional)

Anchor support is **opt-in** via the `anchor` feature flag and is never a hard
dependency of the default build — native Solana stays first-class.

```sh
cargo build -p cu-profiler-cli --features anchor
```

## What it does

Given an Anchor IDL (the JSON your program emits), `cu-profiler`:

- **labels the program** by its IDL name, so reports show `amm` instead of a raw
  pubkey;
- exposes **instruction and account names** from the IDL (for upcoming
  instruction-level mapping);
- **decodes failure logs**: `custom program error: 0x1770` → `InvalidOwner`
  (with the IDL message, if present).

Both the pre-0.30 (`name` / `metadata.address`) and 0.30+
(`address` / `metadata.name`) IDL layouts are parsed.

## Configuration

```toml
[anchor]
idl = "target/idl/my_program.json"
```

With the binary built `--features anchor`, the program named in the IDL is
labelled automatically. Without the feature, the setting is ignored (the CLI
prints a note).

## Library use

```rust
use cu_profiler_core::anchor::AnchorIdl;
use cu_profiler_core::program_registry::ProgramRegistry;

let idl = AnchorIdl::from_json(&idl_json)?;
let mut registry = ProgramRegistry::with_builtins();
idl.apply_labels(&mut registry);            // program → human name
let err = idl.decode_error_reason("custom program error: 0x1770");
# Ok::<(), cu_profiler_core::Error>(())
```

## Limitations

- Instruction-name mapping needs transaction instruction data, which recorded
  logs do not contain; the IDL names are parsed and exposed, but per-instruction
  decoding waits on a live backend.
- Anchor event decoding is not yet implemented.
