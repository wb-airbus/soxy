fn main() {
    #[cfg(debug_assertions)]
    soxy::main(common::Level::Debug);
    #[cfg(not(debug_assertions))]
    soxy::main(common::Level::Info);
}
