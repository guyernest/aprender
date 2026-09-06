//! Turn Chronos weight PRESENCE into a compile-time cfg, so the weight-dependent parity
//! tests are COUNTED skips when unarmed and real tests when armed (D-18).
//!
//! `CHRONOS_MODEL_DIR` set at BUILD time and holding a `model.safetensors` => `cfg(chronos_weights)`.
//! Unset (or pointing at a directory with no weights) => the cfg is absent, and every gated test
//! carries `#[cfg_attr(not(chronos_weights), ignore = "…run `just fetch-chronos-tiny` to arm")]`,
//! which libtest reports as `N ignored` with the reason printed. That is the whole point: the
//! `println!("SKIP"); return;` style reports `0 ignored`, i.e. a silent green, which D-18 forbids.
//!
//! `cargo::rustc-check-cfg` declares the cfg so `unexpected_cfgs` (workspace lint) stays quiet
//! without listing it in the root manifest.

fn main() {
    println!("cargo::rustc-check-cfg=cfg(chronos_weights)");
    println!("cargo:rerun-if-env-changed=CHRONOS_MODEL_DIR");

    let armed = std::env::var_os("CHRONOS_MODEL_DIR").is_some_and(|dir| {
        std::path::Path::new(&dir)
            .join("model.safetensors")
            .is_file()
    });
    if armed {
        println!("cargo:rustc-cfg=chronos_weights");
    }
}
