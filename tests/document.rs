//! File ▸ Open's read: an OBJ records no units, so the Unit and Up Axis
//! options say what it is in, and the document keeps them; its items are
//! Import's read into those units.
use kinogaki_obj::Obj;
use kinogaki_studio_api::geometry::{Units, UpAxis};
use kinogaki_studio_api::params::Value;
use kinogaki_studio_api::{FileFormat, Input, Purpose};
use kinogaki_studio_api_testing::format::document;
use std::path::Path;

#[test]
fn its_unit_and_up_options_are_the_documents() {
    let fixtures = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
    let cube_path = fixtures.join("blender/cube_pbr.obj");
    let cube = Input::file(&cube_path);
    // By default an OBJ is in metres, Y up.
    let opened = document(&Obj, &cube, &Obj.options());
    assert_eq!(opened, Ok(Units::METERS));
    let mut inches = Obj.options();
    inches.insert("unit".into(), Value::Choice("in".into()));
    inches.insert("up".into(), Value::Choice("z".into()));
    let opened = document(&Obj, &cube, &inches);
    assert_eq!(opened, Ok(Units::new(0.0254, UpAxis::Z).unwrap()));
    // Scale and Embed Textures change what the file holds: File ▸ Open
    // leaves them out, reading the file as it is.
    let rows = Obj.schema(&inches, Purpose::Document);
    let names: Vec<&str> = rows.parameters().map(|p| p.name.as_str()).collect();
    assert!(
        names.contains(&"unit") && names.contains(&"up"),
        "{names:?}"
    );
    assert!(
        !names.contains(&"scale") && !names.contains(&"embed"),
        "{names:?}"
    );
    let polygons_path = fixtures.join("polygons.obj");
    let polygons = Input::file(&polygons_path);
    let read = document(&Obj, &polygons, &Obj.options());
    assert!(read.is_ok(), "{read:?}");
}
