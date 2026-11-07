# OBJ reader and writer

The `obj` module of `kinogaki-obj` (`src/obj/`). It knows no SDK and
depends only on lib-misc-utils' `kinogaki-bytes` (line-feed search) and
`kinogaki-float-text` (shortest round-trip float text). No third-party
crates. Owned code forbids `unsafe`. Paths below are under `src/obj/`.

## Files

| File | Responsibility |
| --- | --- |
| `model.rs` | `Obj`, `Elements` (CSR rows of corners), `Run` (state statements), `Skipped` |
| `validate.rs` | `Obj::validate`: what the writer needs for an exact round trip |
| `error.rs` | `ObjError`, `Malformed` (kind, line, column); internal `Fault` (byte offset) |
| `limits.rs` | `Limits`, `ReadOptions` |
| `line.rs` | tokens of one line: separators, comments, numbers, the rest as a name |
| `number.rs` | exact decimal parsing (Clinger fast path, std fallback), index fields |
| `read/block.rs` | one block: split, count, size tables, parse chunks in parallel, merge |
| `read/count.rs` | the counting pass |
| `read/chunk.rs` | the parsing pass for one chunk |
| `read/stream.rs` | blocks of whole lines from files/readers, each read while the one before is parsed |
| `read/mod.rs` | public `parse`, `parse_with`, `read`, `read_file`, `read_file_with` |
| `write/mod.rs` | writer: validation, parallel formatting batches, statement interleaving |
| `write/format.rs` | shortest round-trip floats (kinogaki-float-text), integers |
| `mtl/` | MTL model, sequential reader, writer |

## Model

Vertex tables are flat (`positions`, `texcoords`, `normals`, optional
`weights` and `colors`, each empty or one per position). `faces`, `lines` and
`points` are `Elements`: `offsets` (one more than the element count),
per-corner position indices, and per-corner `vt`/`vn` indices that are empty
when no corner references one, else hold `NONE` where a corner has none.
Indices are 0-based and always resolved, whether the file used positive or
negative references.

`o`, `g`, `usemtl` and `s` change state for the elements after them. Each
becomes a `Run { face, line, point, value }`, the counts of each element kind
read before the statement; the value for an element is the last run starting
at or before it (`run_at`, `Obj::face_run`). Groups keep their names
space-separated in one value.

## Reading

A block is a run of whole lines: the whole text for `parse`, or one window of
a stream.

1. `split` cuts the block into chunks at line feeds: at least
   `ReadOptions::chunk_bytes` (256 KiB) each, at most 256. Every parallel
   pass runs on the SDK's workers (`parallel::map`), the host's, within its
   one budget: the reader starts no thread of its own, and with no workers
   installed (its own tests and tools install the SDK testing support's) it
   runs on the calling thread.
2. `count` runs on every chunk in parallel: line feeds, `v`/`vt`/`vn`/`f`
   records and face corners. Keywords are recognised exactly as the parsing
   pass recognises them.
3. Prefix sums give every chunk its first position, `vt`, `vn`, face and
   corner. Each table grows once to its final size and is cut into disjoint
   `&mut` slices per chunk (`split_at_mut`), so no locks or copies are needed.
4. `chunk::parse` runs on every chunk in parallel. References resolve against
   the global count of elements declared before each line (the chunk's base
   plus what it has read), exactly as a sequential reader would. Records that
   are rare or variable (weights, colors, `l`, `p`, statements, skipped
   keywords) go to a per-chunk `Extras`.
5. If any chunk stopped at a fault, the first chunk's fault wins, so the error
   is the first in file order whatever the threading; its byte offset becomes a
   line and column by counting line feeds in that block (only on failure).
   Otherwise the `Extras` merge in chunk order, shifting run positions and
   element offsets.

Corner `vt`/`vn` index tables exist from the first block that has any
`vt`/`vn`; `finish` drops them when no face referenced one, so a model never
carries a table full of `NONE`.

Streaming (`read`, and `read_file` above one block): 32 MiB buffers are
filled, each cut after its last line feed with the tail carried into the
next, so the parser always sees whole lines. Each step is two jobs on the
workers, the block read before parsed while the next is read; a parsed
block's buffer is the next one read into. A line longer than a block grows
the buffer up to `ReadOptions::line_bytes`. A cancelled read stops at the
next block, and no chunk starts parsing once it is cancelled. Line numbers continue across blocks from the counted
line feeds.

Numbers: `number::parse_f64` reads sign, up to 19 significant digits and the
exponent in one pass; with a mantissa within 2^53 and a decimal exponent
within ±22 one multiply or divide is correctly rounded (Clinger). Anything
else goes to `str::parse::<f64>` with the token, so every value is
bit-identical to the standard parser. Non-finite values (`inf`, `nan`,
overflow) are rejected.

## Writing

`write_with` validates the model first (so it never writes text that would
read back differently), then writes `mtllib`, `v`, `vt`, `vn` and the
elements. Each table is formatted in tasks of 16K records on the workers and
written in order, 32 tasks at a time, so memory holds about 25 MB of text. Statements of all four kinds are sorted by their element
position and interleaved with face, line and point ranges. Floats are the
shortest round-trip spelling without a trailing `.0`. `write_file` and
`mtl::write_file` replace the target whole or not at all, through the SDK's
`files::replace`.

## MTL

`mtl::parse` reads a library in one sequential pass (libraries are small):
each `newmtl` block's classic statements (`Ka`, `Kd`, `Ks`, `Ke`, `Tf`, `Ns`,
`Ni`, `d`, `Tr`, `illum`), the PBR extension's (`Pr`, `Pm`, `Ps`, `Pc`,
`Pcr`, `aniso`, `anisor`) and its maps, each map's options parsed into
`MapOptions` (`-o`, `-s`, `-mm`, `-clamp`, `-imfchan`, `-bm`; any other kept
verbatim with its values). `d` and `Tr` are kept apart, as written. A
statement in another form (`Kd spectral ...`, `d -halo ...`) and any unknown
statement are kept verbatim in `Material::other`. `mtl::to_bytes` writes the
statements in a fixed order and the options in the order above.

How the plugin maps MTL onto MaterialX networks (`src/materials/`) is in
the README.

## Differences from luce-obj

luce-obj (the Luce package this replaces) reads polygons into a luce-geocore
`Mesh`; this module reads the whole file into a plain model, and Studio's OBJ
plugin builds the standard `Geometry` from it.

| | luce-obj | `obj` |
| --- | --- | --- |
| Precision | positions stored as f32 offsets from an origin | every value exact f64 |
| Records | `v`, `vt`, `vn`, `f` | also `l`, `p`, vertex colors, weights kept |
| Statements | `o`/`g`/`s`/`usemtl`/`mtllib` accepted and dropped | kept as runs; `mtllib` paths kept; MTL read and written |
| Unknown keywords | error | skipped and reported |
| Comments | a comment after a 1-value `vt` makes its second word a value (error) | comments end any line |
| Invalid UTF-8 | whole file rejected | names read lossily, comments ignored |
| Errors | message only, from a slower reference decoder | kind, line and column from the fast path |
| Limits | 1 GiB, 8M points/faces, 32M corners | configurable; defaults 16 GiB, 2^30 records, 2^32−1 corners |
| Writing | positions, faces, per-corner `vt`/`vn` | the whole model, parallel, atomic |
