# Third-party notices

The OBJ plugin's own code is original, and it uses no third-party crates (its byte
search and float text come from Kinogaki's lib-misc-utils).

The test fixtures `tests/fixtures/{numbers,polygons,mixed,accents,bad_utf8}.obj`
and their generator `generate_fixtures.py` come from luce-obj
(github.com/dymokomi/luce-obj, MIT OR Apache-2.0, Copyright (c) 2026 Dy Mokomi),
as do the shapes of the generated `large`, `mixed_crlf` and `chunked` test inputs.
