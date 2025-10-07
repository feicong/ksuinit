fn main() {
    // For aarch64-unknown-linux-musl target, we need to provide getauxval
    if std::env::var("TARGET").unwrap_or_default() == "aarch64-unknown-linux-musl" {
        println!("cargo:rustc-link-arg=-Wl,--defsym=getauxval=0");
    }
}
