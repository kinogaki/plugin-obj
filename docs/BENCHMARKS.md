# OBJ benchmarks

Hardware: Apple M4 Max (16 cores), macOS 15 (Darwin 24.6), Rust 1.98.1,
release profile (thin LTO, one codegen unit, overflow checks on). The machine
was shared with other build agents (load average 8–15 on 16 cores), so
numbers are the minimum and median of 5 runs (3 for slow or single-threaded
cases); files sit in the page cache.

Inputs (`cargo run --release --example bench -- generate DIR`): quad grids
with `%.6f` numbers, as common exporters write them.

| File | Faces | Points | Records | Size |
| --- | --- | --- | --- | --- |
| quad1000 | 1,000,000 | 1,002,001 | `v`, `f p p p p` | 57 MiB |
| quad1000_uvn | 1,000,000 | 1,002,001 | `v`, `vt`, `vn`, `f p/t/n ...` | 111 MiB |
| quad3163 | 10,004,569 | 10,010,896 | `v`, `f p p p p` | 611 MiB |
| quad3163_uvn | 10,004,569 | 10,010,896 | `v`, `vt`, `vn`, `f p/t/n ...` | 1,189 MiB |

## Reading

`read_file` (file to `Obj`, streaming); `parse` (text already in memory, all
threads); `parse 1 thread`. Times in ms, min / median.

| File | read_file | parse | parse, 1 thread |
| --- | --- | --- | --- |
| quad1000 | 12.5 / 12.6 | 9.3 / 9.6 | 89.3 / 89.7 |
| quad1000_uvn | 26.9 / 27.8 | 20.6 / 20.8 | 172.4 / 174.3 |
| quad3163 | 127.5 / 129.4 | 101.6 / 106.4 | 950.6 / 955.8 |
| quad3163_uvn | 260.3 / 261.8 | 213.8 / 218.0 | 1855.4 / 1880.9 |

That is about 4.6 GB/s from file and 6 GB/s from memory on 16 cores, and
0.65 GB/s on one.

## Writing

`to_bytes` (into memory) and `write_file` (atomic: temporary file, fsync,
rename). Times in ms, min / median.

| File | Text written | to_bytes | write_file |
| --- | --- | --- | --- |
| quad1000 | 51 MiB | 12.7 / 13.0 | 22.2 / 25.0 |
| quad1000_uvn | 99 MiB | 26.7 / 27.7 | 39.4 / 41.1 |
| quad3163 | 608 MiB | 145.6 / 150.1 | 169.4 / 180.4 |
| quad3163_uvn | 1,183 MiB | 333.5 / 339.0 | 371.7 / 402.9 |

## Against luce-obj

Same files, same machine and session. luce-obj 0.10.1 (`tools/luce-oracle`,
built with `luce-base build --native --release`): `Obj.load` reads, parses in
parallel and builds a luce-geocore `Mesh` (edges, triangulation, normals);
`Obj.encode` writes text from that mesh (per-corner `vt`/`vn`). Best of 3.

| File | luce-obj load | obj::read_file | luce-obj encode | obj::to_bytes |
| --- | --- | --- | --- | --- |
| quad1000 | 26.5 | 12.5 | 551.7 | 12.7 |
| quad1000_uvn | 56.2 | 26.9 | 2217.8 | 26.7 |
| quad3163 | — ¹ | 127.5 | — ¹ | 145.6 |
| quad3163_uvn | — ¹ | 260.3 | — ¹ | 333.5 |

¹ luce-obj refuses more than 8,388,608 points or faces (and 1 GiB of text).

luce-obj's load includes building its mesh, which the `obj` module leaves
to the plugin.

Reproduce: `cargo run --release -p kinogaki-obj --example bench -- generate DIR`,
then `bench read FILE...` and `bench write FILE...`; luce-obj with
`tools/luce-oracle` (`oracle time FILE...`).
