use std::path::PathBuf;

fn main() {
    let base = PathBuf::from("tardygrada");
    println!("cargo:rerun-if-changed=tardygrada/");

    let is_linux = std::env::var("CARGO_CFG_TARGET_OS")
        .map(|os| os == "linux")
        .unwrap_or(false);

    // ── Monocypher — compiled without -pedantic (uses C extensions) ──
    let mut mono = cc::Build::new();
    mono.file(base.join("vm/monocypher.c"))
        .include(&base)
        .flag("-std=c11")
        .flag("-O2")
        .flag("-Wall")
        .warnings(false); // monocypher triggers warnings we don't own
    if is_linux {
        mono.flag("-D_DEFAULT_SOURCE");
    }
    mono.compile("monocypher");

    // ── All other C files — with -pedantic ──
    let c_files: Vec<PathBuf> = vec![
        // vm/
        "vm/memory.c",
        "vm/context.c",
        "vm/vm.c",
        "vm/crypto.c",
        "vm/message.c",
        "vm/constitution.c",
        "vm/heal.c",
        "vm/persist.c",
        "vm/semantic.c",
        // mcp/
        "mcp/json.c",
        "mcp/server.c",
        // verify/
        "verify/pipeline.c",
        "verify/decompose.c",
        "verify/preprocess.c",
        // ontology/
        "ontology/bridge.c",
        "ontology/self.c",
        "ontology/datalog.c",
        "ontology/frames.c",
        "ontology/inference.c",
        // compiler/
        "compiler/lexer.c",
        "compiler/compiler.c",
        "compiler/exec.c",
        "compiler/terraform.c",
        // terraform/
        "terraform/terraform.c",
        // coordinate/
        "coordinate/bridge.c",
    ]
    .into_iter()
    .map(|f| base.join(f))
    .collect();

    let mut build = cc::Build::new();
    for f in &c_files {
        build.file(f);
    }
    build
        .include(&base)
        .flag("-std=c11")
        .flag("-O2")
        .flag("-Wall")
        .flag("-Wextra")
        .flag("-pedantic");

    if is_linux {
        build.flag("-D_DEFAULT_SOURCE");
        build.flag("-Wno-stringop-truncation");
        build.flag("-Wno-format-truncation");
        build.flag("-Wno-stringop-overflow");
    }

    build.compile("tardygrada_vm");
}
