#[path = "../../build_support/code_identity.rs"]
mod identity;
fn main() {
    identity::capture_for_cargo();
}
