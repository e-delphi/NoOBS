// 1. Embute o ícone do app (`rust\icon.ico`, cópia própria do ícone do projeto) como recurso
//    `MAINICON`: a classe da janela, a bandeja e a hibernação o carregam por
//    nome, igual ao `LoadIcon(HInstance, 'MAINICON')` do Delphi.
// 2. Versão a partir da tag do git (o `gen-version.bat` do Delphi): criar a tag
//    É o ato de lançar, então a versão nunca fica dessincronizada. Build fora
//    de uma tag limpa carrega o sufixo do describe — e o app trata como
//    desenvolvimento (não checa atualização).
use std::process::Command;

fn main() {
    println!("cargo:rerun-if-changed=noobs.rc");
    println!("cargo:rerun-if-changed=icon.ico");
    embed_resource::compile("noobs.rc", embed_resource::NONE)
        .manifest_optional()
        .expect("falha ao compilar noobs.rc (rc.exe do Windows SDK)");

    // Recalcula a versão quando o HEAD ou o índice mudam (commit, checkout,
    // arquivo alterado → `-dirty`).
    println!("cargo:rerun-if-changed=../.git/HEAD");
    println!("cargo:rerun-if-changed=../.git/index");
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
