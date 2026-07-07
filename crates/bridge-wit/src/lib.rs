//! layer L3 bridge-wit bindings for the normative `hydra:bridge@1.0.0` WIT world.

pub mod bindings {
    wit_bindgen::generate!({
        world: "bridge",
        path: "../../wit",
        pub_export_macro: true,
    });
}

pub use bindings::exports::hydra::bridge::adapter;
pub use bindings::hydra::bridge::host;
pub use bindings::hydra::bridge::types;

#[macro_export]
macro_rules! export_adapter {
    ($component:ident) => {
        $crate::bindings::export!($component with_types_in $crate::bindings);
    };
}
