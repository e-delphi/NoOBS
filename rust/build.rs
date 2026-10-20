// 1. Embute o ícone do app (`rust\icon.ico`, cópia própria do ícone do projeto) como recurso
//    `MAINICON`: a classe da janela, a bandeja e a hibernação o carregam por
//    nome, igual ao `LoadIcon(HInstance, 'MAINICON')` do Delphi.
// 2. Versão a partir da tag do git (o `gen-version.bat` do Delphi): criar a tag
//    É o ato de lançar, então a versão nunca fica dessincronizada. Build fora
//    de uma tag limpa carrega o sufixo do describe — e o app trata como
//    desenvolvimento (não checa atualização).
// 3. Embute a UI (`ui\`) e as traduções (`lang\`) no exe: o runtime é só o
//    NoOBS.exe e as DLLs do FFmpeg. Gera `assets.rs` no OUT_DIR com a lista
//    (caminho relativo, bytes) e um hash do conteúdo — a UI é extraída numa
//    pasta com esse nome (`assets::ui_dir`), então uma UI nova nunca é servida
//    da pasta da anterior.
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

/// FNV-1a 64: estável entre builds (o `DefaultHasher` não promete isso).
fn fnv(h: &mut u64, bytes: &[u8]) {
    for b in bytes {
        *h ^= *b as u64;
        *h = h.wrapping_mul(0x100000001b3);
    }
}

fn embed_assets() {
    let manifest = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap());
    let mut hash = 0xcbf29ce484222325u64;
    let mut code = String::new();
    for (name, var) in [("ui", "UI_FILES"), ("lang", "LANG_FILES")] {
        let root = manifest.join(name);
        println!("cargo:rerun-if-changed={}", root.display());
        let mut files = Vec::new();
        walk(&root, &root, &mut files);
        code += &format!("pub static {var}: &[(&str, &[u8])] = &[\n");
        for (rel, path) in files {
            println!("cargo:rerun-if-changed={}", path.display());
            fnv(&mut hash, rel.as_bytes());
            fnv(&mut hash, &std::fs::read(&path).unwrap());
            code += &format!("    ({rel:?}, include_bytes!({:?})),\n", path.display().to_string());
        }
        code += "];\n";
    }
    code += &format!("pub const ASSETS_HASH: &str = \"{hash:016x}\";\n");
    let out = PathBuf::from(std::env::var("OUT_DIR").unwrap()).join("assets.rs");
    std::fs::write(out, code).unwrap();
}

fn main() {
    embed_assets();
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
