// 1. Embute o ícone do app (`rust\icon.ico`, cópia própria do ícone do projeto) como recurso
//    `MAINICON`: a classe da janela, a bandeja e a hibernação o carregam por
//    nome, igual ao `LoadIcon(HInstance, 'MAINICON')` do Delphi.
// 2. Versão a partir da tag do git (o `gen-version.bat` do Delphi): criar a tag
//    É o ato de lançar, então a versão nunca fica dessincronizada. Build fora
//    de uma tag limpa carrega o sufixo do describe — e o app trata como
//    desenvolvimento (não checa atualização).
// 3. Embute as traduções (`lang\`) no exe: o runtime é só o NoOBS.exe e as
//    DLLs do FFmpeg. Gera `assets.rs` no OUT_DIR com a lista (caminho
//    relativo, bytes); o app lê da memória (`assets.rs`).
// 4. Compila a interface Slint (`ui\app.slint` e o que ela importa).
use std::path::{Path, PathBuf};
use std::process::Command;

fn walk(root: &Path, dir: &Path, out: &mut Vec<(String, PathBuf)>) {
    let mut entries: Vec<_> = std::fs::read_dir(dir).expect("pasta de assets").flatten().map(|e| e.path()).collect();
    entries.sort();
    for p in entries {
        if p.is_dir() {
            walk(root, &p, out);
        } else {
            let rel = p.strip_prefix(root).unwrap().to_string_lossy().replace('\\', "/");
            out.push((rel, p));
        }
    }
}

fn embed_assets() {
    let manifest = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap());
    let mut code = String::new();
    for (name, var) in [("lang", "LANG_FILES")] {
        let root = manifest.join(name);
        println!("cargo:rerun-if-changed={}", root.display());
        let mut files = Vec::new();
        walk(&root, &root, &mut files);
        code += &format!("pub static {var}: &[(&str, &[u8])] = &[\n");
        for (rel, path) in files {
            println!("cargo:rerun-if-changed={}", path.display());
            code += &format!("    ({rel:?}, include_bytes!({:?})),\n", path.display().to_string());
        }
        code += "];\n";
    }
    let out = PathBuf::from(std::env::var("OUT_DIR").unwrap()).join("assets.rs");
    std::fs::write(out, code).unwrap();
}

fn main() {
    embed_assets();
    // Os controles são desenhados à mão (iguais aos da versão web); o estilo
    // só vale pro que sobrar dos widgets padrão do Slint.
    let cfg = slint_build::CompilerConfiguration::new().with_style("fluent".into());
    slint_build::compile_with_config("ui/app.slint", cfg).expect("falha ao compilar ui/app.slint");
    println!("cargo:rerun-if-changed=noobs.rc");
    println!("cargo:rerun-if-changed=icon.ico");
    embed_resource::compile("noobs.rc", embed_resource::NONE)
        .manifest_optional()
        .expect("falha ao compilar noobs.rc (rc.exe do Windows SDK)");

    // Recalcula a versão quando o HEAD ou o índice mudam (commit, checkout,
    // arquivo alterado → `-dirty`) e quando uma TAG é criada: criar a tag no
    // commit atual não mexe em nenhum dos dois, e o exe sairia com a versão
    // de antes da tag.
    println!("cargo:rerun-if-changed=../.git/HEAD");
    println!("cargo:rerun-if-changed=../.git/index");
    println!("cargo:rerun-if-changed=../.git/refs/tags");
    println!("cargo:rerun-if-changed=../.git/packed-refs");
    let version = Command::new("git")
        .args(["describe", "--tags", "--always", "--dirty"])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_owned())
        .filter(|v| !v.is_empty())
        .unwrap_or_else(|| "dev".to_owned());
    println!("cargo:rustc-env=NOOBS_APP_VERSION={version}");
}
