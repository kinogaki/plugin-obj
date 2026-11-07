# OBJ

Wavefront OBJ and MTL as a format plugin for [Kinogaki Studio](https://github.com/kinogaki): an OBJ file reads as one standard Geometry of the public [studio-sdk](https://github.com/kinogaki/studio-sdk), its material libraries as MaterialX networks, and polygons and polylines write as one OBJ with the Materials they name as an MTL library beside it. The plugin holds the reader and writer themselves (`src/obj/`, which uses nothing of the SDK but the read's budget) and the conversion between their model and the SDK's values (`src/read.rs`, `src/write.rs`, `src/materials/`).

The library crate is `kinogaki-obj`; `plugin/` (`kinogaki-plugin-obj`) is the dylib Studio loads, `formats/obj.dylib`. Package `kinogaki.obj`, format `kinogaki.format.obj`.

## Reading

- **Geometry:** polygons then polylines over the file's points, vertex `uv` and `N`, point `Cd` (the common `v x y z r g b` extension, read as sRGB), primitive `name` (by `o`, or by `g` with the Pieces option), `material` (`usemtl`) and primitive groups (`g`), placed from the file's units and up axis (options; Y up in metres by default) into the project's.
- **Materials:** each library `mtllib` names read once as MaterialX networks, one item before the geometry, keyed by the names `usemtl` gives (below). Texture maps are Image nodes reading the files the library names, kept as paths, or embedded with **Embed Textures**. Libraries and maps are reached through the SDK's `Named`: in the file's folder and below, unless **Read Files Outside the Folder** is on; regular files only, never a device or a pipe; what is out of reach is left out and listed in one warning. A file held in memory names nothing on disk, so its libraries are left out, said.

## Materials

Each MTL material is a MaterialX network:

| MTL | MaterialX |
|---|---|
| classic statements: `Ka`, `Kd`, `Ks`, `Ke`, `Tf`, `Ns`, `Ni`, `d`, `illum`, `map_Ka`, `map_Kd`, `map_Ks`, `map_Ke`, `map_Ns`, `map_d`, `map_refl` | Phong Surface (`kinogaki_phong_surface`): ambient, diffuse, specular, emissive color, transmission filter, shininess, IOR, opacity, illumination model, reflection color |
| `Tr` | the Phong Surface's opacity through `invert` where no `d` or `map_d` gives it, else its Transparency Factor |
| the PBR extension: `Pr`, `Pm`, `Ps`, `Pc`, `Pcr`, `aniso`, `anisor`, `norm` and their maps | the material is a Standard Surface: `specular_roughness`, `metalness`, `sheen`, `coat`, `coat_roughness`, `specular_anisotropy`, `specular_rotation`, `normalmap` into `normal`; `Kd` its `base_color` (base 1), `Ke` its `emission_color` (emission 1), `Ni` `specular_IOR`, `d` `opacity` (a grey), `Tf` `transmission_color` |
| classic statements beside PBR ones (Blender writes both) | a Phong Surface beside the Standard Surface, read by nothing, holding `Ka`, `Ks`, `Ns`, `Tr`, `illum` and their maps |
| `bump`, `map_Bump` (`-bm`) | a `bump` of heights into the normal, `-bm` its scale; with **Bump Maps as Normal Maps** (Blender writes normal maps there) a `normalmap`. A bump map named like a normal map is said |
| `disp` | a `displacement` of heights into a Surface Material, which is shown |
| a map | its file an Image node wired into an `image` node of the input's type; times the statement's value where both are given (`multiply`) |
| `-s u v`, `-o u v` | the texture coordinates scaled, then moved (`texcoord` → `multiply` → `add`): what Blender and most readers do |
| `-clamp on` | both address modes `clamp` |
| `-imfchan r`, `g`, `b`, `m` | the channel `extract`ed from a color image, `m` the alpha of a color4 |
| `-mm base gain` | `multiply` by the gain, then `add` the base |
| `-blendu`, `-blendv`, `-boost`, `-texres`, `-t`, `-cc`, `-type`, `-imfchan l` and `z`, a texture's third coordinate, `refl`, `decal`, `sharpness` and other statements | left out, said once a material |

Colors (`Ka`, `Kd`, `Ks`, `Ke`, `Tf`) are linear Rec.709, as every format's material colors: MTL records no space. They are read into the working space, and written out of it to twelve significant digits, so a color read and written back is the number it was.

Writing goes the other way: a Phong Surface's and a Standard Surface's statements as their networks record them (a value where the input holds one, a map where an Image node feeds it, both where a `multiply` joins them), the Phong Surface beside a Standard Surface's classic statements too; a normal map as `norm` and a bump map as `map_Bump`; any other shading model translated to Standard Surface (`MaterialNetwork::translated`), an engine's own material through its standard form. What MTL can't record is left out, said. Texture paths are written relative to the library; embedded images are written beside it, in `<stem>_textures/`, once for equal bytes.

## Writing

One OBJ of every item's polygons and polylines at its placement, Y up in metres (a comment says so; OBJ records neither), `uv` and `N` as `vt` and `vn`, point `Cd` as vertex colors, `name` as `o`, `material` as `usemtl`, groups as `g`; B-reps and instances through the host's mesher. The materials item's Materials go to `<stem>.mtl` beside the file (`mtllib`), each `usemtl` naming its material by its name there.

## The reader and writer

`obj::Obj` is a plain model of what an OBJ file holds:

```rust
use kinogaki_obj::obj::{self, Obj, WriteOptions};

let model: Obj = obj::read_file("model.obj")?;
for face in 0..model.faces.len() {
    let points = model.faces.corners(face); // 0-based position indices
    let material = Obj::face_run(&model.materials, face as u32); // usemtl in effect
}
for value in &model.material_libraries {
    for name in obj::mtl::library_paths(value) {
        // try model-dir/name with obj::mtl::read_file
    }
}
obj::write_file("copy.obj", &model, &WriteOptions::default())?; // atomic
```

What it reads:

- `v x y z [w] [r g b]`, `vt u [v [w]]`, `vn x y z`;
- `f` polygons (any corner count), `l` polylines and `p` point sets, with `p`, `p/t`, `p//n`, `p/t/n` references, positive or negative (relative);
- `o`, `g`, `usemtl` and `s` as runs over the elements, and `mtllib`;
- `#` comments anywhere a token starts, CRLF, tabs and blank runs;
- MTL: `Ka/Kd/Ks/Ke/Tf`, `Ns/Ni/d/Tr/illum`, the PBR extension (`Pr/Pm/Ps/Pc/Pcr/aniso/anisor`), texture maps with their options parsed (`-o`, `-s`, `-mm`, `-clamp`, `-imfchan`, `-bm`; others kept verbatim), and any other statement verbatim.

Free-form geometry (`vp`, `curv`, `surf`...), display statements and unknown keywords are skipped and counted in `Obj::skipped`. Line continuations (`\`) are rejected explicitly.

Reading is parallel: a counting pass sizes every table once, then chunks split at line feeds parse on all cores into their own slices. Files stream through 32 MiB blocks, so reading overlaps parsing and memory holds about two blocks of text. Numbers parse bit-identically to `str::parse::<f64>`; writing uses the shortest spelling that reads back exactly, so `parse(to_bytes(obj)) == obj`. A 1M-face, 57 MiB file reads in about 13 ms and writes in about 13 ms on an M4 Max ([docs/BENCHMARKS.md](docs/BENCHMARKS.md)).

Untrusted input is bounded by the read's budget (`ReadOptions::budget`, the SDK's `ReadBudget`): each block's tables, sized from its counted records, are charged to it before they are made, a line longer than one of its allocations is refused, and its cancellation stops a read between blocks; corners stay below 2³², the offsets' width. Malformed text fails with a `Malformed` error carrying its kind, line and column, never a panic, and the first error in file order is reported whatever the thread count. The statements a file holds are read in linear time: a face's groups are its `g` run's, each run's names split once; skipped keywords are counted by a table.

Its design: [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md).

## Tests and data

- `tests/round_trip.rs`: files through the format: the standard value, units, up axis and scale, what is written reads back;
- `tests/materials.rs`: every MTL statement and option above read into its network and written back as the same statement and values, the networks checked as MaterialX takes them, textures embedded and written beside, another model written through Standard Surface, and Blender's exports (`tests/fixtures/blender`, made by Blender 5.1.2 with `make.py` there) read and written back unchanged;
- `tests/read.rs`, `write.rs`, `mtl.rs`: the reader and writer;
- `tests/fuzz.rs`: mutated files never panic (`OBJ_FUZZ=100 cargo test -p kinogaki-obj --release --test fuzz` runs it longer);
- `tests/hostile.rs`: the hostile files that once took the reader seconds to minutes, or reached outside the file's folder, read as Import reads (a 2 MiB stack and a budget): a face in 20,000 groups, 150,000 unknown keywords, a polygon of 300,000 corners, tables past the budget, libraries and maps out of the folder, a pipe, a library named 500 times;
- `tests/luce_differential.rs`: the reader against luce-obj on a corpus; `tools/luce-oracle/refresh.sh` regenerates the expected results with the Luce toolchain.

File ▸ Import and Export through the editor are Studio's workflows.

## Building

The format builds from this repository, the public studio-sdk and lib-misc-utils, checked out side by side as Studio's tree lays them out (`sdk/`, `libs/lib-misc-utils/` and `plugins/obj/`), with the compiler the SDK pins (`sdk/rust-toolchain.toml`):

```sh
cd plugins/obj
cargo test
```

`plugin/` links Studio's shared library (`kinogaki_shared`), so it is built in Studio's tree, or against a Studio release's plugin kit as the SDK's guides describe.

## License

Dual-licensed under the MIT license ([LICENSE-MIT](LICENSE-MIT)) and the Apache License 2.0 ([LICENSE-APACHE](LICENSE-APACHE)), at your option. [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md) names what comes from elsewhere.
