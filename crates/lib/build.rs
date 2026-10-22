// libj-sys/build.rs
fn main() {
    // TODO: Not portable
    let j_bin_path = "/Users/jamesdrake/GitHub/jsource/bin/darwin/j64arm"

    // Replace this path with the exact folder where your compiled libj file lives
    println!("cargo:rustc-link-search=native={}", j_bin_path);

    // This tells Cargo to look for "libj.so", "libj.dylib", or "j.dll"
    println!("cargo:rustc-link-lib=dylib=j");

    // ✨ Embedding RPATH tells the OS where to look at runtime during `cargo test`
    println!("cargo:rustc-link-arg=-Wl,-rpath,{}", j_bin_path);

    // Tells Cargo to rebuild this crate if the build script itself changes
    println!("cargo:rerun-if-changed=build.rs");
}
