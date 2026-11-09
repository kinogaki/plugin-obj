//! OBJ read and written: what the file holds lands in the standard value, and
//! what a value holds comes back. File ▸ Import and Export through the
//! editor are the root's workflows (`workflows/tests/workflows/obj.rs`).
use kinogaki_obj::Obj;
use kinogaki_studio_api::geometry::{Domain, Geometry, Units, Vec3, Xform, names};
use kinogaki_studio_api::{
    FileFormat, FileItem, FormatOptions, Input, Purpose, Reading, Value, Writing, standard,
};
use kinogaki_studio_api_testing::Unconverted;
use kinogaki_studio_api_testing::format::{Scratch, round_trip};

const CUBE: &str = "\
o a
v 0 0 0
v 1 0 0
v 1 1 0
v 0 1 0
vt 0 0
vt 1 0
vt 1 1
vt 0 1
vn 0 0 1
usemtl red
g top sides
f 1/1/1 2/2/1 3/3/1 4/4/1
o b
usemtl blue
g sides
f 1/1/1 3/3/1 2/2/1
l 1 2 3
";

fn read(text: &str, name: &str, set: &[(&str, Value)], units: Units) -> Geometry {
    let dir = Scratch::new("obj");
    let file = dir.join(name);
    std::fs::write(&file, text).unwrap();
    let format = Obj;
    assert_eq!(format.id(), kinogaki_obj::format());
    let mut options = format.options();
    options.extend(set.iter().map(|(k, v)| ((*k).to_owned(), v.clone())));
    let colors = Unconverted;
    let reading = Reading::new(Purpose::Value, units, &colors);
    let read = format
        .read(&Input::file(&file), &options, &reading)
        .unwrap();
    assert!(read.warnings.is_empty(), "{:?}", read.warnings);
    standard::geometry()
        .get(read.items[0].data.as_ref().unwrap())
        .unwrap()
        .clone()
}

#[test]
fn a_file_held_in_memory_reads_as_the_same_file_on_disk() {
    let on_disk = read(CUBE, "cube.obj", &[], Units::METERS);
    let colors = Unconverted;
    let reading = Reading::new(Purpose::Value, Units::METERS, &colors);
    let held = Input::memory("cube.obj", CUBE.as_bytes());
    let read = Obj.read(&held, &Obj.options(), &reading).unwrap();
    assert_eq!(read.items[0].name, "cube");
    let in_memory = standard::geometry().get(read.items[0].data.as_ref().unwrap());
    assert_eq!(in_memory, Some(&on_disk));
}

#[test]
fn a_file_lands_in_the_standard_value() {
    let value = read(CUBE, "cube.obj", &[], Units::METERS);
    let t = value.primitives();
    assert_eq!((value.positions().len(), t.len()), (4, 3));
    assert!(t.kind(0).is_polygon() && t.kind(1).is_polygon() && !t.kind(2).is_polygon());
    assert_eq!(t.primitive(1), [0, 2, 1]);
    let a = value.attributes();
    let name = a.text(Domain::Primitive, names::NAME).unwrap();
    assert_eq!(
        (name.get(0), name.get(1), name.get(2)),
        (Some("a"), Some("b"), Some("b"))
    );
    let material = a.text(Domain::Primitive, names::MATERIAL).unwrap();
    assert_eq!(material.get(0), Some("red"));
    assert_eq!(
        a.tuples::<[f32; 2]>(Domain::Vertex, names::UV).unwrap()[2],
        [1.0, 1.0]
    );
    assert_eq!(
        a.tuples::<[f32; 3]>(Domain::Vertex, names::N).unwrap()[0],
        [0.0, 0.0, 1.0]
    );
    let sides = a.group(Domain::Primitive, "sides").unwrap();
    assert_eq!(sides.iter().collect::<Vec<_>>(), [0, 1, 2]);
    let top = a.group(Domain::Primitive, "top").unwrap();
    assert_eq!(top.iter().collect::<Vec<_>>(), [0]);
}

#[test]
fn units_up_and_scale_place_the_file_in_the_project() {
    let mm = Units::MILLIMETERS;
    let value = read(CUBE, "units.obj", &[("scale", Value::Number(2.0))], mm);
    assert_eq!(value.positions()[1], [2000.0, 0.0, 0.0]);
    let value = read(CUBE, "cm.obj", &[("unit", Value::Choice("cm".into()))], mm);
    assert!((value.positions()[1][0] - 10.0).abs() < 1e-9);
    let z_up = Units::new(1.0, kinogaki_studio_api::geometry::UpAxis::Z).unwrap();
    let value = read(CUBE, "up.obj", &[], z_up);
    let y = value.positions()[3];
    assert!(
        (Vec3::from(y) - Vec3::new(0.0, 0.0, 1.0)).length() < 1e-9,
        "{y:?}"
    );
    let n = value
        .attributes()
        .tuples::<[f32; 3]>(Domain::Vertex, names::N)
        .unwrap()[0];
    assert!((n[1] + 1.0).abs() < 1e-6, "the normal turns too: {n:?}");
}

#[test]
fn what_is_written_reads_back() {
    let value = read(CUBE, "out.obj", &[], Units::METERS);
    let format = Obj;
    let writer = format.writer().expect("it writes");
    let dir = Scratch::new("obj");
    let file = dir.join("written.obj");
    let colors = Unconverted;
    let writing = Writing::new(Units::METERS, &colors);
    let mut placed = FileItem::top("thing", standard::geometry().wrap(value.clone()));
    placed.placement = Xform::translation(Vec3::new(5.0, 0.0, 0.0));
    let warnings = writer
        .write(&file, &[placed], &writing, &FormatOptions::new())
        .unwrap();
    assert!(warnings.is_empty(), "{warnings:?}");
    let back = read(
        &std::fs::read_to_string(&file).unwrap(),
        "back.obj",
        &[],
        Units::METERS,
    );
    assert_eq!(back.primitives().len(), 3);
    assert_eq!(back.positions()[1], [6.0, 0.0, 0.0], "at its placement");
    let (a, b) = (value.attributes(), back.attributes());
    for name in [names::NAME, names::MATERIAL] {
        let (x, y) = (
            a.text(Domain::Primitive, name).unwrap(),
            b.text(Domain::Primitive, name).unwrap(),
        );
        assert!(x.iter().eq(y.iter()), "{name}");
    }
    assert_eq!(
        a.tuples::<[f32; 2]>(Domain::Vertex, names::UV),
        b.tuples::<[f32; 2]>(Domain::Vertex, names::UV),
    );
    assert_eq!(
        b.group(Domain::Primitive, "top").map(|g| g.iter().count()),
        a.group(Domain::Primitive, "top").map(|g| g.iter().count())
    );
}

/// What Studio writes it reads back where it was, in any project's units, and
/// another application finds it in metres, Y up.
#[test]
fn a_file_written_in_a_projects_units_reads_back_where_it_was() {
    use kinogaki_studio_api::geometry::{Attributes, Primitives, UpAxis};
    let table = Primitives::of_polygons(3, vec![0, 3].into(), vec![0, 1, 2].into()).unwrap();
    let positions = vec![[100.0, 0.0, 0.0], [0.0, 100.0, 0.0], [0.0, 0.0, 250.0]];
    let value = Geometry::new(positions.clone().into(), table, Attributes::new()).unwrap();
    for (up, name) in [(UpAxis::Y, "y.obj"), (UpAxis::Z, "z.obj")] {
        let units = Units::new(0.001, up).unwrap();
        let dir = Scratch::new("obj");
        let file = dir.join(name);
        let item = FileItem::top("tri", standard::geometry().wrap(value.clone()));
        let (read, said) = round_trip(&Obj, (&[item], None), &file, units).unwrap();
        assert!(said.is_empty() && read.warnings.is_empty(), "{said:?}");
        let geometry = |i: &FileItem| standard::geometry().get(i.data.as_ref()?).cloned();
        let back = read.items.iter().find_map(geometry).expect("its triangle");
        for (a, b) in positions.iter().zip(back.positions().iter()) {
            assert!((0..3).all(|i| (a[i] - b[i]).abs() < 1e-3), "{a:?} {b:?}");
        }
        let text = std::fs::read_to_string(&file).unwrap();
        match up {
            UpAxis::Y => assert!(text.contains("v 0.1 0 0"), "{text}"),
            UpAxis::Z => assert!(text.contains("v 0 0.25 0"), "{text}"),
        }
    }
}

/// The face's normal by its winding: its corners' cross product.
fn winding_normal(value: &Geometry, face: usize) -> Vec3 {
    let corners = value.primitives().primitive(face);
    let at = |c: usize| Vec3::from(value.positions()[corners[c] as usize]);
    (at(1) - at(0))
        .cross(at(2) - at(0))
        .normalized()
        .expect("a face with area")
}

fn near(a: Vec3, b: Vec3) -> bool {
    (a - b).length() < 1e-9
}

/// A triangle counter-clockwise seen from the file's +Z, its `vn` +Z.
const UP_Z: &str = "\
v 0 0 0
v 1 0 0
v 0 1 0
vn 0 0 1
f 1//1 2//1 3//1
";

/// A Z-up file read into a Y-up project turns as a rotation does (its +Z
/// the project's +Y, its +Y the project's −Z): its faces wind as they did,
/// facing where its normals point. A mirroring would face them away.
#[test]
fn a_z_up_file_reads_into_a_y_up_project_turned_not_mirrored() {
    let up_z = [("up", Value::Choice("z".into()))];
    let value = read(UP_Z, "z-up.obj", &up_z, Units::METERS);
    let p = Vec3::from(value.positions()[2]);
    assert!(near(p, Vec3::new(0.0, 0.0, -1.0)), "file +Y is −Z: {p:?}");
    let n = value
        .attributes()
        .tuples::<[f32; 3]>(Domain::Vertex, names::N)
        .unwrap()[0];
    let n = Vec3::new(n[0].into(), n[1].into(), n[2].into());
    assert!(near(n, Vec3::Y), "file +Z is +Y: {n:?}");
    let wound = winding_normal(&value, 0);
    assert!(near(wound, Vec3::Y), "it faces its normal: {wound:?}");
}

/// A Y-up file read into a Z-up project turns the other way (its +Y the
/// project's +Z, its +Z the project's −Y), facing where its normals point.
#[test]
fn a_y_up_file_reads_into_a_z_up_project_turned_not_mirrored() {
    let z_up = Units::new(1.0, kinogaki_studio_api::geometry::UpAxis::Z).unwrap();
    let value = read(UP_Z, "y-up.obj", &[], z_up);
    let p = Vec3::from(value.positions()[2]);
    assert!(near(p, Vec3::Z), "file +Y is +Z: {p:?}");
    let n = value
        .attributes()
        .tuples::<[f32; 3]>(Domain::Vertex, names::N)
        .unwrap()[0];
    let n = Vec3::new(n[0].into(), n[1].into(), n[2].into());
    assert!(near(n, -Vec3::Y), "file +Z is −Y: {n:?}");
    let wound = winding_normal(&value, 0);
    assert!(near(wound, -Vec3::Y), "it faces its normal: {wound:?}");
}

/// A Z-up project's faces are written into the file's Y up turned as a
/// rotation does (the project's +Z the file's +Y, its +Y the file's −Z), so
/// another application sees them face where their normals point.
#[test]
fn a_z_up_project_writes_its_faces_turned_not_mirrored() {
    let z_up = Units::new(1.0, kinogaki_studio_api::geometry::UpAxis::Z).unwrap();
    let up_z = [("up", Value::Choice("z".into()))];
    let value = read(UP_Z, "as-is.obj", &up_z, z_up);
    let dir = Scratch::new("obj");
    let file = dir.join("turned.obj");
    let colors = Unconverted;
    let item = FileItem::top("tri", standard::geometry().wrap(value));
    let writer = Obj.writer().expect("it writes");
    let said = writer
        .write(
            &file,
            &[item],
            &Writing::new(z_up, &colors),
            &FormatOptions::new(),
        )
        .unwrap();
    assert!(said.is_empty(), "{said:?}");
    // Read as the file is, Y up in metres.
    let text = std::fs::read_to_string(&file).unwrap();
    let back = read(&text, "back.obj", &[], Units::METERS);
    let p = Vec3::from(back.positions()[2]);
    assert!(
        near(p, Vec3::new(0.0, 0.0, -1.0)),
        "+Y is the file's −Z: {p:?}"
    );
    let n = back
        .attributes()
        .tuples::<[f32; 3]>(Domain::Vertex, names::N)
        .unwrap()[0];
    let n = Vec3::new(n[0].into(), n[1].into(), n[2].into());
    assert!(near(n, Vec3::Y), "+Z is the file's +Y: {n:?}");
    let wound = winding_normal(&back, 0);
    assert!(near(wound, Vec3::Y), "it faces its normal: {wound:?}");
}
