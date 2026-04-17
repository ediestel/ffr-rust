fn main() {
    // For Neovim modules: Lua symbols are resolved at load time by the host process.
    // macOS requires -undefined dynamic_lookup to allow unresolved symbols in cdylibs.
    #[cfg(target_os = "macos")]
    {
        println!("cargo:rustc-cdylib-link-arg=-undefined");
        println!("cargo:rustc-cdylib-link-arg=dynamic_lookup");
    }
}
