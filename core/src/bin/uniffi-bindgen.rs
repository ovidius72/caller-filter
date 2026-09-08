//! Binding generator. Run by the build, never by hand, and its output is never
//! committed — checked-in bindings drift from the core in silence and fail at
//! runtime on one platform only.
fn main() {
    uniffi::uniffi_bindgen_main()
}
