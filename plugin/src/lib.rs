//! Wavefront OBJ as a Studio plugin library (`formats/obj.dylib`).
use kinogaki_studio_api::export_package;

kinogaki_shared::link!();
export_package!(kinogaki_obj::package);
